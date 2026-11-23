//! memory/episodic.rs - Memoria episódica
//! 
//! Almacena episodios discretos: conversaciones, ejecuciones, decisiones, errores, soluciones.

use crate::core::contracts::{ExecutionResult, ResultStatus};
use crate::memory::adapters::Mem0Adapter;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Configuración de memoria episódica
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicConfig {
    pub storage_path: PathBuf,
    pub max_episodes: usize,
    pub retention_days: u32,
    pub enable_vector_search: bool,
    pub embedding_model: String,
}

impl Default for EpisodicConfig {
    fn default() -> Self {
        Self {
            storage_path: PathBuf::from("./data/memory/episodic"),
            max_episodes: 10000,
            retention_days: 90,
            enable_vector_search: true,
            embedding_model: "all-MiniLM-L6-v2".to_string(),
        }
    }
}

/// Tipo de episodio
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeType {
    Conversation,
    CodeGeneration,
    CodeExecution,
    Debugging,
    Verification,
    Repair,
    Review,
    Decision,
    Error,
    Insight,
}

/// Episodio individual en la memoria episódica
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: Uuid,
    pub episode_type: EpisodeType,
    pub timestamp: DateTime<Utc>,
    pub task_id: Option<String>,
    pub session_id: String,
    pub agent_id: String,
    
    // Contenido del episodio
    pub input: String,
    pub output: Option<String>,
    pub reasoning: Option<String>,
    
    // Contexto técnico
    pub files_involved: Vec<String>,
    pub tools_used: Vec<String>,
    pub commands_executed: Vec<String>,
    
    // Resultado
    pub success: bool,
    pub error: Option<String>,
    pub metrics: EpisodeMetrics,
    
    // Embeddings para búsqueda semántica
    pub embedding: Option<Vec<f32>>,
    
    // Metadatos
    pub tags: Vec<String>,
    pub importance: f32, // 0.0 - 1.0
}

/// Métricas del episodio
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EpisodeMetrics {
    pub duration_ms: u64,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub tools_called: u32,
    pub files_modified: u32,
    pub lines_changed: u32,
    pub verification_score: Option<f64>,
}

/// Índice para búsqueda rápida
#[derive(Debug, Default)]
struct EpisodeIndex {
    by_id: HashMap<Uuid, usize>,
    by_task: HashMap<String, Vec<usize>>,
    by_session: HashMap<String, Vec<usize>>,
    by_type: HashMap<EpisodeType, Vec<usize>>,
    by_tag: HashMap<String, Vec<usize>>,
    by_time: Vec<(DateTime<Utc>, usize)>, // ordenado por timestamp
}

/// Memoria episódica principal
pub struct EpisodicMemory {
    config: EpisodicConfig,
    episodes: Arc<RwLock<Vec<Episode>>>,
    index: Arc<RwLock<EpisodeIndex>>,
    mem0_adapter: Option<Mem0Adapter>,
}

impl EpisodicMemory {
    pub fn new(config: EpisodicConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.storage_path)?;
        
        let mem0_adapter = if config.enable_vector_search {
            Some(Mem0Adapter::new(crate::memory::adapters::Mem0Config {
                api_endpoint: "http://localhost:8000".to_string(),
                api_key: None,
                collection_name: "episodic_memory".to_string(),
                embedding_model: config.embedding_model.clone(),
            })?)
        } else {
            None
        };
        
        let memory = Self {
            config,
            episodes: Arc::new(RwLock::new(Vec::new())),
            index: Arc::new(RwLock::new(EpisodeIndex::default())),
            mem0_adapter,
        };
        
