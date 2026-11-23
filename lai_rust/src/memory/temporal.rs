//! memory/temporal.rs - Memoria temporal (Graphiti-style)
//! 
//! Grafo temporal para rastrear evolución: qué cambió, cuándo, por qué, quién decidió.

use crate::core::contracts::{ExecutionResult, ResultStatus};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Configuración de memoria temporal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalConfig {
    pub storage_path: PathBuf,
    pub enable_graphiti: bool,
    pub graphiti_uri: Option<String>,
    pub snapshot_interval_hours: u32,
    pub max_events_per_entity: usize,
}

impl Default for TemporalConfig {
    fn default() -> Self {
        Self {
            storage_path: PathBuf::from("./data/memory/temporal"),
            enable_graphiti: false,
            graphiti_uri: None,
            snapshot_interval_hours: 24,
            max_events_per_entity: 1000,
        }
    }
}

/// Evento temporal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalEvent {
    pub id: Uuid,
    pub entity_id: Uuid,
    pub entity_type: EntityType,
    pub event_type: TemporalEventType,
    pub timestamp: DateTime<Utc>,
    pub actor: String, // agent_id o user_id
    pub description: String,
    pub before_state: Option<serde_json::Value>,
    pub after_state: Option<serde_json::Value>,
    pub diff: Option<serde_json::Value>,
    pub reasoning: Option<String>,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Tipo de entidad
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    File,
    Function,
    Class,
    Module,
    Config,
    Dependency,
    Architecture,
    Requirement,
    Test,
    Documentation,
    Deployment,
    Infrastructure,
    Other,
}

/// Tipo de evento temporal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalEventType {
    Created,
    Modified,
    Deleted,
    Renamed,
    Moved,
    Refactored,
    ConfigChanged,
    DependencyAdded,
    DependencyRemoved,
    DependencyUpdated,
    TestAdded,
    TestRemoved,
    TestPassed,
    TestFailed,
    BugIntroduced,
    BugFixed,
    VulnerabilityDetected,
    VulnerabilityFixed,
    SecurityPatchApplied,
    PerformanceImproved,
    PerformanceDegraded,
    Deployed,
    RolledBack,
    Scaled,
    Migrated,
    Documented,
    DecisionMade,
    ReviewCompleted,
    Approved,
    Rejected,
}

/// Grafo temporal
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemporalGraph {
    pub entities: HashMap<Uuid, TemporalEntity>,
    pub events: Vec<TemporalEvent>,
    pub edges: Vec<TemporalEdge>,
}

/// Entidad temporal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalEntity {
    pub id: Uuid,
    pub entity_type: EntityType,
    pub name: String,
    pub identifier: String, // path, class name, etc.
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub current_state: serde_json::Value,
    pub event_ids: Vec<Uuid>,
    pub tags: Vec<String>,
}

/// Arista temporal (relación entre eventos)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalEdge {
    pub id: Uuid,
    pub from_event: Uuid,
    pub to_event: Uuid,
    pub edge_type: TemporalEdgeType,
    pub weight: f32,
}

/// Tipo de arista temporal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalEdgeType {
    CausallyPrecedes,
    TriggeredBy,
    DependsOn,
    Resolves,
    ConflictsWith,
    Supersedes,
    RelatedTo,
}

/// Snapshot del estado en un momento
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalSnapshot {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub entities: HashMap<Uuid, serde_json::Value>,
    pub events: Vec<Uuid>,
    pub description: String,
    pub trigger: SnapshotTrigger,
}

/// Qué disparó el snapshot
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotTrigger {
    Scheduled,
    Manual,
    MajorChange,
    Deployment,
    Incident,
    Milestone,
}

/// Índice temporal
#[derive(Debug, Default)]
struct TemporalIndex {
    entity_events: HashMap<Uuid, Vec<usize>>,
    events_by_type: HashMap<TemporalEventType, Vec<usize>>,
    events_by_time: Vec<(DateTime<Utc>, usize)>,
    events_by_actor: HashMap<String, Vec<usize>>,
    entity_snapshots: HashMap<Uuid, Vec<usize>>,
}

/// Memoria temporal principal
pub struct TemporalMemory {
    config: TemporalConfig,
    graph: Arc<RwLock<TemporalGraph>>,
    index: Arc<RwLock<TemporalIndex>>,
    snapshots: Arc<RwLock<Vec<TemporalSnapshot>>>,
    graphiti_adapter: Option<GraphitiAdapter>,
}

