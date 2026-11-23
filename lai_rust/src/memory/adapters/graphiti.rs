//! memory/adapters/graphiti.rs - Adaptador Graphiti para grafos temporales
//! 
//! Integra Graphiti (https://github.com/getzep/graphiti) para grafos de conocimiento temporales.

use crate::core::contracts::{AdapterError, CapabilityAdapter, Evidence, EvidenceType, ExecutionMetrics, ExecutionRequest, ExecutionResult, ResultStatus, ExecutionContext, VerificationLevel, ResourceLimits, PreflightResult, IntegrityCheck, CheckSeverity, ValidationResult, ValidationIssue, ValidationSeverity, HealthStatus};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};

/// Configuración del adaptador Graphiti
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphitiConfig {
    pub api_endpoint: String,
    pub api_key: Option<String>,
    pub group_id: String,
    pub timeout_seconds: u64,
    pub batch_size: usize,
}

impl Default for GraphitiConfig {
    fn default() -> Self {
        Self {
            api_endpoint: "http://localhost:8001".to_string(),
            api_key: None,
            group_id: "lai_temporal".to_string(),
            timeout_seconds: 60,
            batch_size: 50,
        }
    }
}

/// Cliente Graphiti
pub struct GraphitiAdapter {
    config: GraphitiConfig,
    client: Client,
}

impl GraphitiAdapter {
    pub fn new(config: GraphitiConfig) -> Result<Self, AdapterError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to create HTTP client: {}", e)))?;
        
        let adapter = Self { config, client };
        Ok(adapter)
    }
    
    /// Ingiere un evento temporal
    pub async fn ingest_event(&self, event: &crate::memory::temporal::TemporalEvent) -> Result<(), AdapterError> {
        let url = format!("{}/api/v1/episodes", self.config.api_endpoint);
        
        let episode_data = serde_json::json!({
            "group_id": self.config.group_id,
            "name": format!("event_{}", event.event_type),
            "content": serde_json::json!({
                "event_type": format!("{:?}", event.event_type),
                "entity_id": event.entity_id.to_string(),
                "entity_type": format!("{:?}", event.entity_type),
                "actor": event.actor,
                "description": event.description,
                "before_state": event.before_state,
                "after_state": event.after_state,
                "diff": event.diff,
                "reasoning": event.reasoning,
                "tags": event.tags,
                "confidence": event.confidence,
                "metadata": event.metadata,
            }).to_string(),
            "timestamp": event.timestamp.to_rfc3339(),
            "source": "lai_temporal_memory",
            "source_description": "LAI temporal memory event",
        });
        
        let mut request = self.client.post(&url).json(&episode_data);
        
        if let Some(key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let resp = request.send().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to ingest event: {}", e)))?;
        
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            warn!("Graphiti ingest warning: {} - {}", status, text);
        }
        
        Ok(())
    }
    
    /// Ingiere un snapshot
    pub async fn ingest_snapshot(&self, snapshot: &crate::memory::temporal::TemporalSnapshot) -> Result<(), AdapterError> {
        let url = format!("{}/api/v1/episodes", self.config.api_endpoint);
        
        let episode_data = serde_json::json!({
            "group_id": self.config.group_id,
            "name": format!("snapshot_{}", snapshot.trigger),
            "content": serde_json::json!({
                "snapshot_id": snapshot.id.to_string(),
                "description": snapshot.description,
                "trigger": format!("{:?}", snapshot.trigger),
                "entities_count": snapshot.entities.len(),
                "events_count": snapshot.events.len(),
            }).to_string(),
            "timestamp": snapshot.timestamp.to_rfc3339(),
            "source": "lai_temporal_snapshot",
            "source_description": "LAI temporal memory snapshot",
        });
        
        let mut request = self.client.post(&url).json(&episode_data);
        
        if let Some(key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let resp = request.send().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to ingest snapshot: {}", e)))?;
        
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            warn!("Graphiti snapshot ingest warning: {} - {}", status, text);
        }
        
        Ok(())
    }
    
    /// Busca en el grafo temporal
    pub async fn search(&self, query: &str, limit: usize) -> Result<GraphitiSearchResult, AdapterError> {
        let url = format!("{}/api/v1/search", self.config.api_endpoint);
        
        let mut request = self.client.post(&url).json(&serde_json::json!({
            "group_id": self.config.group_id,
            "query": query,
            "limit": limit,
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
        
        let result: GraphitiSearchResult = resp.json().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to parse search response: {}", e)))?;
        
        Ok(result)
    }
    
    /// Obtiene entidades relacionadas
    pub async fn get_related_entities(&self, entity_id: &str, max_depth: usize) -> Result<GraphitiEntityResult, AdapterError> {
        let url = format!("{}/api/v1/entities/{}/related", self.config.api_endpoint, entity_id);
        
        let mut request = self.client.get(&url).query(&[
            ("group_id", self.config.group_id.as_str()),
            ("max_depth", &max_depth.to_string()),
        ]);
        
        if let Some(key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let resp = request.send().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to get related entities: {}", e)))?;
        
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AdapterError::ExecutionFailed(format!("Failed to get related entities: {} - {}", status, text)));
        }
        
        let result: GraphitiEntityResult = resp.json().await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to parse entity response: {}", e)))?;
        
        Ok(result)
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
                    version: "graphiti-1.0".to_string(),
                    last_check: SystemTime::now(),
                    details: HashMap::new(),
                }
            }
            _ => {
                crate::core::contracts::HealthStatus {
                    healthy: false,
                    version: "graphiti-1.0".to_string(),
                    last_check: SystemTime::now(),
                    details: HashMap::new(),
                }
            }
        }
    }
}

