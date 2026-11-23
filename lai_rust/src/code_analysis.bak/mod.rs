//! code_analysis - Análisis semántico profundo de código
//! 
//! Implementa análisis basado en Code Property Graphs (CPG) usando Joern y codebadger.
//! Capacidades:
//! - Call graphs y call paths
//! - Data flow analysis (reaching definitions, live variables, def-use chains)
//! - Taint tracking (sources a sinks)
//! - Control flow analysis (CFG, dominadores)
//! - Detección de vulnerabilidades (SQLi, XSS, Command Injection, Path Traversal, etc.)
//! - Hardcoded secrets detection
//! - Program slicing

pub mod cpg;
pub mod adapters;

// Re-exports
pub use cpg::{
    CpgNodeType, CpgEdgeType, CpgNode, CpgEdge, CodePropertyGraph, CpgMeta,
    CpgQueryResult, CpgAnalysisConfig, CpgVulnerability, VulnerabilitySeverity,
    VulnerabilityLocation, CpgAdapter, CpgQuery, CpgQueryType, SliceCriterion,
    SliceDirection, HealthStatus,
};
pub use adapters::joern::{JoernAdapter, JoernAdapterConfig};
pub use adapters::codebadger::{CodebadgerAdapter, CodebadgerAdapterConfig, CodebadgerReport, CodebadgerFinding, CodebadgerSeverity, CodebadgerLocation};