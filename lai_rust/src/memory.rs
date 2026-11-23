//! memory.rs - MemoryStore persistente (v4.0, Mejora 9).
//! SQLite + FTS5 local (sin servicios externos, Regla 0). Cuatro categorías:
//! semántica, episódica, de trabajo y documental. Cada entrada conserva
//! procedencia (origen), confianza, vigencia e `invalidated_by`.
//!
//! Regla v4.0: un cambio de `definition_hash` de una herramienta es un evento
//! de invalidación de primer nivel: la memoria episódica que dependa de esa
//! herramienta se marca como potencialmente inválida (no se borra).

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

/// Entrada de memoria con procedencia.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub kind: String,
    pub project: String,
    pub source: String,
    pub confidence: f64,
    pub content: String,
    pub metadata: String,
    pub created_at: String,
}

impl MemoryEntry {
    pub fn semantic(project: &str, content: &str) -> Self {
        Self {
            kind: "semantic".to_string(),
            project: project.to_string(),
            source: "distillation".to_string(),
            confidence: 0.7,
            content: content.to_string(),
            metadata: "{}".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn episodic(task_id: &str, content: &str, tool_hash: Option<&str>) -> Self {
        let metadata = tool_hash
            .map(|h| format!("{{\"tool_hash\": \"{}\"}}", h))
            .unwrap_or_else(|| "{}".to_string());
        Self {
            kind: "episodic".to_string(),
            project: "global".to_string(),
            source: task_id.to_string(),
            confidence: 0.8,
            content: content.to_string(),
            metadata,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Almacén de memoria con respaldo SQLite + FTS5.
pub struct MemoryStore {
    conn: Mutex<Connection>,
}

impl MemoryStore {
    /// Abre (o crea) el almacén en `path` y aplica el esquema.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS memories (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 kind TEXT NOT NULL,
                 project TEXT NOT NULL,
                 source TEXT NOT NULL,
                 confidence REAL NOT NULL DEFAULT 0.5,
                 content TEXT NOT NULL,
                 metadata TEXT NOT NULL DEFAULT '{}',
                 created_at TEXT NOT NULL,
                 valid_until TEXT,
                 invalidated_by TEXT
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts
                 USING fts5(content, content='memories', content_rowid='id');
             CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                 INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content);
             END;
             CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                 INSERT INTO memories_fts(memories_fts, rowid, content) VALUES('delete', old.id, old.content);
             END;",
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Almacena una entrada.
    pub fn remember(&self, entry: &MemoryEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO memories (kind, project, source, confidence, content, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.kind,
                entry.project,
                entry.source,
                entry.confidence,
                entry.content,
                entry.metadata,
                entry.created_at
            ],
        )?;
        Ok(())
    }

    /// Recupera las k entradas más relevantes (FTS5 BM25), sin invalidadas.
    pub fn recall(&self, query: &str, project: Option<&str>, k: usize) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().unwrap();
        // Saneo FTS5: cada token alfanumérico entre comillas (evita que
        // '-', ':' u operadores se interpreten como sintaxis de consulta).
        let tokens: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(|t| format!("\"{}\"", t))
            .collect();
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let pattern = tokens.join(" ");

        let mut sql = String::from(
            "SELECT m.id, m.kind, m.project, m.source, m.confidence, m.content, m.metadata, m.created_at \
             FROM memories_fts f JOIN memories m ON m.id = f.rowid \
             WHERE memories_fts MATCH ? AND m.invalidated_by IS NULL",
        );
        let mut values: Vec<String> = vec![pattern];
        if let Some(p) = project {
            sql.push_str(" AND m.project = ?");
            values.push(p.to_string());
        }
        sql.push_str(" ORDER BY rank LIMIT ?");
        values.push(k.to_string());

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            Ok(MemoryEntry {
                kind: row.get(1)?,
                project: row.get(2)?,
                source: row.get(3)?,
                confidence: row.get(4)?,
                content: row.get(5)?,
                metadata: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Marca como potencialmente inválidas las entradas cuya metadata
    /// contenga `old_hash` (herramienta cambiada), registrando `new_hash`.
    /// Devuelve el número de entradas invalidadas.
    pub fn invalidate_by_tool_hash(&self, old_hash: &str, new_hash: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE memories SET invalidated_by = ?1 \
             WHERE invalidated_by IS NULL AND metadata LIKE '%' || ?2 || '%'",
            params![new_hash, old_hash],
        )?;
        Ok(changed)
    }

    /// Destila una trayectoria completada en memoria episódica.
    pub fn distill(&self, task_id: &str, outcome: &str, summary: &str, tool_hash: Option<&str>) -> Result<()> {
        let entry = MemoryEntry::episodic(
            task_id,
            &format!("task={} outcome={} summary={}", task_id, outcome, summary),
            tool_hash,
        );
        self.remember(&entry)
    }

    /// Número de entradas almacenadas.
    pub fn len(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (MemoryStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("lai-memory-test-{}", uuid::Uuid::new_v4()));
        let store = MemoryStore::open(&dir.join("memory.db")).unwrap();
        (store, dir)
    }

    #[test]
    fn test_remember_and_recall() {
        let (store, dir) = store();
        store
            .remember(&MemoryEntry::semantic("lai", "el modulo policy implementa guardrails deterministas"))
            .unwrap();
        store
            .remember(&MemoryEntry::semantic("lai", "el repositorio usa arquitectura de capas"))
            .unwrap();
        assert_eq!(store.len().unwrap(), 2);

        let results = store.recall("guardrails", None, 5).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("policy"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_invalidation_by_tool_hash() {
        let (store, dir) = store();
        store
            .remember(&MemoryEntry::episodic("t1", "write_file produjo X", Some("hash-v1")))
            .unwrap();
        store
            .remember(&MemoryEntry::episodic("t2", "read_file produjo Y", Some("hash-read")))
            .unwrap();

        let invalidated = store.invalidate_by_tool_hash("hash-v1", "hash-v2").unwrap();
        assert_eq!(invalidated, 1);

        let results = store.recall("write_file", None, 5).unwrap();
        assert!(results.is_empty(), "la entrada invalidada no debe recuperarse");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_distill() {
        let (store, dir) = store();
        store.distill("task-1", "success", "se arreglo el bug X", None).unwrap();
        let results = store.recall("task-1", None, 5).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].kind == "episodic");
        let _ = std::fs::remove_dir_all(&dir);
    }
}