impl TemporalMemory {
    pub fn new(config: TemporalConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.storage_path)?;
        
        let graphiti_adapter = if config.enable_graphiti {
            Some(GraphitiAdapter::new(GraphitiConfig {
                api_endpoint: config.graphiti_uri.clone().unwrap_or_default(),
                ..Default::default()
            })?)
        else {
            None
        };
        
        let memory = Self {
            config,
            graph: Arc::new(RwLock::new(TemporalGraph::default())),
            index: Arc::new(RwLock::new(TemporalIndex::default())),
            snapshots: Arc::new(RwLock::new(Vec::new())),
            graphiti_adapter,
        };
        
        memory.load()?;
        Ok(memory)
    }
    
    /// Registra un evento temporal
    pub async fn record_event(&self, event: TemporalEvent) -> Result<Uuid> {
        let event_id = event.id;
        
        // Actualizar entidad
        self.update_entity(&event).await?;
        
        // Añadir evento
        let event_idx = {
            let mut graph = self.graph.write().unwrap();
            let idx = graph.events.len();
            graph.events.push(event.clone());
            idx
        };
        
        // Actualizar índices
        self.update_index(event_idx, &event).await?;
        
        // Persistir
        self.persist_event(&event).await?;
        
        // Sincronizar con Graphiti si está habilitado
        if let Some(adapter) = &self.graphiti_adapter {
            let _ = adapter.ingest_event(&event).await;
        }
        
        Ok(event_id)
    }
    