        memory.load()?;
        Ok(memory)
    }
    
    /// Añade un nuevo episodio
    pub async fn add(&self, mut episode: Episode) -> Result<Uuid> {
        episode.id = Uuid::new_v4();
        
        // Generar embedding si está habilitado
        if let Some(adapter) = &self.mem0_adapter {
            let text = format!("{} {}", episode.input, episode.output.as_deref().unwrap_or(""));
            episode.embedding = adapter.generate_embedding(&text).await.ok();
        }
        
        let idx = {
            let mut episodes = self.episodes.write().unwrap();
            let idx = episodes.len();
            episodes.push(episode.clone());
            idx
        };
        
        // Actualizar índices
        self.update_index(idx, &episode).await?;
        
        // Persistir
        self.persist_episode(&episode).await?;
        
        Ok(episode.id)
    }
    
    /// Busca episodios por similitud semántica
    pub async fn search_similar(&self, query: &str, limit: usize, threshold: f32) -> Result<Vec<Episode>> {
        if let Some(adapter) = &self.mem0_adapter {
            let query_embedding = adapter.generate_embedding(query).await?;
            self.vector_search(&query_embedding, limit, threshold).await
        } else {
            // Fallback: búsqueda por texto
            self.text_search(query, limit).await
        }
    }
    
    /// Busca por task_id
    pub fn get_by_task(&self, task_id: &str) -> Result<Vec<Episode>> {
        let episodes = self.episodes.read().unwrap();
        let index = self.index.read().unwrap();
        
        if let Some(indices) = index.by_task.get(task_id) {
            Ok(indices.iter().map(|&i| episodes[i].clone()).collect())
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Busca por sesión
    pub fn get_by_session(&self, session_id: &str) -> Result<Vec<Episode>> {
        let episodes = self.episodes.read().unwrap();
        let index = self.index.read().unwrap();
        
        if let Some(indices) = index.by_session.get(session_id) {
            Ok(indices.iter().map(|&i| episodes[i].clone()).collect())
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Busca por tipo de episodio
    pub fn get_by_type(&self, episode_type: EpisodeType) -> Result<Vec<Episode>> {
        let episodes = self.episodes.read().unwrap();
        let index = self.index.read().unwrap();
        
        if let Some(indices) = index.by_type.get(&episode_type) {
            Ok(indices.iter().map(|&i| episodes[i].clone()).collect())
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Busca por tags
    pub fn get_by_tag(&self, tag: &str) -> Result<Vec<Episode>> {
        let episodes = self.episodes.read().unwrap();
        let index = self.index.read().unwrap();
        
        if let Some(indices) = index.by_tag.get(tag) {
            Ok(indices.iter().map(|&i| episodes[i].clone()).collect())
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Obtiene estadísticas
    pub fn stats(&self) -> EpisodicStats {
        let episodes = self.episodes.read().unwrap();
        let index = self.index.read().unwrap();
        
        let mut type_counts = HashMap::new();
        let mut tag_counts = HashMap::new();
        let mut total_duration = 0u64;
        let mut success_count = 0;
        
        for ep in episodes.iter() {
            *type_counts.entry(ep.episode_type).or_insert(0) += 1;
            for tag in &ep.tags {
                *tag_counts.entry(tag.clone()).or_insert(0) += 1;
            }
            total_duration += ep.metrics.duration_ms;
            if ep.success { success_count += 1; }
        }
        
        EpisodicStats {
            total_episodes: episodes.len(),
            type_distribution: type_counts,
            tag_distribution: tag_counts,
            avg_duration_ms: if episodes.is_empty() { 0 } else { total_duration / episodes.len() as u64 },
            success_rate: if episodes.is_empty() { 0.0 } else { success_count as f64 / episodes.len() as f64 },
            storage_size_mb: self.estimate_storage_size(),
        }
    }
    
    async fn update_index(&self, idx: usize, episode: &Episode) -> Result<()> {
        let mut index = self.index.write().unwrap();
        
        index.by_id.insert(episode.id, idx);
        index.by_task.entry(episode.task_id.clone().unwrap_or_default()).or_default().push(idx);
        index.by_session.entry(episode.session_id.clone()).or_default().push(idx);
        index.by_type.entry(episode.episode_type).or_default().push(idx);
        
        for tag in &episode.tags {
            index.by_tag.entry(tag.clone()).or_default().push(idx);
        }
        
        index.by_time.push((episode.timestamp, idx));
        index.by_time.sort_by(|a, b| a.0.cmp(&b.0));
        
        Ok(())
    }
    
    async fn persist_episode(&self, episode: &Episode) -> Result<()> {
        let path = self.config.storage_path.join(format!("{}.json", episode.id));
        let content = serde_json::to_string_pretty(episode)?;
        std::fs::write(path, content)?;
        
        // También guardar en Mem0 si está disponible
        if let Some(adapter) = &self.mem0_adapter {
            let _ = adapter.store_episode(episode).await;
        }
        
        Ok(())
    }
    
    async fn vector_search(&self, query_embedding: &[f32], limit: usize, threshold: f32) -> Result<Vec<Episode>> {
        if let Some(adapter) = &self.mem0_adapter {
            adapter.vector_search(query_embedding, limit, threshold).await
        } else {
            Ok(Vec::new())
        }
    }
    
    async fn text_search(&self, query: &str, limit: usize) -> Result<Vec<Episode>> {
        let episodes = self.episodes.read().unwrap();
        let query_lower = query.to_lowercase();
        
        let mut results: Vec<_> = episodes.iter()
            .filter(|ep| {
                ep.input.to_lowercase().contains(&query_lower) ||
                ep.output.as_deref().unwrap_or("").to_lowercase().contains(&query_lower) ||
                ep.reasoning.as_deref().unwrap_or("").to_lowercase().contains(&query_lower) ||
                ep.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .take(limit)
            .cloned()
            .collect();
        
        // Ordenar por recencia
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        Ok(results)
    }
    
    fn load(&self) -> Result<()> {
        let storage_path = &self.config.storage_path;
        if !storage_path.exists() {
            return Ok(());
        }
        
        let mut episodes = self.episodes.write().unwrap();
        let mut index = self.index.write().unwrap();
        
        for entry in std::fs::read_dir(storage_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let content = std::fs::read_to_string(path)?;
                if let Ok(episode) = serde_json::from_str::<Episode>(&content) {
                    let idx = episodes.len();
                    episodes.push(episode.clone());
                    let _ = self.update_index(idx, &episode);
                }
            }
        }
        
        Ok(())
    }
    
    fn estimate_storage_size(&self) -> f64 {
        let episodes = self.episodes.read().unwrap();
        let total_bytes: u64 = episodes.iter()
            .map(|ep| serde_json::to_string(ep).map(|s| s.len() as u64).unwrap_or(0))
            .sum();
        total_bytes as f64 / (1024.0 * 1024.0)
    }
}

/// Estadísticas de memoria episódica
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicStats {
    pub total_episodes: usize,
    pub type_distribution: HashMap<EpisodeType, usize>,
    pub tag_distribution: HashMap<String, usize>,
    pub avg_duration_ms: u64,
    pub success_rate: f64,
    pub storage_size_mb: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_episodic_memory_creation() {
        let config = EpisodicConfig::default();
        let memory = EpisodicMemory::new(config);
        assert!(memory.is_ok());
    }
    
    #[tokio::test]
    async fn test_add_episode() {
        let config = EpisodicConfig::default();
        let memory = EpisodicMemory::new(config).unwrap();
        
        let episode = Episode {
            id: Uuid::new_v4(),
            episode_type: EpisodeType::CodeGeneration,
            timestamp: Utc::now(),
            task_id: Some("task-123".to_string()),
            session_id: "session-456".to_string(),
            agent_id: "agent-1".to_string(),
            input: "Generate a hello world function".to_string(),
            output: Some("def hello():\n    print('Hello, World!')".to_string()),
            reasoning: Some("Simple function to print hello world".to_string()),
            files_involved: vec!["hello.py".to_string()],
            tools_used: vec!["write_file".to_string()],
            commands_executed: vec![],
            success: true,
            error: None,
            metrics: EpisodeMetrics::default(),
            embedding: None,
            tags: vec!["python".to_string(), "hello-world".to_string()],
            importance: 0.8,
        };
        
        let id = memory.add(episode).await.unwrap();
        assert!(!id.is_nil());
    }
}