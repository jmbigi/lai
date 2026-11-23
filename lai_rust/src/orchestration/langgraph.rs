/// LangGraph Orchestration Adapter for lai
/// 
/// Provides integration with LangGraph for stateful graph-based orchestration,
/// enabling complex workflows with checkpoints, retries, and human-in-the-loop.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

/// LangGraph node types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LangGraphNodeType {
    /// LLM call node
    LlmCall,
    /// Tool execution node
    ToolCall,
    /// Conditional branch node
    Conditional,
    /// Human approval node
    HumanApproval,
    /// Checkpoint node
    Checkpoint,
    /// Aggregation node
    Aggregate,
    /// Start node
    Start,
    /// End node
    End,
}

/// LangGraph edge types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LangGraphEdgeType {
    /// Normal sequential edge
    Sequential,
    /// Conditional edge (with predicate)
    Conditional,
    /// Parallel edge
    Parallel,
    /// Retry edge
    Retry,
    /// Fallback edge
    Fallback,
}

/// LangGraph node definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangGraphNode {
    pub id: String,
    pub name: String,
    pub node_type: LangGraphNodeType,
    pub config: HashMap<String, serde_json::Value>,
    pub timeout_seconds: Option<u64>,
    pub retry_config: Option<RetryConfig>,
    pub metadata: HashMap<String, String>,
}

impl LangGraphNode {
    pub fn new(id: String, name: String, node_type: LangGraphNodeType) -> Self {
        Self {
            id,
            name,
            node_type,
            config: HashMap::new(),
            timeout_seconds: None,
            retry_config: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_config(mut self, config: HashMap<String, serde_json::Value>) -> Self {
        self.config = config;
        self
    }

    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = Some(seconds);
        self
    }

    pub fn with_retry(mut self, config: RetryConfig) -> Self {
        self.retry_config = Some(config);
        self
    }
}

/// Retry configuration for nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub exponential_backoff: bool,
    pub retry_on: Vec<String>, // Error types to retry on
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            exponential_backoff: true,
            retry_on: vec!["timeout".to_string(), "network_error".to_string()],
        }
    }
}

/// LangGraph edge definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangGraphEdge {
    pub from: String,
    pub to: String,
    pub edge_type: LangGraphEdgeType,
    pub condition: Option<String>, // Expression for conditional edges
    pub metadata: HashMap<String, String>,
}

/// LangGraph state (shared across nodes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangGraphState {
    pub variables: HashMap<String, serde_json::Value>,
    pub messages: Vec<LangGraphMessage>,
    pub tool_results: HashMap<String, serde_json::Value>,
    pub checkpoints: Vec<CheckpointData>,
    pub execution_metadata: ExecutionMetadata,
}

impl Default for LangGraphState {
    fn default() -> Self {
        Self {
            variables: HashMap::new(),
            messages: Vec::new(),
            tool_results: HashMap::new(),
            checkpoints: Vec::new(),
            execution_metadata: ExecutionMetadata::default(),
        }
    }
}

/// Message in the LangGraph conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangGraphMessage {
    pub role: String, // "user", "assistant", "system", "tool"
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Checkpoint data for recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData {
    pub id: String,
    pub node_id: String,
    pub state_snapshot: LangGraphState,
    pub timestamp: SystemTime,
    pub metadata: HashMap<String, String>,
}

/// Execution metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    pub start_time: Option<SystemTime>,
    pub end_time: Option<SystemTime>,
    pub total_tokens: u64,
    pub total_latency_ms: u64,
    pub node_executions: HashMap<String, NodeExecutionInfo>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeExecutionInfo {
    pub node_id: String,
    pub start_time: SystemTime,
    pub end_time: Option<SystemTime>,
    pub status: NodeExecutionStatus,
    pub attempts: u32,
    pub tokens_used: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Retrying,
    Skipped,
}

/// LangGraph execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangGraphExecutionResult {
    pub run_id: String,
    pub graph_id: String,
    pub final_state: LangGraphState,
    pub execution_path: Vec<String>, // Node IDs in execution order
    pub status: ExecutionStatus,
    pub total_time_ms: u64,
    pub checkpoints_created: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Completed,
    Failed,
    Interrupted,
    PendingApproval,
}

/// LangGraph graph definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangGraphDefinition {
    pub id: String,
    pub name: String,
    pub version: String,
    pub nodes: Vec<LangGraphNode>,
    pub edges: Vec<LangGraphEdge>,
    pub entry_point: String,
    pub exit_points: Vec<String>,
    pub config: GraphConfig,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

