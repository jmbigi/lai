//! adapters - Adaptadores para análisis CPG
//! 
//! Implementa adaptadores para Joern y codebadger.

pub mod joern;
pub mod codebadger;

// Re-exports
pub use joern::{JoernAdapter, JoernAdapterConfig};
pub use codebadger::{CodebadgerAdapter, CodebadgerAdapterConfig, CodebadgerReport, CodebadgerFinding, CodebadgerSeverity, CodebadgerLocation};