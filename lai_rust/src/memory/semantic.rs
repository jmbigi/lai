//! memory/semantic.rs - Memoria semántica
//! 
//! Almacena conocimiento estructurado: arquitectura, dependencias, constraints, preferencias, contratos.

use crate::core::contracts::{ExecutionResult, ResultStatus};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Configuración de memoria semántica
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticConfig {
    pub storage_path: PathBuf,
    pub enable_graph: bool,
    pub neo4j_uri: Option<String>,
    pub neo4j_user: Option<String>,
    pub neo4j_password: Option<String>,
}

impl Default for SemanticConfig {
    fn default() -> Self {
        Self {
            storage_path: PathBuf::from("./data/memory/semantic"),
            enable_graph: false,
            neo4j_uri: None,
            neo4j_user: None,
            neo4j_password: None,
        }
    }
}

/// Concepto en la memoria semántica
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub id: Uuid,
    pub name: String,
    pub concept_type: ConceptType,
    pub description: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub confidence: f32, // 0.0 - 1.0
    pub source: ConceptSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: u32,
    pub tags: Vec<String>,
}

/// Tipo de concepto
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConceptType {
    Architecture,
    Component,
    Module,
    Interface,
    DataModel,
    ApiEndpoint,
    DatabaseSchema,
    ConfigParameter,
    CodingStandard,
    DesignPattern,
    Dependency,
    Constraint,
    Requirement,
    BusinessRule,
    SecurityPolicy,
    PerformanceTarget,
    TestingStrategy,
    DeploymentTarget,
    Other,
}

/// Origen del concepto
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConceptSource {
    CodeAnalysis,
    Documentation,
    HumanInput,
    Inferred,
    External,
    Verification,
}

/// Relación entre conceptos
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub id: Uuid,
    pub source: Uuid,
    pub target: Uuid,
    pub relation_type: RelationType,
    pub properties: HashMap<String, serde_json::Value>,
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
    pub source: RelationSource,
}

/// Tipo de relación
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    DependsOn,
    Implements,
    Extends,
    Uses,
    Contains,
    Calls,
    References,
    Configures,
    Requires,
    ConflictsWith,
    AlternativeTo,
    Deprecates,
    Replaces,
    PartOf,
    HasProperty,
    HasMethod,
    HasField,
    InheritsFrom,
}

/// Origen de la relación
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationSource {
    StaticAnalysis,
    RuntimeObservation,
    HumanAssertion,
    Inferred,
    Documentation,
}

/// Índice semántico
#[derive(Debug, Default)]
struct SemanticIndex {
    by_id: HashMap<Uuid, usize>,
    by_name: HashMap<String, Vec<usize>>,
    by_type: HashMap<ConceptType, Vec<usize>>,
    by_tag: HashMap<String, Vec<usize>>,
    relations_from: HashMap<Uuid, Vec<usize>>,
    relations_to: HashMap<Uuid, Vec<usize>>,
}

/// Memoria semántica principal
pub struct SemanticMemory {
    config: SemanticConfig,
    concepts: Arc<RwLock<Vec<Concept>>>,
    relations: Arc<RwLock<Vec<Relation>>>,
    index: Arc<RwLock<SemanticIndex>>,
}

impl SemanticMemory {
    pub fn new(config: SemanticConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.storage_path)?;
        
        let memory = Self {
            config,
            concepts: Arc::new(RwLock::new(Vec::new())),
            relations: Arc::new(RwLock::new(Vec::new())),
            index: Arc::new(RwLock::new(SemanticIndex::default())),
        };
        
