//! memory - Memoria persistente para agentes autónomos
//! 
//! Implementa tres niveles de memoria:
//! - Episódica: conversaciones, ejecuciones, decisiones, errores
//! - Semántica: arquitectura, dependencias, constraints, preferencias
//! - Temporal: qué/cuándo/por qué cambió, grafos temporales (Graphiti)

pub mod episodic;
pub mod semantic;
pub mod temporal;
pub mod adapters;

pub use episodic::{EpisodicMemory, EpisodicConfig, Episode, EpisodeType};
pub use semantic::{SemanticMemory, SemanticConfig, Concept, Relation};
pub use temporal::{TemporalMemory, TemporalConfig, TemporalEvent, TemporalGraph};
pub use adapters::{Mem0Adapter, Mem0Config, GraphitiAdapter, GraphitiConfig};