impl LangGraphDefinition {
    pub fn new(id: String, name: String, entry_point: String) -> Self {
        let now = SystemTime::now();
        Self {
            id,
            name,
            version: "1.0.0".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            entry_point,
            exit_points: Vec::new(),
            config: GraphConfig::default(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn add_node(&mut self, node: LangGraphNode) {
        self.nodes.push(node);
        self.updated_at = SystemTime::now();
    }

    pub fn add_edge(&mut self, edge: LangGraphEdge) {
        self.edges.push(edge);
        self.updated_at = SystemTime::now();
    }
}

/// Graph configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConfig {
    pub max_iterations: u32,
    pub checkpoint_interval: u32, // Create checkpoint every N nodes
    pub enable_parallel: bool,
    pub default_timeout_seconds: u64,
    pub persistence_enabled: bool,
    pub persistence_path: Option<PathBuf>,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            checkpoint_interval: 5,
            enable_parallel: true,
            default_timeout_seconds: 300,
            persistence_enabled: false,
            persistence_path: None,
        }
    }
}

/// LangGraph adapter for lai
pub struct LangGraphAdapter {
    graph: LangGraphDefinition,
    state: Arc<RwLock<LangGraphState>>,
    execution_history: Arc<RwLock<Vec<LangGraphExecutionResult>>>,
    node_handlers: HashMap<String, Box<dyn NodeHandler>>,
}

impl LangGraphAdapter {
    /// Create a new LangGraph adapter
    pub fn new(graph: LangGraphDefinition) -> Self {
        Self {
            graph,
            state: Arc::new(RwLock::new(LangGraphState::default())),
            execution_history: Arc::new(RwLock::new(Vec::new())),
            node_handlers: HashMap::new(),
        }
    }

    /// Register a node handler
    pub fn register_handler(&mut self, node_id: String, handler: Box<dyn NodeHandler>) {
        self.node_handlers.insert(node_id, handler);
    }

    /// Execute the graph
    pub async fn execute(&self, initial_state: Option<LangGraphState>) -> Result<LangGraphExecutionResult, LangGraphError> {
        let run_id = Uuid::new_v4().to_string();
        let start_time = Instant::now();
        
        // Initialize state
        if let Some(state) = initial_state {
            *self.state.write().await = state;
        } else {
            let mut state = LangGraphState::default();
            state.execution_metadata.start_time = Some(SystemTime::now());
            *self.state.write().await = state;
        }

        let mut execution_path = Vec::new();
        let mut current_node_id = self.graph.entry_point.clone();
        let mut checkpoints_created = 0;
        let mut errors = Vec::new();
        let mut iteration = 0;

        while iteration < self.graph.config.max_iterations {
            iteration += 1;

            // Find current node
            let node = self.graph.nodes.iter()
                .find(|n| n.id == current_node_id)
                .ok_or_else(|| LangGraphError::NodeNotFound(current_node_id.clone()))?;

            // Check if we've reached an exit point
            if self.graph.exit_points.contains(&current_node_id) {
                break;
            }

            execution_path.push(current_node_id.clone());

            // Execute node
            let node_result = self.execute_node(node).await;
            
            // Update execution metadata
            self.update_execution_metadata(&current_node_id, &node_result).await;

            match node_result {
                Ok(next_node) => {
                    current_node_id = next_node;
                }
                Err(e) => {
                    errors.push(format!("Node {} failed: {}", current_node_id, e));
                    
                    // Try fallback edge
                    if let Some(fallback) = self.find_fallback_edge(&current_node_id) {
                        current_node_id = fallback;
                        continue;
                    }
                    
                    // Try retry
                    if let Some(retry_config) = &node.retry_config {
                        if self.should_retry(node, &errors).await {
                            continue;
                        }
                    }
                    
                    // No fallback or retry exhausted
                    let final_state = self.state.read().await.clone();
                    return Ok(LangGraphExecutionResult {
                        run_id,
                        graph_id: self.graph.id.clone(),
                        final_state,
                        execution_path,
                        status: ExecutionStatus::Failed,
                        total_time_ms: start_time.elapsed().as_millis() as u64,
                        checkpoints_created,
                        errors,
                    });
                }
            }

            // Create checkpoint if interval reached
            if execution_path.len() % self.graph.config.checkpoint_interval as usize == 0 {
                if self.graph.config.persistence_enabled {
                    self.create_checkpoint(&current_node_id).await?;
                    checkpoints_created += 1;
                }
            }
        }

        let final_state = self.state.read().await.clone();
        let mut state_mut = self.state.write().await;
        state_mut.execution_metadata.end_time = Some(SystemTime::now());

        let result = LangGraphExecutionResult {
            run_id,
            graph_id: self.graph.id.clone(),
            final_state: final_state.clone(),
            execution_path,
            status: if errors.is_empty() { ExecutionStatus::Completed } else { ExecutionStatus::Failed },
            total_time_ms: start_time.elapsed().as_millis() as u64,
            checkpoints_created,
            errors,
        };

        // Store in history
        self.execution_history.write().await.push(result.clone());

        Ok(result)
    }

