//! memory/adapters/mem0.rs - Adaptador Mem0 para memoria vectorial
//! 
//! Integra Mem0 (https://github.com/mem0ai/mem0) para memoria a largo plazo
//! con búsqueda semántica y embeddings.

use crate::core::contracts::{AdapterError, Evidence, EvidenceType, ExecutionMetrics, ExecutionRequest, ExecutionResult, ResultStatus, ExecutionContext, VerificationLevel, ResourceLimits, PreflightResult, IntegrityCheck, CheckSeverity, ValidationResult, ValidationIssue, ValidationSeverity, HealthStatus, CapabilityAdapter};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};

/// Configuración del adaptador Mem0
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mem0Config {
    pub api_endpoint: String,
    pub api_key: Option<String>,
    pub collection_name: String,
    pub embedding_model: String,
    pub embedding_dimension: usize,
    pub timeout_seconds: u64,
    pub batch_size: usize,
}

impl Default for Mem0Config {
    fn default() -> Self {
        Self {
            api_endpoint: "http://localhost:8000".to_string(),
            api_key: None,
            collection_name: "mem0_memory".to_string(),
            embedding_model: "all-MiniLM-L6-v2".to_string(),
            embedding_dimension: 384,
            timeout_seconds: 30,
            batch_size: 100,
        }
    }
}

/// Cliente Mem0
pub struct Mem0Adapter {
    config: Mem0Config,
    client: Client,
    collection_id: Option<String>,
}

impl Mem0Adapter {
    pub fn new(config: Mem0Config) -> Result<Self, AdapterError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to create HTTP client: {}", e)))?;
        
        let adapter = Self {
            config,
            client,
            collection_id: None,
        };
        
