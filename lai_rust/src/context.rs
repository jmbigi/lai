//! context.rs - Context Engineering: selección y presupuesto de contexto.
//! Compone el contexto de cada paso del plan priorizando la evidencia
//! reciente de ejecución/verificación sobre inferencias almacenadas.

use crate::harness::{ExecutionContext, PlanStep};
use crate::repomap::{build_repo_map, RepoMap};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Contexto de trabajo de una tarea.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Archivos incluidos en el contexto (rutas relativas).
    pub files: Vec<String>,
    /// Hallazgos del INSPECT (notas estructurales).
    pub notes: Vec<String>,
    /// Presupuesto de tokens del contexto.
    pub budget_tokens: usize,
}

#[derive(Debug)]
struct ContextInner {
    working_dir: PathBuf,
    state: RwLock<Context>,
    repo_map: RwLock<Option<RepoMap>>,
}

/// Motor de contexto. `Clone` para que `ExecutionContext` pueda clonarlo.
#[derive(Debug, Clone)]
pub struct ContextEngine {
    inner: Arc<ContextInner>,
}

impl ContextEngine {
    pub fn new(working_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(ContextInner {
                working_dir,
                state: RwLock::new(Context::default()),
                repo_map: RwLock::new(None),
            }),
        })
    }

    /// Construye (o actualiza) el RepoMap del directorio indicado.
    pub async fn build_repo_map(&self, dir: &Path) -> Result<RepoMap> {
        let map = build_repo_map(dir)?;
        let mut guard = self.inner.repo_map.write().unwrap();
        *guard = Some(map.clone());
        Ok(map)
    }

    /// Añade archivos relevantes al contexto de trabajo (deduplicados).
    pub async fn add_files(&self, files: &[String]) -> Result<()> {
        let mut st = self.inner.state.write().unwrap();
        for f in files {
            if !st.files.contains(f) {
                st.files.push(f.clone());
            }
        }
        Ok(())
    }

    /// Añade una nota de inspección.
    pub async fn add_note(&self, note: String) -> Result<()> {
        let mut st = self.inner.state.write().unwrap();
        st.notes.push(note);
        Ok(())
    }

    /// Snapshot JSON del RepoMap para checkpoints (v4.0: el checkpoint debe
    /// incluir el estado del índice para poder comparar y recuperarse).
    pub fn repo_map_snapshot(&self) -> serde_json::Value {
        let map_guard = self.inner.repo_map.read().unwrap();
        match map_guard.as_ref() {
            Some(map) => serde_json::json!({
                "indexed": true,
                "root": map.root.to_string_lossy(),
                "summary": map.summary(),
                "files": map.files.iter().map(|f| serde_json::json!({
                    "path": f.path,
                    "language": f.language,
                    "hash": f.hash,
                    "size_bytes": f.size_bytes,
                    "symbols": f.symbols.iter().map(|s| format!("{}:{}", s.kind, s.name)).collect::<Vec<_>>(),
                    "imports": f.imports,
                })).collect::<Vec<_>>(),
            }),
            None => serde_json::json!({ "indexed": false }),
        }
    }

    /// Resumen compacto del contexto para prompts de planificación.
    pub fn summary(&self) -> String {
        let st = self.inner.state.read().unwrap();
        let map_guard = self.inner.repo_map.read().unwrap();
        let mut out = String::new();
        if let Some(map) = map_guard.as_ref() {
            out.push_str(&map.summary());
        } else {
            out.push_str("RepoMap: (no indexado)");
        }
        out.push_str(&format!("\nContexto de trabajo: {} archivos", st.files.len()));
        if !st.files.is_empty() {
            out.push_str(&format!("\nArchivos: {}", st.files.join(", ")));
        }
        if !st.notes.is_empty() {
            out.push_str(&format!("\nNotas: {}", st.notes.join(" | ")));
        }
        out
    }

    /// Construye el prompt de contexto para un paso del plan.
    pub async fn build_context_for_step(&self, ctx: &ExecutionContext, step: &PlanStep) -> Result<String> {
        let mut parts = Vec::new();
        parts.push(format!(
            "Task: {} ({})\nObjective: {}\nAcceptance criteria: {}\n\nCurrent step: {}\nDescription: {}\nExpected outcome: {}",
            ctx.task_id,
            ctx.task_spec.task_type,
            ctx.task_spec.objective,
            ctx.task_spec.acceptance_criteria.join("; "),
            step.id,
            step.description,
            step.expected_outcome
        ));

        let st = self.inner.state.read().unwrap();
        if !st.files.is_empty() {
            parts.push(format!("Relevant files: {}", st.files.join(", ")));
        }
        if !st.notes.is_empty() {
            parts.push(format!("Notes: {}", st.notes.join(" | ")));
        }
        drop(st);

        let map_guard = self.inner.repo_map.read().unwrap();
        if let Some(map) = map_guard.as_ref() {
            if !step.description.is_empty() {
                let candidates = map.find_files(&step.description);
                if !candidates.is_empty() {
                    parts.push(format!("Candidate files (RepoMap): {}", candidates.join(", ")));
                }
            }
        }
        drop(map_guard);

        // Presupuesto: recorte conservador por longitud total.
        let budget = {
            let st = self.inner.state.read().unwrap();
            if st.budget_tokens > 0 { st.budget_tokens } else { 6000 }
        };
        let joined = parts.join("\n\n");
        let max_chars = budget * 4;
        if joined.len() > max_chars {
            let mut end = max_chars;
            while end > 0 && !joined.is_char_boundary(end) {
                end -= 1;
            }
            Ok(format!("{}... [truncado por presupuesto]", &joined[..end]))
        } else {
            Ok(joined)
        }
    }

    /// Contexto actual completo (para verificación/reparación).
    pub async fn current_context(&self) -> Result<String> {
        Ok(self.summary())
    }

    /// Snapshot serializable del contexto (para checkpoints).
    pub async fn snapshot(&self) -> Result<serde_json::Value> {
        let st = self.inner.state.read().unwrap();
        Ok(serde_json::json!({
            "files": st.files,
            "notes": st.notes,
            "budget_tokens": st.budget_tokens,
            "working_dir": self.inner.working_dir.to_string_lossy(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_files_dedup() {
        let engine = ContextEngine::new(std::env::current_dir().unwrap()).unwrap();
        engine.add_files(&["a.py".to_string(), "a.py".to_string(), "b.py".to_string()]).await.unwrap();
        let st = engine.inner.state.read().unwrap();
        assert_eq!(st.files.len(), 2);
    }

    #[test]
    fn test_summary_without_map() {
        let engine = ContextEngine::new(std::env::current_dir().unwrap()).unwrap();
        assert!(engine.summary().contains("no indexado"));
    }
}