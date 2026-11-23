//! code_analysis/cpg.rs - Interfaz Code Property Graph (CPG)
//! 
//! Define la interfaz común para análisis semántico basado en CPG.
//! Implementa consultas para: call graphs, data-flow, taint tracking, program slicing.

use crate::core::contracts::{ExecutionRequest, ExecutionResult, ResultStatus, AdapterError, CapabilityAdapter, Evidence, EvidenceType, ExecutionMetrics, ExecutionContext, VerificationLevel, ResourceLimits, PreflightResult, IntegrityCheck, CheckSeverity, ValidationResult, ValidationIssue, ValidationSeverity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use async_trait::async_trait;

/// Tipo de nodo en el CPG
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CpgNodeType {
    Method,
    Call,
    Parameter,
    Return,
    Field,
    Local,
    Block,
    ControlStructure,
    Literal,
    Identifier,
    TypeDecl,
    MethodReturn,
    MethodParameterIn,
    MethodParameterOut,
    File,
    Namespace,
    Type,
    Comment,
}

/// Tipo de arista en el CPG
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CpgEdgeType {
    // AST edges
    Ast,
    // Control flow
    Cfg,
    // Data flow
    Ddg,
    // Call graph
    Calls,
    // Type information
    Type,
    // Reaching definitions
    Reaches,
    // Taint tracking
    Taint,
    // Dominator tree
    Dominates,
    // Post-dominator
    PostDominates,
}

/// Nodo en el Code Property Graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpgNode {
    pub id: u64,
    pub node_type: CpgNodeType,
    pub name: Option<String>,
    pub code: Option<String>,
    pub file_name: Option<String>,
    pub line_number: Option<u32>,
    pub column_number: Option<u32>,
    pub line_number_end: Option<u32>,
    pub column_number_end: Option<u32>,
    pub properties: HashMap<String, String>,
}

/// Arista en el Code Property Graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpgEdge {
    pub id: u64,
    pub source: u64,
    pub target: u64,
    pub edge_type: CpgEdgeType,
    pub properties: HashMap<String, String>,
}

/// Code Property Graph completo
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodePropertyGraph {
    pub nodes: HashMap<u64, CpgNode>,
    pub edges: Vec<CpgEdge>,
    pub meta: CpgMeta,
}

/// Metadatos del CPG
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CpgMeta {
    pub language: String,
    pub version: String,
    pub generated_at: String,
    pub file_count: usize,
    pub node_count: usize,
    pub edge_count: usize,
}

/// Resultado de consulta CPG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpgQueryResult {
    pub nodes: Vec<CpgNode>,
    pub edges: Vec<CpgEdge>,
    pub paths: Vec<Vec<u64>>, // Para rutas en data-flow/taint
}

/// Configuración para análisis CPG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpgAnalysisConfig {
    pub include_call_graph: bool,
    pub include_data_flow: bool,
    pub include_taint_tracking: bool,
    pub include_control_flow: bool,
    pub max_depth: usize,
    pub timeout_seconds: u64,
}

impl Default for CpgAnalysisConfig {
    fn default() -> Self {
        Self {
            include_call_graph: true,
            include_data_flow: true,
            include_taint_tracking: true,
            include_control_flow: true,
            max_depth: 10,
            timeout_seconds: 60,
        }
    }
}

/// Vulnerabilidad detectada por análisis CPG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpgVulnerability {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: VulnerabilitySeverity,
    pub cwe_id: Option<String>,
    pub location: VulnerabilityLocation,
    pub evidence: Vec<String>,
    pub confidence: f32,
}

/// Severidad de vulnerabilidad
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum VulnerabilitySeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

/// Ubicación de vulnerabilidad
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityLocation {
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub function_name: Option<String>,
    pub code_snippet: Option<String>,
}

/// Trait para adaptadores CPG
#[async_trait]
pub trait CpgAdapter: Send + Sync {
    /// Genera CPG desde código fuente
    async fn generate_cpg(&self, source_paths: &[PathBuf], config: &CpgAnalysisConfig) -> Result<CodePropertyGraph, AdapterError>;
    
    /// Ejecuta consulta sobre CPG existente
    async fn query_cpg(&self, cpg: &CodePropertyGraph, query: &CpgQuery) -> Result<CpgQueryResult, AdapterError>;
    
