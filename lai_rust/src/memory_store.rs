//! memory_store.rs - 4-tier memory system with provenance, invalidation, scope-aware reranking (v5.0)
//! Principles: Provenance tracking, tool-change invalidation, scope-aware reranking, confidence scoring

use crate::telemetry::{record_policy_decision, task_span, end_task_span};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(clap::Subcommand, Debug, Clone, Serialize, Deserialize)]
pub enum CheckpointCommands {
    /// List checkpoints
    List,
    /// Show checkpoint details
    Show {
        #[arg(help = "Checkpoint ID")]
        id: String,
    },
    /// Create checkpoint
    Create {
        #[arg(help = "Description")]
        description: String,
    },
    /// Rollback to checkpoint
    Rollback {
        #[arg(help = "Checkpoint ID")]
        id: String,
    },
}

#[derive(clap::Subcommand, Debug, Clone, Serialize, Deserialize)]
pub enum MemoryCommands {
    /// List memories
    List {
        #[arg(long, help = "Memory type (semantic, episodic, working, documentary)")]
        r#type: Option<String>,
    },
    /// Search memories
    Search {
        #[arg(help = "Query")]
        query: String,
        #[arg(long, help = "Memory type")]
        r#type: Option<String>,
        #[arg(long, default_value = "10", help = "Max results")]
        limit: usize,
    },
    /// Store memory
    Store {
        #[arg(help = "Content")]
        content: String,
        #[arg(long, help = "Memory type")]
        r#type: String,
        #[arg(long, help = "Source")]
        source: String,
        #[arg(long, help = "Tags (comma-separated)")]
        tags: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MemoryType {
    Semantic,      // Facts, patterns, learned knowledge
    Episodic,      // Specific task executions, tool results
    Working,       // Current task context, temporary
    Documentary,   // Documentation, specs, decisions
}

impl MemoryType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "semantic" => Some(MemoryType::Semantic),
            "episodic" => Some(MemoryType::Episodic),
            "working" => Some(MemoryType::Working),
            "documentary" => Some(MemoryType::Documentary),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryType::Semantic => "semantic",
            MemoryType::Episodic => "episodic",
            MemoryType::Working => "working",
            MemoryType::Documentary => "documentary",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub memory_type: MemoryType,
    pub content: String,
    pub source: String,
    pub tool_name: Option<String>,
    pub tool_definition_hash: Option<String>,
    pub tags: Vec<String>,
    pub confidence: f32,           // 0.0 - 1.0
    pub relevance_score: f32,      // Computed at query time
    pub created_at: String,
    pub last_accessed: String,
    pub access_count: u32,
    pub invalidated_by: Option<String>,  // tool_definition_hash that invalidated this
    pub invalidated_at: Option<String>,
    pub task_id: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl MemoryEntry {
    pub fn new(
        memory_type: MemoryType,
        content: String,
        source: String,
        tool_name: Option<String>,
        tool_definition_hash: Option<String>,
        tags: Vec<String>,
        confidence: f32,
        task_id: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            memory_type,
            content,
            source,
            tool_name,
            tool_definition_hash,
            tags,
            confidence,
            relevance_score: 0.0,
            created_at: now.clone(),
            last_accessed: now,
            access_count: 0,
            invalidated_by: None,
            invalidated_at: None,
            task_id,
            metadata: HashMap::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.invalidated_by.is_none()
    }

    pub fn invalidate(&mut self, tool_hash: &str) {
        self.invalidated_by = Some(tool_hash.to_string());
        self.invalidated_at = Some(chrono::Utc::now().to_rfc3339());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub entry: MemoryEntry,
    pub score: f32,
    pub match_type: MatchType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MatchType {
    Exact,
    Semantic,
    Tag,
    Keyword,
    Hybrid,
}

pub struct MemoryStore {
    conn: Arc<Mutex<Connection>>,
    working_dir: PathBuf,
    embedding_dim: usize,
}

impl MemoryStore {
    pub fn new(working_dir: &Path) -> Result<Self, String> {
        let db_path = working_dir.join(".lai").join("memory.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
        let conn = Arc::new(Mutex::new(conn));

        let store = Self {
            conn,
            working_dir: working_dir.to_path_buf(),
            embedding_dim: 384,  // For local embeddings if enabled
        };

        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                memory_type TEXT NOT NULL,
                content TEXT NOT NULL,
                source TEXT NOT NULL,
                tool_name TEXT,
                tool_definition_hash TEXT,
                tags TEXT NOT NULL,  -- JSON array
                confidence REAL NOT NULL,
                relevance_score REAL NOT NULL DEFAULT 0.0,
                created_at TEXT NOT NULL,
                last_accessed TEXT NOT NULL,
                access_count INTEGER NOT NULL DEFAULT 0,
                invalidated_by TEXT,
                invalidated_at TEXT,
                task_id TEXT,
                metadata TEXT NOT NULL,  -- JSON object
                embedding BLOB  -- Optional: for semantic search
            );

            CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(memory_type);
            CREATE INDEX IF NOT EXISTS idx_memories_tool_hash ON memories(tool_definition_hash);
            CREATE INDEX IF NOT EXISTS idx_memories_task ON memories(task_id);
            CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);
            CREATE INDEX IF NOT EXISTS idx_memories_invalidated ON memories(invalidated_by);

            CREATE TABLE IF NOT EXISTS tool_definitions (
                definition_hash TEXT PRIMARY KEY,
                tool_name TEXT NOT NULL,
                input_schema TEXT NOT NULL,
                output_schema TEXT NOT NULL,
                annotations TEXT,
                provider_identity TEXT,
                pinned_version TEXT,
                scope_expiry TEXT,
                created_at TEXT NOT NULL,
                validated_at TEXT,
                is_valid INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS memory_links (
                from_id TEXT NOT NULL,
                to_id TEXT NOT NULL,
                link_type TEXT NOT NULL,  -- "causes", "references", "supersedes", "related"
                created_at TEXT NOT NULL,
                PRIMARY KEY (from_id, to_id, link_type),
                FOREIGN KEY (from_id) REFERENCES memories(id),
                FOREIGN KEY (to_id) REFERENCES memories(id)
            );

            CREATE INDEX IF NOT EXISTS idx_memory_links_from ON memory_links(from_id);
            CREATE INDEX IF NOT EXISTS idx_memory_links_to ON memory_links(to_id);
            "#,
        ).map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn store(&self, entry: MemoryEntry) -> Result<String, String> {
        let conn = self.conn.lock().unwrap();

        let tags_json = serde_json::to_string(&entry.tags).map_err(|e| e.to_string())?;
        let metadata_json = serde_json::to_string(&entry.metadata).map_err(|e| e.to_string())?;

        conn.execute(
            r#"
            INSERT INTO memories (
                id, memory_type, content, source, tool_name, tool_definition_hash,
                tags, confidence, relevance_score, created_at, last_accessed,
                access_count, invalidated_by, invalidated_at, task_id, metadata
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                entry.id,
                entry.memory_type.as_str(),
                entry.content,
                entry.source,
                entry.tool_name,
                entry.tool_definition_hash,
                tags_json,
                entry.confidence,
                entry.relevance_score,
                entry.created_at,
                entry.last_accessed,
                entry.access_count,
                entry.invalidated_by,
                entry.invalidated_at,
                entry.task_id,
                metadata_json,
            ],
        ).map_err(|e| e.to_string())?;

        Ok(entry.id)
    }

    pub fn store_many(&self, entries: Vec<MemoryEntry>) -> Result<Vec<String>, String> {
        let mut ids = Vec::new();
        for entry in entries {
            ids.push(self.store(entry)?);
        }
        Ok(ids)
    }

    pub fn search(
        &self,
        query: &str,
        memory_type: Option<MemoryType>,
        limit: usize,
        include_invalidated: bool,
    ) -> Result<Vec<SearchResult>, String> {
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from(
            "SELECT id, memory_type, content, source, tool_name, tool_definition_hash,
                    tags, confidence, relevance_score, created_at, last_accessed,
                    access_count, invalidated_by, invalidated_at, task_id, metadata
             FROM memories WHERE 1=1"
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(mt) = memory_type {
            sql.push_str(" AND memory_type = ?");
            params_vec.push(Box::new(mt.as_str().to_string()));
        }

        if !include_invalidated {
            sql.push_str(" AND invalidated_by IS NULL");
        }

        // Keyword search using SQLite FTS-like approach
        sql.push_str(" AND (content LIKE ? OR tags LIKE ?)");
        let like_pattern = format!("%{}%", query);
        params_vec.push(Box::new(like_pattern.clone()));
        params_vec.push(Box::new(like_pattern));

        sql.push_str(" ORDER BY confidence DESC, last_accessed DESC LIMIT ?");
        params_vec.push(Box::new(limit as i64));

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(&params_refs[..], |row| self.row_to_entry(row)).map_err(|e| e.to_string())?;

        let mut results = Vec::new();
        for row in rows {
            let entry = row.map_err(|e| e.to_string())?;
            let score = self.compute_relevance_score(&entry, query);
            let match_type = self.determine_match_type(&entry, query);
            results.push(SearchResult {
                entry,
                score,
                match_type,
            });
        }

        // Sort by relevance score (hybrid: confidence * keyword_match * recency)
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        Ok(results)
    }

    pub fn search_semantic(&self, query: &str, memory_type: Option<MemoryType>, limit: usize) -> Result<Vec<SearchResult>, String> {
        // For now, fall back to keyword search
        // When embeddings are enabled, this would use vector similarity
        self.search(query, memory_type, limit, false)
    }

    pub fn search_hybrid(&self, query: &str, memory_type: Option<MemoryType>, limit: usize, scope: SearchScope) -> Result<Vec<SearchResult>, String> {
        let keyword_results = self.search(query, memory_type.clone(), limit * 2, false)?;
        let semantic_results = self.search_semantic(query, memory_type, limit * 2)?;

        // Reciprocal Rank Fusion
        let mut scores: HashMap<String, (f32, MemoryEntry, MatchType)> = HashMap::new();

        for (rank, result) in keyword_results.iter().enumerate() {
            let rrf_score = 1.0 / (60.0 + rank as f32 + 1.0);
            scores.entry(result.entry.id.clone())
                .and_modify(|(s, _, _)| *s += rrf_score)
                .or_insert((rrf_score, result.entry.clone(), result.match_type.clone()));
        }

        for (rank, result) in semantic_results.iter().enumerate() {
            let rrf_score = 1.0 / (60.0 + rank as f32 + 1.0);
            scores.entry(result.entry.id.clone())
                .and_modify(|(s, _, _)| *s += rrf_score)
                .or_insert((rrf_score, result.entry.clone(), result.match_type.clone()));
        }

        let mut combined: Vec<SearchResult> = scores.into_values()
            .map(|(score, entry, match_type)| SearchResult { entry, score, match_type })
            .collect();

        // Apply scope-aware reranking
        // Per shelfmark evidence: reranking helps code but HURTS memory retrieval (-10.5 pp)
        // So we do NOT rerank for memory scope
        if scope != SearchScope::Memory {
            combined.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        } else {
            // For memory, use original order (confidence + recency)
            combined.sort_by(|a, b| {
                b.entry.confidence.partial_cmp(&a.entry.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| b.entry.last_accessed.cmp(&a.entry.last_accessed))
            });
        }

        combined.truncate(limit);
        Ok(combined)
    }
}

/// Search scope for reranking policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    Code,
    Standards,
    Memory,
}

impl MemoryStore {
    fn compute_relevance_score(&self, entry: &MemoryEntry, query: &str) -> f32 {
        let mut score = entry.confidence;

        // Keyword match boost
        let query_lower = query.to_lowercase();
        let content_lower = entry.content.to_lowercase();
        if content_lower.contains(&query_lower) {
            score += 0.3;
        }

        // Tag match boost
        for tag in &entry.tags {
            if tag.to_lowercase().contains(&query_lower) {
                score += 0.2;
                break;
            }
        }

        // Recency boost (exponential decay over 30 days)
        if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&entry.created_at) {
            let age_days = (chrono::Utc::now() - created.with_timezone(&chrono::Utc)).num_days() as f32;
            if age_days < 30.0 {
                score += 0.1 * (1.0 - age_days / 30.0);
            }
        }

        // Access frequency boost
        score += (entry.access_count as f32).ln().max(0.0) * 0.05;

        score.min(1.0)
    }

    fn determine_match_type(&self, entry: &MemoryEntry, query: &str) -> MatchType {
        let query_lower = query.to_lowercase();
        let content_lower = entry.content.to_lowercase();

        if content_lower == query_lower {
            MatchType::Exact
        } else if content_lower.contains(&query_lower) {
            MatchType::Keyword
        } else {
            for tag in &entry.tags {
                if tag.to_lowercase().contains(&query_lower) {
                    return MatchType::Tag;
                }
            }
            MatchType::Hybrid
        }
    }

    pub fn get(&self, id: &str) -> Result<Option<MemoryEntry>, String> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, memory_type, content, source, tool_name, tool_definition_hash,
                    tags, confidence, relevance_score, created_at, last_accessed,
                    access_count, invalidated_by, invalidated_at, task_id, metadata
             FROM memories WHERE id = ?"
        ).map_err(|e| e.to_string())?;

        let entry = stmt.query_row([id], |row| self.row_to_entry(row)).optional().map_err(|e| e.to_string())?;

        // Update access count
        if entry.is_some() {
            let now_str = chrono::Utc::now().to_rfc3339();
                conn.execute(
                "UPDATE memories SET access_count = access_count + 1, last_accessed = ? WHERE id = ?",
                [&now_str, id],
            ).map_err(|e| e.to_string())?;
        }

        Ok(entry)
    }

    pub fn update(&self, entry: &MemoryEntry) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();

        let tags_json = serde_json::to_string(&entry.tags).map_err(|e| e.to_string())?;
        let metadata_json = serde_json::to_string(&entry.metadata).map_err(|e| e.to_string())?;

        conn.execute(
            r#"
            UPDATE memories SET
                content = ?, source = ?, tool_name = ?, tool_definition_hash = ?,
                tags = ?, confidence = ?, relevance_score = ?, last_accessed = ?,
                access_count = ?, invalidated_by = ?, invalidated_at = ?, metadata = ?
            WHERE id = ?
            "#,
            params![
                entry.content,
                entry.source,
                entry.tool_name,
                entry.tool_definition_hash,
                tags_json,
                entry.confidence,
                entry.relevance_score,
                entry.last_accessed,
                entry.access_count,
                entry.invalidated_by,
                entry.invalidated_at,
                metadata_json,
                entry.id,
            ],
        ).map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn invalidate_by_tool_hash(&self, tool_definition_hash: &str) -> Result<u64, String> {
        let conn = self.conn.lock().unwrap();

        let now = chrono::Utc::now().to_rfc3339();
        let rows = conn.execute(
            "UPDATE memories SET invalidated_by = ?, invalidated_at = ? WHERE tool_definition_hash = ? AND invalidated_by IS NULL",
            [tool_definition_hash, &now, tool_definition_hash],
        ).map_err(|e| e.to_string())?;

        Ok(rows as u64)
    }

    pub fn register_tool_definition(&self, tool_def: ToolDefinition) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();

        let input_schema = serde_json::to_string(&tool_def.input_schema).map_err(|e| e.to_string())?;
        let output_schema = serde_json::to_string(&tool_def.output_schema).map_err(|e| e.to_string())?;
        let annotations = tool_def.annotations.map(|a| serde_json::to_string(&a).unwrap_or_default());

        conn.execute(
            r#"
            INSERT OR REPLACE INTO tool_definitions (
                definition_hash, tool_name, input_schema, output_schema, annotations,
                provider_identity, pinned_version, scope_expiry, created_at, validated_at, is_valid
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)
            "#,
            params![
                tool_def.definition_hash,
                tool_def.tool_name,
                input_schema,
                output_schema,
                annotations,
                tool_def.provider_identity,
                tool_def.pinned_version,
                tool_def.scope_expiry,
                chrono::Utc::now().to_rfc3339(),
                chrono::Utc::now().to_rfc3339(),
            ],
        ).map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn verify_tool_definition(&self, definition_hash: &str) -> Result<bool, String> {
        let conn = self.conn.lock().unwrap();

        let is_valid: bool = conn.query_row(
            "SELECT is_valid FROM tool_definitions WHERE definition_hash = ?",
            [definition_hash],
            |row| row.get(0),
        ).optional().map_err(|e| e.to_string())?.unwrap_or(false);

        Ok(is_valid)
    }

    pub fn list_memories(&self, memory_type: Option<MemoryType>, limit: usize) -> Result<Vec<MemoryEntry>, String> {
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from(
            "SELECT id, memory_type, content, source, tool_name, tool_definition_hash,
                    tags, confidence, relevance_score, created_at, last_accessed,
                    access_count, invalidated_by, invalidated_at, task_id, metadata
             FROM memories WHERE invalidated_by IS NULL"
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(mt) = memory_type {
            sql.push_str(" AND memory_type = ?");
            params_vec.push(Box::new(mt.as_str().to_string()));
        }

        sql.push_str(" ORDER BY last_accessed DESC LIMIT ?");
        params_vec.push(Box::new(limit as i64));

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(&params_refs[..], |row| self.row_to_entry(row)).map_err(|e| e.to_string())?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|e| e.to_string())?);
        }