        memory.load()?;
        Ok(memory)
    }
    
    /// Añade o actualiza un concepto
    pub async fn upsert_concept(&self, mut concept: Concept) -> Result<Uuid> {
        let now = Utc::now();
        
        if concept.id.is_nil() {
            concept.id = Uuid::new_v4();
            concept.created_at = now;
        }
        concept.updated_at = now;
        concept.version += 1;
        
        let idx = {
            let mut concepts = self.concepts.write().unwrap();
            let idx = concepts.len();
            concepts.push(concept.clone());
            idx
        };
        
        self.update_concept_index(idx, &concept).await?;
        self.persist_concept(&concept).await?;
        
        Ok(concept.id)
    }
    
    /// Obtiene un concepto por ID
    pub fn get_concept(&self, id: Uuid) -> Result<Option<Concept>> {
        let concepts = self.concepts.read().unwrap();
        let index = self.index.read().unwrap();
        
        if let Some(&idx) = index.by_id.get(&id) {
            Ok(Some(concepts[idx].clone()))
        } else {
            Ok(None)
        }
    }
    
    /// Busca conceptos por nombre (fuzzy)
    pub fn find_concepts(&self, name: &str, concept_type: Option<ConceptType>) -> Result<Vec<Concept>> {
        let concepts = self.concepts.read().unwrap();
        let query = name.to_lowercase();
        
        let mut results: Vec<_> = concepts.iter()
            .filter(|c| {
                let name_match = c.name.to_lowercase().contains(&query);
                let type_match = concept_type.map_or(true, |ct| c.concept_type == ct);
                name_match && type_match
            })
            .cloned()
            .collect();
        
        // Ordenar por relevancia (coincidencia exacta primero, luego por confianza)
        results.sort_by(|a, b| {
            let a_exact = a.name.to_lowercase() == name.to_lowercase();
            let b_exact = b.name.to_lowercase() == name.to_lowercase();
            b_exact.cmp(&a_exact)
                .then(b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal))
        });
        
        Ok(results)
    }
    
    /// Busca por tipo
    pub fn get_by_type(&self, concept_type: ConceptType) -> Result<Vec<Concept>> {
        let concepts = self.concepts.read().unwrap();
        let index = self.index.read().unwrap();
        
        if let Some(indices) = index.by_type.get(&concept_type) {
            Ok(indices.iter().map(|&i| concepts[i].clone()).collect())
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Busca por tag
    pub fn get_by_tag(&self, tag: &str) -> Result<Vec<Concept>> {
        let concepts = self.concepts.read().unwrap();
        let index = self.index.read().unwrap();
        
        if let Some(indices) = index.by_tag.get(tag) {
            Ok(indices.iter().map(|&i| concepts[i].clone()).collect())
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Añade una relación
    pub async fn add_relation(&self, mut relation: Relation) -> Result<Uuid> {
        relation.id = Uuid::new_v4();
        relation.created_at = Utc::now();
        
        let idx = {
            let mut relations = self.relations.write().unwrap();
            let idx = relations.len();
            relations.push(relation.clone());
            idx
        };
        
        self.update_relation_index(idx, &relation).await?;
        self.persist_relation(&relation).await?;
        
        Ok(relation.id)
    }
    
    /// Obtiene relaciones salientes
    pub fn get_outgoing_relations(&self, concept_id: Uuid) -> Result<Vec<Relation>> {
        let relations = self.relations.read().unwrap();
        let index = self.index.read().unwrap();
        
        if let Some(indices) = index.relations_from.get(&concept_id) {
            Ok(indices.iter().map(|&i| relations[i].clone()).collect())
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Obtiene relaciones entrantes
    pub fn get_incoming_relations(&self, concept_id: Uuid) -> Result<Vec<Relation>> {
        let relations = self.relations.read().unwrap();
        let index = self.index.read().unwrap();
        
        if let Some(indices) = index.relations_to.get(&concept_id) {
            Ok(indices.iter().map(|&i| relations[i].clone()).collect())
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Obtiene vecinos en el grafo
    pub fn get_neighbors(&self, concept_id: Uuid, relation_types: Option<Vec<RelationType>>, max_depth: usize) -> Result<Vec<Concept>> {
        let mut visited = std::collections::HashSet::new();
        let mut result = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        
        queue.push_back((concept_id, 0));
        visited.insert(concept_id);
        
        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= max_depth { continue; }
            
            let outgoing = self.get_outgoing_relations(current_id).unwrap_or_default();
            let incoming = self.get_incoming_relations(current_id).unwrap_or_default();
            
            for relation in outgoing.into_iter().chain(incoming) {
                let neighbor_id = if relation.source == current_id { relation.target } else { relation.source };
                
                if let Some(relation_types) = &relation_types {
                    if !relation_types.contains(&relation.relation_type) {
                        continue;
                    }
                }
                
                if visited.insert(neighbor_id) {
                    if let Some(concept) = self.get_concept(neighbor_id).unwrap_or(None) {
                        result.push(concept);
                    }
                    queue.push_back((neighbor_id, depth + 1));
                }
            }
        }
        
        Ok(result)
    }
    
    /// Calcula la centralidad de un concepto
    pub fn concept_centrality(&self, concept_id: Uuid) -> f64 {
        let relations = self.relations.read().unwrap();
        let outgoing = relations.iter().filter(|r| r.source == concept_id).count();
        let incoming = relations.iter().filter(|r| r.target == concept_id).count();
        
        // PageRank simplificado
        (outgoing + incoming) as f64 / (relations.len().max(1) as f64)
    }
    
    async fn update_concept_index(&self, idx: usize, concept: &Concept) -> Result<()> {
        let mut index = self.index.write().unwrap();
        
        index.by_id.insert(concept.id, idx);
        index.by_name.entry(concept.name.to_lowercase()).or_default().push(idx);
        index.by_type.entry(concept.concept_type).or_default().push(idx);
        
        for tag in &concept.tags {
            index.by_tag.entry(tag.clone()).or_default().push(idx);
        }
        
        Ok(())
    }
    
    async fn update_relation_index(&self, idx: usize, relation: &Relation) -> Result<()> {
        let mut index = self.index.write().unwrap();
        
        index.relations_from.entry(relation.source).or_default().push(idx);
        index.relations_to.entry(relation.target).or_default().push(idx);
        
        Ok(())
    }
    
    async fn persist_concept(&self, concept: &Concept) -> Result<()> {
        let path = self.config.storage_path.join(format!("concept_{}.json", concept.id));
        let content = serde_json::to_string_pretty(concept)?;
        std::fs::write(path, content)?;
        Ok(())
    }
    
    async fn persist_relation(&self, relation: &Relation) -> Result<()> {
        let path = self.config.storage_path.join(format!("relation_{}.json", relation.id));
        let content = serde_json::to_string_pretty(relation)?;
        std::fs::write(path, content)?;
        Ok(())
    }
    
    fn load(&self) -> Result<()> {
        let storage_path = &self.config.storage_path;
        if !storage_path.exists() {
            return Ok(());
        }
        
        let mut concepts = self.concepts.write().unwrap();
        let mut relations = self.relations.write().unwrap();
        let mut index = self.index.write().unwrap();
        
        for entry in std::fs::read_dir(&self.config.storage_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let content = std::fs::read_to_string(path)?;
                if path.file_name().unwrap().to_string_lossy().starts_with("concept_") {
                    if let Ok(concept) = serde_json::from_str::<Concept>(&content) {
                        let idx = concepts.len();
                        concepts.push(concept.clone());
                        let _ = self.update_concept_index(idx, &concept);
                    }
                } else if path.file_name().unwrap().to_string_lossy().starts_with("relation_") {
                    if let Ok(relation) = serde_json::from_str::<Relation>(&content) {
                        let idx = relations.len();
                        relations.push(relation.clone());
                        let _ = self.update_relation_index(idx, &relation);
                    }
                }
            }
        }
        
        Ok(())
    }
}

/// Estadísticas semánticas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticStats {
    pub total_concepts: usize,
    pub total_relations: usize,
    pub type_distribution: HashMap<ConceptType, usize>,
    pub avg_confidence: f32,
    pub most_central_concepts: Vec<(String, f64)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_semantic_memory_creation() {
        let config = SemanticConfig::default();
        let memory = SemanticMemory::new(config);
        assert!(memory.is_ok());
    }
    
    #[tokio::test]
    async fn test_upsert_concept() {
        let config = SemanticConfig::default();
        let memory = SemanticMemory::new(config).unwrap();
        
        let concept = Concept {
            id: Uuid::new_v4(),
            name: "UserService".to_string(),
            concept_type: ConceptType::Component,
            description: "Service for managing users".to_string(),
            properties: HashMap::new(),
            confidence: 0.9,
            source: ConceptSource::CodeAnalysis,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            tags: vec!["service".to_string(), "user".to_string()],
        };
        
        let id = memory.upsert_concept(concept).await.unwrap();
        assert!(!id.is_nil());
        
        let retrieved = memory.get_concept(id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "UserService");
    }
}