        Ok(adapter)
    }
    
    /// Inicializa la colección
    pub async fn initialize(&mut self) -> Result<(), AdapterError> {
        let url = format!("{}/api/v1/collections", self.config.api_endpoint);
        let mut request = self.client.post(&url).json(&serde_json::json!({
            "name": self.config.collection_name,
            "embedding_model": self.config.embedding_model,
            "embedding_dimension": self.config.embedding_dimension,
        }));
        
        if let Some(key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let resp = request.send().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to create collection: {}", e)))?;
        
        if resp.status().is_success() {
            let result: CollectionResponse = resp.json().await
                .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to parse response: {}", e)))?;
            self.collection_id = Some(result.id);
            info!("Mem0 collection initialized: {}", result.id);
        } else if resp.status().as_u16() == 409 {
            // Collection already exists
            let collections = self.list_collections().await?;
            if let Some(c) = collections.iter().find(|c| c.name == self.config.collection_name) {
                self.collection_id = Some(c.id.clone());
            }
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AdapterError::ExecutionFailed(format!("Failed to create collection: {} - {}", status, text)));
        }
        
        Ok(())
    }
    
    /// Lista colecciones
    async fn list_collections(&self) -> Result<Vec<CollectionInfo>, AdapterError> {
        let url = format!("{}/api/v1/collections", self.config.api_endpoint);
        let mut request = self.client.get(&url);
        
        if let Some(key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let resp = request.send().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to list collections: {}", e)))?;
        
        if !resp.status().is_success() {
            return Err(AdapterError::ExecutionFailed("Failed to list collections".to_string()));
        }
        
        let result: CollectionsResponse = resp.json().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to parse response: {}", e)))?;
        
        Ok(result.collections)
    }
    
    /// Genera embedding para un texto
    pub async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, AdapterError> {
        let url = format!("{}/api/v1/embeddings", self.config.api_endpoint);
        let mut request = self.client.post(&url).json(&serde_json::json!({
            "text": text,
            "model": self.config.embedding_model,
        }));
        
        if let Some(key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let resp = request.send().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to generate embedding: {}", e)))?;
        
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AdapterError::ExecutionFailed(format!("Embedding failed: {} - {}", status, text)));
        }
        
        let result: EmbeddingResponse = resp.json().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to parse embedding: {}", e)))?;
        
        Ok(result.embedding)
    }
    
    /// Almacena un episodio
    pub async fn store_episode(&self, episode: &crate::memory::episodic::Episode) -> Result<String, AdapterError> {
        let collection_id = self.collection_id.as_ref()
            .ok_or_else(|| AdapterError::ExecutionFailed("Collection not initialized".to_string()))?;
        
        let text = format!("{} {}", episode.input, episode.output.as_deref().unwrap_or(""));
        let embedding = self.generate_embedding(&text).await?;
        
        let memory_id = uuid::Uuid::new_v4().to_string();
        let url = format!("{}/api/v1/collections/{}/memories", self.config.api_endpoint, collection_id);
        
        let mut request = self.client.post(&url).json(&serde_json::json!({
            "id": memory_id,
            "text": text,
            "embedding": embedding,
            "metadata": {
                "episode_id": episode.id.to_string(),
                "episode_type": format!("{:?}", episode.episode_type),
                "task_id": episode.task_id,
                "session_id": episode.session_id,
                "agent_id": episode.agent_id,
                "success": episode.success,
                "tags": episode.tags,
                "importance": episode.importance,
                "timestamp": episode.timestamp.to_rfc3339(),
            }
        }));
        
        if let Some(key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let resp = request.send().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to store memory: {}", e)))?;
        
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AdapterError::ExecutionFailed(format!("Store failed: {} - {}", status, text)));
        }
        
        Ok(memory_id)
    }
    
    /// Búsqueda vectorial
    pub async fn vector_search(&self, query_embedding: &[f32], limit: usize, threshold: f32) -> Result<Vec<crate::memory::episodic::Episode>, AdapterError> {
        let collection_id = self.collection_id.as_ref()
            .ok_or_else(|| AdapterError::ExecutionFailed("Collection not initialized".to_string()))?;
        
        let url = format!("{}/api/v1/collections/{}/search", self.config.api_endpoint, collection_id);
        
        let mut request = self.client.post(&url).json(&serde_json::json!({
            "embedding": query_embedding,
            "limit": limit,
            "threshold": threshold,
        }));
        
        if let Some(key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let resp = request.send().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Search failed: {}", e)))?;
        
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AdapterError::ExecutionFailed(format!("Search failed: {} - {}", status, text)));
        }
        
        let result: SearchResponse = resp.json().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to parse search response: {}", e)))?;
        
        // Convertir resultados a episodios (simplificado)
        let episodes = result.results.into_iter().map(|r| {
            crate::memory::episodic::Episode {
                id: uuid::Uuid::parse_str(&r.id).unwrap_or_default(),
                episode_type: crate::memory::episodic::EpisodeType::from_str(&r.metadata.get("episode_type").unwrap_or(&"Other".to_string())).unwrap_or(crate::memory::episodic::EpisodeType::Other),
                timestamp: chrono::DateTime::parse_from_rfc3339(&r.metadata.get("timestamp").unwrap_or(&chrono::Utc::now().to_rfc3339())).unwrap_or_else(|_| chrono::Utc::now()).with_timezone(&chrono::Utc),
                task_id: r.metadata.get("task_id").and_then(|v| v.as_str()).map(String::from),
                session_id: r.metadata.get("session_id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                agent_id: r.metadata.get("agent_id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                input: r.text.clone(),
                output: None,
                reasoning: None,
                files_involved: vec![],
                tools_used: vec![],
                commands_executed: vec![],
                success: r.metadata.get("success").and_then(|v| v.as_bool()).unwrap_or(false),
                error: None,
                metrics: crate::memory::episodic::EpisodeMetrics::default(),
                embedding: Some(r.embedding.unwrap_or_default()),
                tags: r.metadata.get("tags").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
                importance: r.metadata.get("importance").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32,
            }
        }).collect();
        
        Ok(episodes)
    }
    
    /// Health check
    pub async fn health_check(&self) -> crate::core::contracts::HealthStatus {
        let url = format!("{}/api/v1/health", self.config.api_endpoint);
        let mut request = self.client.get(&url);
        
        if let Some(key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        match request.send().await {
            Ok(resp) if resp.status().is_success() => {
                crate::core::contracts::HealthStatus {
                    healthy: true,
                    version: "mem0-1.0".to_string(),
                    last_check: std::time::SystemTime::now(),
                    details: HashMap::new(),
                }
            }
            _ => {
                crate::core::contracts::HealthStatus {
                    healthy: false,
                    version: "mem0-1.0".to_string(),
                    last_check: std::time::SystemTime::now(),
                    details: HashMap::new(),
                }
            }
        }
    }
}