    /// Execute a single node
    async fn execute_node(&self, node: &LangGraphNode) -> Result<String, LangGraphError> {
        // Check if we have a handler
        if let Some(handler) = self.node_handlers.get(&node.id) {
            let mut state = self.state.write().await;
            let result = handler.execute(node, &mut state).await?;
            return Ok(result);
        }

        // Default execution based on node type
        match node.node_type {
            LangGraphNodeType::LlmCall => self.execute_llm_node(node).await,
            LangGraphNodeType::ToolCall => self.execute_tool_node(node).await,
            LangGraphNodeType::Conditional => self.execute_conditional_node(node).await,
            LangGraphNodeType::HumanApproval => self.execute_approval_node(node).await,
            LangGraphNodeType::Checkpoint => self.execute_checkpoint_node(node).await,
            LangGraphNodeType::Aggregate => self.execute_aggregate_node(node).await,
            LangGraphNodeType::Start => self.execute_start_node(node).await,
            LangGraphNodeType::End => Ok(self.find_next_node(&node.id).unwrap_or_default()),
        }
    }

    /// Execute LLM call node
    async fn execute_llm_node(&self, node: &LangGraphNode) -> Result<String, LangGraphError> {
        // Simulate LLM call - in production would call actual LLM
        let mut state = self.state.write().await;
        
        let prompt = node.config.get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("Default prompt");
        
        state.messages.push(LangGraphMessage {
            role: "assistant".to_string(),
            content: format!("LLM response for: {}", prompt),
            tool_calls: None,
            tool_call_id: None,
            timestamp: SystemTime::now(),
        });

        Ok(self.find_next_node(&node.id).unwrap_or_default())
    }

    /// Execute tool call node
    async fn execute_tool_node(&self, node: &LangGraphNode) -> Result<String, LangGraphError> {
        let mut state = self.state.write().await;
        
        let tool_name = node.config.get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown_tool");
        
        let args = node.config.get("arguments")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        // Simulate tool execution
        state.tool_results.insert(tool_name.to_string(), serde_json::json!({
            "status": "success",
            "result": format!("Tool {} executed with args: {}", tool_name, args),
        }));

        Ok(self.find_next_node(&node.id).unwrap_or_default())
    }

    /// Execute conditional node
    async fn execute_conditional_node(&self, node: &LangGraphNode) -> Result<String, LangGraphError> {
        let state = self.state.read().await;
        
        let condition = node.config.get("condition")
            .and_then(|v| v.as_str())
            .unwrap_or("true");
        
        // Simple condition evaluation - in production would use expression evaluator
        let condition_met = condition == "true" || condition == "1";
        
        drop(state);
        
        // Find conditional edges
        for edge in &self.graph.edges {
            if edge.from == node.id && edge.edge_type == LangGraphEdgeType::Conditional {
                if let Some(cond) = &edge.condition {
                    if cond == condition || condition_met {
                        return Ok(edge.to.clone());
                    }
                } else if condition_met {
                    return Ok(edge.to.clone());
                }
            }
        }
        
        // Default to first sequential edge
        Ok(self.find_next_node(&node.id).unwrap_or_default())
    }

    /// Execute human approval node
    async fn execute_approval_node(&self, node: &LangGraphNode) -> Result<String, LangGraphError> {
        // In production, this would wait for human input
        // For now, simulate approval
        let mut state = self.state.write().await;
        state.execution_metadata.errors.push("Human approval required - auto-approved for testing".to_string());
        
        Ok(self.find_next_node(&node.id).unwrap_or_default())
    }

    /// Execute checkpoint node
    async fn execute_checkpoint_node(&self, node: &LangGraphNode) -> Result<String, LangGraphError> {
        self.create_checkpoint(&node.id).await?;
        Ok(self.find_next_node(&node.id).unwrap_or_default())
    }

