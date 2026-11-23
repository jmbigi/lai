//! security - Módulo de seguridad e integridad
//! 
//! Incluye: verificación de integridad, MCP Gateway, Policy Engine (OPA/Rego)

pub mod integrity;
pub mod mcp_gateway;
pub mod policies;

// Re-exports
pub use integrity::{IntegrityVerifier, VerifiedToolRegistry, VerifiedTool, ToolDefinition, VerificationOutcome, compute_definition_hash, compute_executable_hash, RuntimeIntegrityMonitor};
pub use mcp_gateway::{McpGateway, McpGatewayConfig, McpHangarClient, McpToolAdapter, StandardMcpToolAdapter, McpGatewayHealth, ExecutionAudit};
pub use policies::{PolicyEngine, PolicyEngineBuilder, CompositePolicyEvaluator, builtin_policies};