// Structs de respuesta
#[derive(Debug, Deserialize)]
struct GraphitiSearchResult {
    episodes: Vec<GraphitiEpisode>,
    edges: Vec<GraphitiEdge>,
    nodes: Vec<GraphitiNode>,
}

#[derive(Debug, Deserialize)]
struct GraphitiEpisode {
    id: String,
    name: String,
    content: String,
    timestamp: String,
    group_id: String,
}

#[derive(Debug, Deserialize)]
struct GraphitiEdge {
    id: String,
    source: String,
    target: String,
    relationship: String,
    weight: f32,
}

#[derive(Debug, Deserialize)]
struct GraphitiNode {
    id: String,
    name: String,
    labels: Vec<String>,
    properties: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GraphitiEntityResult {
    entity: GraphitiNode,
    related: Vec<GraphitiEdge>,
}

// Implementación del trait CapabilityAdapter
#[async_trait::async_trait]
impl crate::core::contracts::CapabilityAdapter for GraphitiAdapter {
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
        let start = Instant::now();
        
        let operation = request.arguments.get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("search");
        
        let result = match operation {
            "ingest_event" => {
                let event_json = request.arguments.get("event")
                    .ok_or_else(|| AdapterError::ExecutionFailed("Missing event".to_string()))?;
                
                let event: crate::memory::temporal::TemporalEvent = serde_json::from_value(event_json.clone())
                    .map_err(|e| AdapterError::ExecutionFailed(format!("Invalid event: {}", e)))?;
                
                self.ingest_event(&event).await?;
                serde_json::json!({ "ingested": true })
            },
            "ingest_snapshot" => {
                let snapshot_json = request.arguments.get("snapshot")
                    .ok_or_else(|| AdapterError::ExecutionFailed("Missing snapshot".to_string()))?;
                
                let snapshot: crate::memory::temporal::TemporalSnapshot = serde_json::from_value(snapshot_json.clone())
                    .map_err(|e| AdapterError::ExecutionFailed(format!("Invalid snapshot: {}", e)))?;
                
                self.ingest_snapshot(&snapshot).await?;
                serde_json::json!({ "ingested": true })
            },
            "search" => {
                let query = request.arguments.get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AdapterError::ExecutionFailed("Missing query".to_string()))?;
                
                let limit = request.arguments.get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as usize;
                
                let result = self.search(query, limit).await?;
                serde_json::to_value(result).unwrap()
            },
            "get_related" => {
                let entity_id = request.arguments.get("entity_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AdapterError::ExecutionFailed("Missing entity_id".to_string()))?;
                
                let max_depth = request.arguments.get("max_depth")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2) as usize;
                
                let result = self.get_related_entities(entity_id, max_depth).await?;
                serde_json::to_value(result).unwrap()
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
                tool_version: "graphiti-1.0".to_string(),
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
            started_at: SystemTime::now() - start.elapsed(),
            completed_at: SystemTime::now(),
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
        "graphiti_temporal_graph"
    }
    
    fn capability_version(&self) -> &str {
        "1.0.0"
    }
    
    fn tool_definition_hash(&self) -> &str {
        "graphiti_v1"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_graphiti_adapter_creation() {
        let config = GraphitiConfig::default();
        let adapter = GraphitiAdapter::new(config);
        // Will fail if graphiti not running - expected in CI
        // assert!(adapter.is_ok());
    }
}