    /// Execute aggregate node
    async fn execute_aggregate_node(&self, node: &LangGraphNode) -> Result<String, LangGraphError> {
        let mut state = self.state.write().await;
        
        // Aggregate results from previous nodes
        let agg_key = node.config.get("aggregate_key")
            .and_then(|v| v.as_str())
            .unwrap_or("aggregated_results");
        
        let mut aggregated = Vec::new();
        for (key, value) in &state.tool_results {
            aggregated.push(serde_json::json!({
                "tool": key,
                "result": value,
            }));
        }
        
        state.variables.insert(agg_key.to_string(), serde_json::json!(aggregated));
        
        Ok(self.find_next_node(&node.id).unwrap_or_default())
    }

    /// Execute start node
    async fn execute_start_node(&self, node: &LangGraphNode) -> Result<String, LangGraphError> {
        Ok(self.find_next_node(&node.id).unwrap_or_default())
    }

    /// Find next sequential node
    fn find_next_node(&self, current_id: &str) -> Option<String> {
        for edge in &self.graph.edges {
            if edge.from == current_id && edge.edge_type == LangGraphEdgeType::Sequential {
                return Some(edge.to.clone());
            }
        }
        None
    }

    /// Find fallback edge
    fn find_fallback_edge(&self, current_id: &str) -> Option<String> {
        for edge in &self.graph.edges {
            if edge.from == current_id && edge.edge_type == LangGraphEdgeType::Fallback {
                return Some(edge.to.clone());
            }
        }
        None
    }

    /// Check if should retry
    async fn should_retry(&self, node: &LangGraphNode, _errors: &[String]) -> bool {
        if let Some(retry_config) = &node.retry_config {
            let state = self.state.read().await;
            if let Some(exec_info) = state.execution_metadata.node_executions.get(&node.id) {
                return exec_info.attempts < retry_config.max_attempts;
            }
        }
        false
    }

    /// Create checkpoint
    async fn create_checkpoint(&self, node_id: &str) -> Result<(), LangGraphError> {
        let state = self.state.read().await;
        let checkpoint = CheckpointData {
            id: Uuid::new_v4().to_string(),
            node_id: node_id.to_string(),
            state_snapshot: state.clone(),
            timestamp: SystemTime::now(),
            metadata: HashMap::new(),
        };
        
        let mut state_mut = self.state.write().await;
        state_mut.checkpoints.push(checkpoint);
        
        Ok(())
    }

    /// Update execution metadata
    async fn update_execution_metadata(&self, node_id: &str, result: &Result<String, LangGraphError>) {
        let mut state = self.state.write().await;
        let exec_info = state.execution_metadata.node_executions
            .entry(node_id.to_string())
            .or_insert_with(|| NodeExecutionInfo {
                node_id: node_id.to_string(),
                start_time: SystemTime::now(),
                end_time: None,
                status: NodeExecutionStatus::Running,
                attempts: 0,
                tokens_used: 0,
                error: None,
            });
        
        exec_info.attempts += 1;
        exec_info.end_time = Some(SystemTime::now());
        exec_info.status = if result.is_ok() { 
            NodeExecutionStatus::Completed 
        } else { 
            NodeExecutionStatus::Failed 
        };
        
        if let Err(e) = result {
            exec_info.error = Some(e.to_string());
        }
    }

    /// Resume from checkpoint
    pub async fn resume_from_checkpoint(&self, checkpoint_id: &str) -> Result<LangGraphExecutionResult, LangGraphError> {
        let state = self.state.read().await;
        let checkpoint = state.checkpoints.iter()
            .find(|c| c.id == checkpoint_id)
            .ok_or_else(|| LangGraphError::CheckpointNotFound(checkpoint_id.to_string()))?
            .clone();
        drop(state);
        
        // Restore state
        *self.state.write().await = checkpoint.state_snapshot;
        
        // Continue execution from checkpoint node
        let node_id = checkpoint.node_id;
        let next_node = self.find_next_node(&node_id).unwrap_or(node_id);
        
        // Execute from next node
        self.execute(Some(checkpoint.state_snapshot)).await
    }

    /// Get execution history
    pub async fn get_history(&self) -> Vec<LangGraphExecutionResult> {
        self.execution_history.read().await.clone()
    }
}

/// Node handler trait for custom node execution
#[async_trait]
pub trait NodeHandler: Send + Sync {
    async fn execute(&self, node: &LangGraphNode, state: &mut LangGraphState) -> Result<String, LangGraphError>;
}