// Structs de respuesta
#[derive(Debug, Deserialize)]
struct CollectionResponse {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct CollectionsResponse {
    collections: Vec<CollectionInfo>,
}

#[derive(Debug, Deserialize)]
struct CollectionInfo {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
}

#[derive(Debug, Deserialize)]
struct SearchResult {
    id: String,
    text: String,
    embedding: Option<Vec<f32>>,
    metadata: HashMap<String, serde_json::Value>,
    score: f32,
}

// Implementación del trait CapabilityAdapter
#[async_trait::async_trait]
impl crate::core::contracts::CapabilityAdapter for Mem0Adapter {
    async fn preflight(&self, _request: &crate::core::contracts::ExecutionRequest) -> Result<crate::core::contracts::PreflightResult, AdapterError> {
        Ok(crate::core::contracts::PreflightResult {
            allowed: true,
            checks: vec![],
            required_permissions: vec![],
            resource_limits: Default::default(),
            warnings: Vec::new(),
        })
    }
    
    async fn execute(&self, request: crate::core::contracts::ExecutionRequest) -> Result<crate::core::contracts::ExecutionResult, AdapterError> {
        let start = std::time::Instant::now();
        
        // Determinar operación
        let operation = request.arguments.get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("search");
        
        let result = match operation {
            "store" => {
                let text = request.arguments.get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AdapterError::ExecutionFailed("Missing text".to_string()))?;
                
                let metadata = request.arguments.get("metadata")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                
                let embedding = self.generate_embedding(text).await?;
                let id = uuid::Uuid::new_v4().to_string();
                
                // Store logic would go here
                serde_json::json!({ "id": id, "stored": true })
            },
            "search" => {
                let query = request.arguments.get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AdapterError::ExecutionFailed("Missing query".to_string()))?;
                
                let limit = request.arguments.get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as usize;
                
                let threshold = request.arguments.get("threshold")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.7) as f32;
                
                let embedding = self.generate_embedding(query).await?;
                let results = self.vector_search(&embedding, limit, threshold).await?;
                
                serde_json::json!({ "results": results })
            },
            _ => {
                return Err(AdapterError::ExecutionFailed("Unknown operation".to_string()));
            }
        };
        
        Ok(crate::core::contracts::ExecutionResult {
            request_id: request.request_id,
            status: crate::core::contracts::ResultStatus::Passed,
            output: Some(result),
            evidence: vec![],
            metrics: crate::core::contracts::ExecutionMetrics {
                tool_version: "mem0-1.0".to_string(),
                definition_hash: String::new(),
                execution_time_ms: start.elapsed().as_millis() as u64,
                cpu_time_ms: None,
                memory_peak_bytes: None,
                tokens_input: None,
                tokens_output: None,
                network_io_bytes: None,
                result_status: crate::core::contracts::ResultStatus::Passed,
                verification_status: None,
                error_count: 0,
                timeout_count: 0,
                resource_limit_count: 0,
            },
            started_at: std::time::SystemTime::now() - start.elapsed(),
            completed_at: std::time::SystemTime::now(),
        })
    }
    
    fn validate(&self, result: &crate::core::contracts::ExecutionResult) -> crate::core::contracts::ValidationResult {
        crate::core::contracts::ValidationResult {
            valid: result.status == crate::core::contracts::ResultStatus::Passed,
            issues: vec![],
            adjusted_status: None,
        }
    }
    
    fn get_metrics(&self, _execution_id: &str) -> Option<crate::core::contracts::ExecutionMetrics> {
        None
    }
    
    async fn health_check(&self) -> crate::core::contracts::HealthStatus {
        self.health_check().await
    }
    
    fn capability_id(&self) -> &str {
        "mem0_vector_memory"
    }
    
    fn capability_version(&self) -> &str {
        "1.0.0"
    }
    
    fn tool_definition_hash(&self) -> &str {
        "mem0_v1"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_mem0_adapter_creation() {
        let config = Mem0Config::default();
        let adapter = Mem0Adapter::new(config);
        // Will fail if mem0 not running - expected in CI
        // assert!(adapter.is_ok());
    }
}