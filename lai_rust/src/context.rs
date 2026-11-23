//! ContextEngine - Ingeniería de contexto con presupuesto de tokens (Regla 3).
use crate::repomap::RepoMap;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextPriority { Critical = 0, High = 1, Medium = 2, Low = 3 }

impl ContextPriority {
    pub fn rank(self) -> u8 {
        match self {
            ContextPriority::Critical => 0,
            ContextPriority::High => 1,
            ContextPriority::Medium => 2,
            ContextPriority::Low => 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContextItem {
    pub id: String,
    pub priority: ContextPriority,
    pub content: String,
    pub source: String,
    pub token_estimate: u32,
    pub relevance_score: f32,
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct TaskContext {
    pub task_id: String,
    pub objective: String,
    pub items: Vec<ContextItem>,
    pub total_tokens: u32,
    pub token_budget: u32,
    pub files_referenced: Vec<String>,
    pub symbols_referenced: Vec<String>,
    pub generated_at: String,
}

pub struct ContextEngine {
    working_dir: PathBuf,
    repo_map: Option<RepoMap>,
    loaded_files: HashMap<String, String>,
    default_token_budget: u32,
    response_reserve: u32,
}

impl ContextEngine {
    pub fn new(working_dir: PathBuf) -> Result<Self, std::io::Error> {
        Ok(Self {
            working_dir,
            repo_map: None,
            loaded_files: HashMap::new(),
            default_token_budget: 8000,
            response_reserve: 2000,
        })
    }

    pub fn load_repo_map(&mut self, root: &std::path::Path) {
        self.repo_map = Some(RepoMap::build(root));
    }

    pub fn build_context(&mut self, objective: &str, token_budget: u32) -> TaskContext {
        let mut items = Vec::new();
        let mut total_tokens = 0;

        items.push(ContextItem {
            id: "objective".into(),
            priority: ContextPriority::Critical,
            content: objective.to_string(),
            source: "user_request".into(),
            token_estimate: objective.len() as u32 / 4,
            relevance_score: 1.0,
            metadata: HashMap::new(),
        });
        total_tokens += objective.len() as u32 / 4;

        if let Some(ref repo_map) = self.repo_map {
            let map_text = repo_map.to_text(4000);
            if total_tokens + map_text.len() as u32 / 4 <= token_budget {
                items.push(ContextItem {
                    id: "repo_map".into(),
                    priority: ContextPriority::High,
                    content: map_text.clone(),
                    source: "repo_map".into(),
                    token_estimate: map_text.len() as u32 / 4,
                    relevance_score: 0.9,
                    metadata: HashMap::new(),
                });
                total_tokens += map_text.len() as u32 / 4;
            }
        }

        TaskContext {
            task_id: format!("task-{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()),
            objective: objective.to_string(),
            items,
            total_tokens,
            token_budget,
            files_referenced: Vec::new(),
            symbols_referenced: Vec::new(),
            generated_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn truncate_to_budget(&mut self, context: &mut TaskContext) {
        context.items.sort_by(|a, b| {
            a.priority.rank().cmp(&b.priority.rank())
                .then_with(|| b.relevance_score.partial_cmp(&a.relevance_score).unwrap_or(std::cmp::Ordering::Equal))
        });
        let mut kept = Vec::new();
        let mut used = 0;
        for item in context.items.drain(..) {
            if used + item.token_estimate <= context.token_budget {
                used += item.token_estimate;
                kept.push(item);
            }
        }
        context.items = kept;
        context.total_tokens = used;
    }

    pub fn estimate_tokens(text: &str) -> u32 {
        (text.len() / 4) as u32
    }
}

impl Default for ContextEngine {
    fn default() -> Self {
        Self::new(std::env::current_dir().unwrap_or_default()).unwrap()
    }
}
