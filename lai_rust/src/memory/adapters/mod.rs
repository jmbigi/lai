//! memory/adapters - Adaptadores para memoria persistente
//! 
//! Incluye adaptadores para Mem0 (memoria vectorial) y Graphiti (grafos temporales).

pub mod mem0;
pub mod graphiti;

pub use mem0::{Mem0Adapter, Mem0Config};
pub use graphiti::{GraphitiAdapter, GraphitiConfig};