    /// Obtiene la línea de tiempo de una entidad
    pub fn get_entity_timeline(&self, entity_id: Uuid, limit: usize) -> Result<Vec<TemporalEvent>> {
        let graph = self.graph.read().unwrap();
        let index = self.index.read().unwrap();
        
        if let Some(indices) = index.entity_events.get(&entity_id) {
            let events = &graph.events;
            let mut result: Vec<_> = indices.iter()
                .rev()
                .take(limit)
                .map(|&i| events[i].clone())
                .collect();
            result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Obtiene eventos en un rango temporal
    pub fn get_events_in_range(&self, start: DateTime<Utc>, end: DateTime<Utc>, event_types: Option<Vec<TemporalEventType>>) -> Result<Vec<TemporalEvent>> {
        let graph = self.graph.read().unwrap();
        let index = self.index.read().unwrap();
        
        let start_idx = index.events_by_time.binary_search_by(|(t, _)| t.cmp(&start))
            .unwrap_or_else(|e| e);
        let end_idx = index.events_by_time.binary_search_by(|(t, _)| t.cmp(&end))
            .unwrap_or_else(|e| e);
        
        let events = &graph.events;
        let mut result = Vec::new();
        
        for &(_, idx) in index.events_by_time.iter().skip(start_idx).take(end_idx.saturating_sub(start_idx)) {
            let event = &events[idx];
            if let Some(types) = &event_types {
                if types.contains(&event.event_type) {
                    result.push(event.clone());
                }
            } else {
                result.push(event.clone());
            }
        }
        
        Ok(result)
    }
    
    /// Obtiene eventos por actor
    pub fn get_events_by_actor(&self, actor: &str, limit: usize) -> Result<Vec<TemporalEvent>> {
        let graph = self.graph.read().unwrap();
        let index = self.index.read().unwrap();
        
        if let Some(indices) = index.events_by_actor.get(actor) {
            let events = &graph.events;
            let result: Vec<_> = indices.iter()
                .rev()
                .take(limit)
                .map(|&i| events[i].clone())
                .collect();
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Crea un snapshot
    pub async fn create_snapshot(&self, description: String, trigger: SnapshotTrigger) -> Result<TemporalSnapshot> {
        let graph = self.graph.read().unwrap();
        let mut entities = HashMap::new();
        
        for (id, entity) in &graph.entities {
            entities.insert(*id, entity.current_state.clone());
        }
        
        let event_ids: Vec<_> = graph.events.iter().map(|e| e.id).collect();
        
        let snapshot = TemporalSnapshot {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            entities,
            events: event_ids,
            description,
            trigger,
        };
        
        let mut snapshots = self.snapshots.write().unwrap();
        let idx = snapshots.len();
        snapshots.push(snapshot.clone());
        
        // Persistir
        self.persist_snapshot(&snapshot).await?;
        
        // Sincronizar con Graphiti
        if let Some(adapter) = &self.graphiti_adapter {
            let _ = adapter.ingest_snapshot(&snapshot).await;
        }
        
        Ok(snapshot)
    }
    
    /// Compara dos snapshots
    pub fn diff_snapshots(&self, snapshot1_id: Uuid, snapshot2_id: Uuid) -> Result<SnapshotDiff> {
        let snapshots = self.snapshots.read().unwrap();
        let s1 = snapshots.iter().find(|s| s.id == snapshot1_id);
        let s2 = snapshots.iter().find(|s| s.id == snapshot2_id);
        
        let (Some(s1), Some(s2)) = (s1, s2) else {
            return Err(Context("Snapshot not found"));
        };
        
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut modified = Vec::new();
        
        for (id, state) in &s2.entities {
            match s1.entities.get(id) {
                Some(old_state) if old_state != state => modified.push((*id, old_state.clone(), state.clone())),
                None => added.push((*id, state.clone())),
                _ => {}
            }
        }
        
        for (id, state) in &s1.entities {
            if !s2.entities.contains_key(id) {
                removed.push((*id, state.clone()));
            }
        }
        
        Ok(SnapshotDiff {
            from: snapshot1_id,
            to: snapshot2_id,
            added,
            removed,
            modified,
        })
    }
    
    /// Detecta patrones de cambio
    pub fn detect_change_patterns(&self, entity_id: Uuid) -> Result<Vec<ChangePattern>> {
        let timeline = self.get_entity_timeline(entity_id, 100)?;
        let mut patterns = Vec::new();
        
        if timeline.len() < 3 {
            return Ok(patterns);
        }
        
        // Patrón: cambios frecuentes en poco tiempo
        let mut recent_changes = 0;
        let now = Utc::now();
        for event in &timeline {
            if (now - event.timestamp).num_hours() < 24 {
                recent_changes += 1;
            }
        }
        
        if recent_changes > 10 {
            patterns.push(ChangePattern {
                pattern_type: ChangePatternType::HighFrequency,
                description: format!("{} changes in last 24h", recent_changes),
                confidence: 0.8,
                affected_entities: vec![entity_id],
            });
        }
        
        // Patrón: oscilación (cambios revertidos)
        let mut reversions = 0;
        for window in timeline.windows(3) {
            if let [e1, _, e3] = window {
                if e1.event_type == TemporalEventType::Modified && e3.event_type == TemporalEventType::Modified {
                    if let (Some(before), Some(after)) = (&e1.before_state, &e3.after_state) {
                        if before == after {
                            reversions += 1;
                        }
                    }
                }
            }
        }
        
        if reversions > 2 {
            patterns.push(ChangePattern {
                pattern_type: ChangePatternType::Oscillation,
                description: format!("{} reversions detected", reversions),
                confidence: 0.7,
                affected_entities: vec![entity_id],
            });
        }
        
        // Patrón: dependencias en cascada
        let cascading = self.detect_cascading_changes(entity_id)?;
        patterns.extend(cascading);
        
        Ok(patterns)
    }
    
    fn detect_cascading_changes(&self, entity_id: Uuid) -> Result<Vec<ChangePattern>> {
        // Buscar eventos que afectaron a esta entidad y luego causaron cambios en otras
        let mut patterns = Vec::new();
        // Implementación simplificada
        Ok(patterns)
    }
    
    async fn update_entity(&self, event: &TemporalEvent) -> Result<()> {
        let mut graph = self.graph.write().unwrap();
        
        let entity = graph.entities.entry(event.entity_id).or_insert_with(|| TemporalEntity {
            id: event.entity_id,
            entity_type: event.entity_type,
            name: format!("entity_{}", event.entity_id),
            identifier: String::new(),
            first_seen: event.timestamp,
            last_seen: event.timestamp,
            current_state: event.after_state.clone().unwrap_or_default(),
            event_ids: Vec::new(),
            tags: Vec::new(),
        });
        
        entity.last_seen = event.timestamp;
        entity.current_state = event.after_state.clone().unwrap_or(entity.current_state.clone());
        entity.event_ids.push(event.id);
        
        Ok(())
    }
    
    async fn update_index(&self, event_idx: usize, event: &TemporalEvent) -> Result<()> {
        let mut index = self.index.write().unwrap();
        
        index.entity_events.entry(event.entity_id).or_default().push(event_idx);
        index.events_by_type.entry(event.event_type).or_default().push(event_idx);
        index.events_by_time.push((event.timestamp, event_idx));
        index.events_by_time.sort_by(|a, b| a.0.cmp(&b.0));
        index.events_by_actor.entry(event.actor.clone()).or_default().push(event_idx);
        
        Ok(())
    }
    
    async fn persist_event(&self, event: &TemporalEvent) -> Result<()> {
        let path = self.config.storage_path.join(format!("event_{}.json", event.id));
        let content = serde_json::to_string_pretty(event)?;
        std::fs::write(path, content)?;
        
        // Sincronizar con Graphiti
        if let Some(adapter) = &self.graphiti_adapter {
            let _ = adapter.ingest_event(event).await;
        }
        
        Ok(())
    }
    
    async fn persist_snapshot(&self, snapshot: &TemporalSnapshot) -> Result<()> {
        let path = self.config.storage_path.join(format!("snapshot_{}.json", snapshot.id));
        let content = serde_json::to_string_pretty(snapshot)?;
        std::fs::write(path, content)?;
        Ok(())
    }
    
    fn load(&self) -> Result<()> {
        let storage_path = &self.config.storage_path;
        if !storage_path.exists() {
            return Ok(());
        }
        
        let mut graph = self.graph.write().unwrap();
        let mut index = self.index.write().unwrap();
        
        for entry in std::fs::read_dir(storage_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let content = std::fs::read_to_string(path)?;
                if path.file_name().unwrap().to_string_lossy().starts_with("event_") {
                    if let Ok(event) = serde_json::from_str::<TemporalEvent>(&content) {
                        let idx = graph.events.len();
                        graph.events.push(event.clone());
                        let _ = self.update_index(idx, &event);
                    }
                } else if path.file_name().unwrap().to_string_lossy().starts_with("snapshot_") {
                    if let Ok(snapshot) = serde_json::from_str::<TemporalSnapshot>(&content) {
                        let mut snapshots = self.snapshots.write().unwrap();
                        snapshots.push(snapshot);
                    }
                }
            }
        }
        
        Ok(())
    }
}

/// Diferencia entre snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDiff {
    pub from: Uuid,
    pub to: Uuid,
    pub added: Vec<(Uuid, serde_json::Value)>,
    pub removed: Vec<(Uuid, serde_json::Value)>,
    pub modified: Vec<(Uuid, serde_json::Value, serde_json::Value)>,
}

/// Patrón de cambio detectado
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePattern {
    pub pattern_type: ChangePatternType,
    pub description: String,
    pub confidence: f32,
    pub affected_entities: Vec<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangePatternType {
    HighFrequency,
    Oscillation,
    Cascading,
    Stagnation,
    Regression,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_temporal_memory_creation() {
        let config = TemporalConfig::default();
        let memory = TemporalMemory::new(config);
        assert!(memory.is_ok());
    }
    
    #[tokio::test]
    async fn test_record_event() {
        let config = TemporalConfig::default();
        let memory = TemporalMemory::new(config).unwrap();
        
        let event = TemporalEvent {
            id: Uuid::new_v4(),
            entity_id: Uuid::new_v4(),
            entity_type: EntityType::File,
            event_type: TemporalEventType::Modified,
            timestamp: Utc::now(),
            actor: "agent-1".to_string(),
            description: "Updated function signature".to_string(),
            before_state: Some(serde_json::json!({"name": "old_func"})),
            after_state: Some(serde_json::json!({"name": "new_func"})),
            diff: Some(serde_json::json!({"name": ["old_func", "new_func"]})),
            reasoning: Some("Renamed for clarity".to_string()),
            tags: vec!["refactor".to_string()],
            confidence: 0.9,
            metadata: HashMap::new(),
        };
        
        let id = memory.record_event(event).await.unwrap();
        assert!(!id.is_nil());
        
        let timeline = memory.get_entity_timeline(event.entity_id, 10).unwrap();
        assert_eq!(timeline.len(), 1);
    }
}