/// LangGraph errors
#[derive(Debug, thiserror::Error)]
pub enum LangGraphError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Checkpoint not found: {0}")]
    CheckpointNotFound(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Condition evaluation failed: {0}")]
    ConditionError(String),
    #[error("Handler error: {0}")]
    HandlerError(String),
    #[error("Persistence error: {0}")]
    PersistenceError(String),
    #[error("Max iterations exceeded")]
    MaxIterationsExceeded,
}

/// Builder for LangGraph definitions
pub struct LangGraphBuilder {
    definition: LangGraphDefinition,
}

impl LangGraphBuilder {
    pub fn new(name: String, entry_point: String) -> Self {
        Self {
            definition: LangGraphDefinition::new(
                Uuid::new_v4().to_string(),
                name,
                entry_point,
            ),
        }
    }

    pub fn add_llm_node(mut self, id: String, name: String, prompt: String) -> Self {
        let mut config = HashMap::new();
        config.insert("prompt".to_string(), serde_json::json!(prompt));
        
        self.definition.add_node(LangGraphNode::new(id, name, LangGraphNodeType::LlmCall)
            .with_config(config));
        self
    }

    pub fn add_tool_node(mut self, id: String, name: String, tool: String, args: serde_json::Value) -> Self {
        let mut config = HashMap::new();
        config.insert("tool".to_string(), serde_json::json!(tool));
        config.insert("arguments".to_string(), args);
        
        self.definition.add_node(LangGraphNode::new(id, name, LangGraphNodeType::ToolCall)
            .with_config(config));
        self
    }

    pub fn add_conditional_node(mut self, id: String, name: String, condition: String) -> Self {
        let mut config = HashMap::new();
        config.insert("condition".to_string(), serde_json::json!(condition));
        
        self.definition.add_node(LangGraphNode::new(id, name, LangGraphNodeType::Conditional)
            .with_config(config));
        self
    }

    pub fn add_approval_node(mut self, id: String, name: String) -> Self {
        self.definition.add_node(LangGraphNode::new(id, name, LangGraphNodeType::HumanApproval));
        self
    }

    pub fn add_checkpoint_node(mut self, id: String, name: String) -> Self {
        self.definition.add_node(LangGraphNode::new(id, name, LangGraphNodeType::Checkpoint));
        self
    }

    pub fn add_sequential_edge(mut self, from: String, to: String) -> Self {
        self.definition.add_edge(LangGraphEdge {
            from,
            to,
            edge_type: LangGraphEdgeType::Sequential,
            condition: None,
            metadata: HashMap::new(),
        });
        self
    }

    pub fn add_conditional_edge(mut self, from: String, to: String, condition: String) -> Self {
        self.definition.add_edge(LangGraphEdge {
            from,
            to,
            edge_type: LangGraphEdgeType::Conditional,
            condition: Some(condition),
            metadata: HashMap::new(),
        });
        self
    }

    pub fn add_fallback_edge(mut self, from: String, to: String) -> Self {
        self.definition.add_edge(LangGraphEdge {
            from,
            to,
            edge_type: LangGraphEdgeType::Fallback,
            condition: None,
            metadata: HashMap::new(),
        });
        self
    }

    pub fn set_config(mut self, config: GraphConfig) -> Self {
        self.definition.config = config;
        self
    }

    pub fn build(self) -> LangGraphDefinition {
        self.definition
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_langgraph_builder() {
        let graph = LangGraphBuilder::new("Test Graph".to_string(), "start".to_string())
            .add_llm_node("llm1".to_string(), "LLM Call".to_string(), "Generate code".to_string())
            .add_tool_node("tool1".to_string(), "Format Code".to_string(), "formatter".to_string(), serde_json::json!({}))
            .add_sequential_edge("start".to_string(), "llm1".to_string())
            .add_sequential_edge("llm1".to_string(), "tool1".to_string())
            .add_sequential_edge("tool1".to_string(), "end".to_string())
            .build();
        
        assert_eq!(graph.nodes.len(), 4); // start, llm1, tool1, end (implicit)
        assert_eq!(graph.edges.len(), 3);
    }

    #[test]
    fn test_langgraph_state() {
        let mut state = LangGraphState::default();
        state.variables.insert("key".to_string(), serde_json::json!("value"));
        assert_eq!(state.variables.get("key"), Some(&serde_json::json!("value")));
    }

    #[test]
    fn test_retry_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert!(config.exponential_backoff);
    }

    #[test]
    fn test_node_execution_status() {
        assert_eq!(NodeExecutionStatus::Pending as u8, 0);
        assert_eq!(NodeExecutionStatus::Completed as u8, 2);
    }
}