    /// Detecta vulnerabilidades
    async fn find_vulnerabilities(&self, cpg: &CodePropertyGraph) -> Result<Vec<CpgVulnerability>, AdapterError>;
    
    /// Obtiene call graph
    async fn get_call_graph(&self, cpg: &CodePropertyGraph, method_name: &str) -> Result<Vec<CpgNode>, AdapterError>;
    
    /// Obtiene data flow para variable
    async fn get_data_flow(&self, cpg: &CodePropertyGraph, variable: &str, method: &str) -> Result<CpgQueryResult, AdapterError>;
    
    /// Taint tracking desde source a sink
    async fn taint_tracking(&self, cpg: &CodePropertyGraph, sources: &[String], sinks: &[String]) -> Result<Vec<Vec<u64>>, AdapterError>;
    
    /// Program slicing
    async fn program_slice(&self, cpg: &CodePropertyGraph, criterion: &SliceCriterion) -> Result<CpgQueryResult, AdapterError>;
    
    /// Health check
    async fn health_check(&self) -> HealthStatus;
    
    /// Capability ID
    fn capability_id(&self) -> &str;
    
    /// Capability version
    fn capability_version(&self) -> &str;
}

/// Criterio para program slicing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceCriterion {
    pub node_id: u64,
    pub direction: SliceDirection,
    pub max_depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SliceDirection {
    Forward,
    Backward,
    Both,
}

/// Consulta CPG genérica
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpgQuery {
    pub query_type: CpgQueryType,
    pub parameters: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CpgQueryType {
    // Call graph queries
    Callees { method: String, depth: usize },
    Callers { method: String, depth: usize },
    CallPath { from: String, to: String },
    
    // Data flow queries
    ReachingDefinitions { variable: String, at_node: u64 },
    LiveVariables { at_node: u64 },
    DefUseChains { variable: String, method: String },
    
    // Taint tracking
    TaintSources { method: String },
    TaintSinks { method: String },
    TaintPaths { sources: Vec<String>, sinks: Vec<String> },
    
    // Control flow
    CFGPaths { from_node: u64, to_node: u64 },
    Dominators { node: u64 },
    PostDominators { node: u64 },
    
    // Vulnerability patterns
    SqlInjection,
    Xss,
    PathTraversal,
    CommandInjection,
    Xxe,
    Ssrf,
    InsecureDeserialization,
    HardcodedSecrets,
}

/// Resultado de salud
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub version: String,
    pub last_check: String,
    pub details: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cpg_node_creation() {
        let node = CpgNode {
            id: 1,
            node_type: CpgNodeType::Method,
            name: Some("main".to_string()),
            code: Some("fn main() {}".to_string()),
            file_name: Some("main.rs".to_string()),
            line_number: Some(1),
            column_number: Some(1),
            line_number_end: Some(3),
            column_number_end: Some(10),
            properties: HashMap::new(),
        };
        assert_eq!(node.node_type, CpgNodeType::Method);
    }
    
    #[test]
    fn test_cpg_edge_creation() {
        let edge = CpgEdge {
            id: 1,
            source: 1,
            target: 2,
            edge_type: CpgEdgeType::Calls,
            properties: HashMap::new(),
        };
        assert_eq!(edge.edge_type, CpgEdgeType::Calls);
    }
    
    #[test]
    fn test_vulnerability_creation() {
        let vuln = CpgVulnerability {
            id: "VULN-001".to_string(),
            title: "SQL Injection".to_string(),
            description: "User input used directly in SQL query".to_string(),
            severity: VulnerabilitySeverity::Critical,
            cwe_id: Some("CWE-89".to_string()),
            location: VulnerabilityLocation {
                file_path: "src/main.rs".to_string(),
                line_start: 10,
                line_end: 15,
                function_name: Some("query_db".to_string()),
                code_snippet: Some("format!(\"SELECT * FROM users WHERE id = {}\", user_input)".to_string()),
            },
            evidence: vec!["user_input flows to query".to_string()],
            confidence: 0.95,
        };
        assert_eq!(vuln.severity, VulnerabilitySeverity::Critical);
    }
}