        Ok(entries)
    }

    pub fn cleanup_old_working_memories(&self, max_age_days: i64) -> Result<u64, String> {
        let conn = self.conn.lock().unwrap();

        let cutoff = chrono::Utc::now() - chrono::Duration::days(max_age_days);
        let cutoff_str = cutoff.to_rfc3339();

        let rows = conn.execute(
            "DELETE FROM memories WHERE memory_type = 'working' AND created_at < ?",
            [cutoff_str],
        ).map_err(|e| e.to_string())?;

        Ok(rows as u64)
    }

    fn row_to_entry(&self, row: &Row) -> rusqlite::Result<MemoryEntry> {
        let memory_type_str: String = row.get(1)?;
        let tags_json: String = row.get(6)?;
        let metadata_json: String = row.get(15)?;

        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        let metadata: HashMap<String, serde_json::Value> = serde_json::from_str(&metadata_json).unwrap_or_default();

        Ok(MemoryEntry {
            id: row.get(0)?,
            memory_type: MemoryType::from_str(&memory_type_str).unwrap_or(MemoryType::Semantic),
            content: row.get(2)?,
            source: row.get(3)?,
            tool_name: row.get(4)?,
            tool_definition_hash: row.get(5)?,
            tags,
            confidence: row.get(7)?,
            relevance_score: row.get(8)?,
            created_at: row.get(9)?,
            last_accessed: row.get(10)?,
            access_count: row.get(11)?,
            invalidated_by: row.get(12)?,
            invalidated_at: row.get(13)?,
            task_id: row.get(14)?,
            metadata,
        })
    }

    pub fn add_link(&self, from_id: &str, to_id: &str, link_type: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();

        let now_str = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO memory_links (from_id, to_id, link_type, created_at) VALUES (?, ?, ?, ?)",
            [from_id, to_id, link_type, &now_str],
        ).map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn get_links(&self, id: &str) -> Result<Vec<(String, String, String)>, String> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT from_id, to_id, link_type FROM memory_links WHERE from_id = ? OR to_id = ?"
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map([id, id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        }).map_err(|e| e.to_string())?;

        let mut links = Vec::new();
        for row in rows {
            links.push(row.map_err(|e| e.to_string())?);
        }

        Ok(links)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub definition_hash: String,
    pub tool_name: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub annotations: Option<serde_json::Value>,
    pub provider_identity: String,
    pub pinned_version: String,
    pub scope_expiry: String,  // RFC3339
}

impl ToolDefinition {
    pub fn compute_hash(
        tool_name: &str,
        input_schema: &serde_json::Value,
        output_schema: &serde_json::Value,
        annotations: &Option<serde_json::Value>,
    ) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(tool_name.as_bytes());
        hasher.update(serde_json::to_vec(input_schema).unwrap_or_default());
        hasher.update(serde_json::to_vec(output_schema).unwrap_or_default());
        if let Some(a) = annotations {
            hasher.update(serde_json::to_vec(a).unwrap_or_default());
        }
        hex::encode(hasher.finalize())
    }
}