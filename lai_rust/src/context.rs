//! context.rs - Context Engineering (v5.0).
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextPriority { Critical = 0, High = 1, Medium = 2, Low = 3 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    pub id: String, pub priority: ContextPriority, pub content: String,
    pub source: String, pub token_estimate: u32, pub relevance_score: f32,
    pub metadata: HashMap<String, serde_json::Value>, pub verified: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub task_id: String, pub objective: String, pub items: Vec<ContextItem>,
    pub total_tokens: u32, pub token_budget: u32,
    pub files_referenced: Vec<String>, pub symbols_referenced: Vec<String>,
    pub generated_at: String, pub omissions: Vec<String>,
}
pub struct ContextEngine {
    working_dir: PathBuf,
    repo_map: Arc<RwLock<Option<crate::repomap::RepoMap>>>,
    loaded_files: Arc<RwLock<HashMap<String, String>>>,
    default_token_budget: u32, response_reserve: u32,
}
impl ContextEngine {
    pub fn new(working_dir: PathBuf) -> Result<Self, std::io::Error> {
        Ok(Self { working_dir, repo_map: Arc::new(RwLock::new(None)), loaded_files: Arc::new(RwLock::new(HashMap::new())), default_token_budget: 8000, response_reserve: 2000 })
    }
    pub async fn build_repo_map(&self, dir: &Path) -> Result<crate::repomap::RepoMap, std::io::Error> {
        let builder = crate::repomap::RepoMapBuilder::new(dir.to_str().unwrap_or("."));
        let map = builder.build()?;
        *self.repo_map.write().unwrap() = Some(map.clone());
        Ok(map)
    }
    pub async fn add_files(&self, files: &[String]) -> Result<(), std::io::Error> {
        let mut loaded = self.loaded_files.write().unwrap();
        for file_path in files {
            let full_path = if Path::new(file_path).is_absolute() { PathBuf::from(file_path) } else { self.working_dir.join(file_path) };
            if let Ok(content) = std::fs::read_to_string(&full_path) { loaded.insert(file_path.to_string(), content); }
        }
        Ok(())
    }
    pub async fn build_context(&self, task_id: &str, objective: &str, token_budget: Option<u32>) -> Result<TaskContext, std::io::Error> {
        let budget = token_budget.unwrap_or(self.default_token_budget);
        let repo_map = self.repo_map.read().unwrap().clone();
        let loaded = self.loaded_files.read().unwrap();
        let mut items: Vec<ContextItem> = Vec::new();
        items.push(ContextItem { id: "objective".to_string(), priority: ContextPriority::Critical, content: format!("Objetivo: {}\n", objective), source: "task".to_string(), token_estimate: estimate_tokens(objective) as u32, relevance_score: 1.0, metadata: HashMap::new(), verified: false });
        if let Some(ref rm) = repo_map { let summary = rm.summary(); items.push(ContextItem { id: "repo_summary".to_string(), priority: ContextPriority::High, content: summary, source: "repomap".to_string(), token_estimate: estimate_tokens(&summary) as u32, relevance_score: 0.9, metadata: HashMap::new(), verified: false }); }
        let mut file_items: Vec<ContextItem> = loaded.iter().map(|(path, content)| { let tokens = estimate_tokens(content) as u32; ContextItem { id: format!("file:{}", path), priority: ContextPriority::Medium, content: format!("--- Archivo: {} ---\n{}\n", path, content), source: "file".to_string(), token_estimate: tokens, relevance_score: 0.7, metadata: HashMap::new(), verified: false } }).collect();
        file_items.sort_by_key(|i| i.token_estimate);
        items.extend(file_items);
        let (selected, total_tokens, files_referenced, omissions) = self.select_items_by_budget(items, budget)?;
        Ok(TaskContext { task_id: task_id.to_string(), objective: objective.to_string(), items: selected, total_tokens, token_budget: budget, files_referenced, symbols_referenced: Vec::new(), generated_at: chrono::Utc::now().to_rfc3339(), omissions })
    }
    pub async fn current_context(&self) -> Result<String, std::io::Error> {
        let loaded = self.loaded_files.read().unwrap(); let mut parts = Vec::new();
        for (path, content) in loaded.iter() { parts.push(format!("--- {} ---\n{}", path, content)); }
        Ok(parts.join("\n\n"))
    }
    pub fn summary(&self) -> String {
        let loaded = self.loaded_files.read().unwrap(); let repo_map = self.repo_map.read().unwrap();
        let total_tokens: u32 = loaded.values().map(|c| estimate_tokens(c) as u32).sum();
        format!("Loaded files: {}, Total tokens: {}, RepoMap: {}", loaded.len(), total_tokens, if repo_map.is_some() { "built" } else { "not built" })
    }
    fn select_items_by_budget(&self, mut items: Vec<ContextItem>, budget: u32) -> Result<(Vec<ContextItem>, u32, Vec<String>, Vec<String>), std::io::Error> {
        items.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| b.relevance_score.partial_cmp(&a.relevance_score).unwrap_or(std::cmp::Ordering::Equal)));
        let mut selected = Vec::new(); let mut total_tokens = 0u32; let mut files_referenced = Vec::new(); let mut omissions = Vec::new();
        for item in items {
            if total_tokens + item.token_estimate > budget { omissions.push(format!("{} (tokens: {}, priority: {:?})", item.id, item.token_estimate, item.priority)); continue; }
            total_tokens += item.token_estimate;
            if item.source == "file" { if let Some(path) = item.id.strip_prefix("file:") { files_referenced.push(path.to_string()); } }
            selected.push(item);
        }
        Ok((selected, total_tokens, files_referenced, omissions))
    }
}
pub fn estimate_tokens(text: &str) -> usize { text.chars().count() / 3 }
pub fn truncate_to_tokens(text: &str, max_tokens: u32) -> String {
    let max_chars = max_tokens as usize * 3;
    if text.len() <= max_chars { text.to_string() } else {
        let mut truncated = text[..max_chars.min(text.len())].to_string();
        if let Some(last_nl) = truncated.rfind('\n') { truncated = truncated[..last_nl].to_string(); }
        truncated.push_str("\n...[contenido truncado]...");
        truncated
    }
}
