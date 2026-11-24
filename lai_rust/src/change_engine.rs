//! ChangeEngine - Motor de cambios controlados con diff/patch y checkpoints.
//! Implementa aplicación segura de cambios con rollback automático.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Execution state for the FSM
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionState {
    Idle,
    Planning,
    Authorizing,
    Executing,
    Verifying,
    Diagnosing,
    Repairing,
    Checkpointing,
    RollingBack,
    Completed,
    Failed,
    Blocked,
    Aborted,
}

/// Execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub final_state: ExecutionState,
    pub completed_steps: Vec<String>,
    pub failed_step: Option<String>,
    pub error: Option<String>,
    pub duration: Duration,
}

/// Tipo de operación de cambio
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    Create,
    Update,
    Delete,
    Move,
}

/// Representa un cambio individual en un archivo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub change_type: ChangeType,
    pub file_path: PathBuf,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub diff: Option<String>,
}

/// Conjunto de cambios para una operación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSet {
    pub id: String,
    pub description: String,
    pub changes: Vec<FileChange>,
    pub created_at: String,
    pub author: String,
}

/// Checkpoint para rollback
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub changeset_id: String,
    pub timestamp: String,
    pub file_snapshots: HashMap<PathBuf, String>,
    /// Files created by the changeset. They are removed on rollback.
    #[serde(default)]
    pub created_files: Vec<PathBuf>,
    pub metadata: HashMap<String, String>,
}

/// Motor de cambios con soporte para checkpoints y rollback
pub struct ChangeEngine {
    workspace_root: PathBuf,
    checkpoints: Arc<RwLock<HashMap<String, Checkpoint>>>,
    pending_changes: Arc<RwLock<Vec<FileChange>>>,
    config: ChangeEngineConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEngineConfig {
    pub max_checkpoints: usize,
    pub auto_checkpoint: bool,
    pub checkpoint_dir: PathBuf,
    pub max_diff_size: usize,
    pub require_confirmation: bool,
}

impl Default for ChangeEngineConfig {
    fn default() -> Self {
        Self {
            max_checkpoints: 50,
            auto_checkpoint: true,
            checkpoint_dir: PathBuf::from(".lai/checkpoints"),
            max_diff_size: 100_000,
            require_confirmation: false,
        }
    }
}

impl ChangeEngine {
    pub fn new(workspace_root: PathBuf, mut config: ChangeEngineConfig) -> Result<Self> {
        // Keep checkpoints inside the workspace when a relative path is used.
        if config.checkpoint_dir.is_relative() {
            config.checkpoint_dir = workspace_root.join(&config.checkpoint_dir);
        }
        fs::create_dir_all(&config.checkpoint_dir)
            .with_context(|| format!("creating checkpoint dir {}", config.checkpoint_dir.display()))?;

        Ok(Self {
            workspace_root,
            checkpoints: Arc::new(RwLock::new(HashMap::new())),
            pending_changes: Arc::new(RwLock::new(Vec::new())),
            config,
        })
    }

    /// Añadir un cambio pendiente
    pub fn add_change(&self, change: FileChange) -> Result<()> {
        if let Some(diff) = &change.diff {
            if diff.len() > self.config.max_diff_size {
                return Err(anyhow::anyhow!("Diff too large: {} bytes (max: {})", diff.len(), self.config.max_diff_size));
            }
        }
        self.pending_changes.write().unwrap().push(change);
        Ok(())
    }

    /// Aplicar todos los cambios pendientes
    pub fn apply_changes(&self, description: &str) -> Result<ChangeSet> {
        // Take the pending changes out before creating the checkpoint: holding
        // this write lock while create_checkpoint() reads it would deadlock.
        let changes: Vec<FileChange> = {
            let mut pending = self.pending_changes.write().unwrap();
            if pending.is_empty() {
                return Err(anyhow::anyhow!("No pending changes to apply"));
            }
            std::mem::take(&mut *pending)
        };

        let changeset_id = uuid::Uuid::new_v4().to_string();

        // Crear checkpoint antes de aplicar si está habilitado
        let checkpoint_id = if self.config.auto_checkpoint {
            match self.create_checkpoint_for(&changes, &changeset_id, "Pre-apply checkpoint") {
                Ok(id) => Some(id),
                Err(e) => {
                    self.pending_changes.write().unwrap().extend(changes);
                    return Err(e);
                }
            }
        } else {
            None
        };

        let mut applied_changes = Vec::with_capacity(changes.len());

        for change in &changes {
            if let Err(e) = self.apply_single_change(change) {
                // Restore the pre-apply state before returning the error.
                if let Some(id) = &checkpoint_id {
                    let _ = self.rollback(id);
                }
                self.pending_changes.write().unwrap().extend(changes);
                return Err(e);
            }
            applied_changes.push(change.clone());
        }

        let changeset = ChangeSet {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.to_string(),
            changes: applied_changes,
            created_at: chrono::Utc::now().to_rfc3339(),
            author: "lai".to_string(),
        };

        Ok(changeset)
    }

    /// Aplicar un cambio individual
    fn apply_single_change(&self, change: &FileChange) -> Result<()> {
        let full_path = self.workspace_root.join(&change.file_path);

        match change.change_type {
            ChangeType::Create => {
                if full_path.exists() {
                    return Err(anyhow::anyhow!("File already exists: {}", change.file_path.display()));
                }
                if let Some(parent) = full_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                if let Some(content) = &change.new_content {
                    fs::write(&full_path, content)?;
                }
            }
            ChangeType::Update => {
                if !full_path.exists() {
                    return Err(anyhow::anyhow!("File does not exist: {}", change.file_path.display()));
                }
                if let Some(content) = &change.new_content {
                    fs::write(&full_path, content)?;
                }
            }
            ChangeType::Delete => {
                if full_path.exists() {
                    fs::remove_file(&full_path)?;
                }
            }
            ChangeType::Move => {
                if let (Some(_old), Some(new)) = (&change.old_content, &change.new_content) {
                    let new_path = self.workspace_root.join(new);
                    if let Some(parent) = new_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(&full_path, &new_path)?;
                }
            }
        }
        Ok(())
    }

    /// Crear checkpoint para rollback
    pub fn create_checkpoint(&self, changeset_id: &str, description: &str) -> Result<String> {
        let pending = self.pending_changes.read().unwrap().clone();
        self.create_checkpoint_for(&pending, changeset_id, description)
    }

    /// Crear checkpoint para un conjunto concreto de cambios.
    ///
    /// No adquiere el lock de `pending_changes`, por lo que es seguro llamarlo
    /// mientras el llamador ya tiene esos cambios en su poder.
    fn create_checkpoint_for(
        &self,
        changes: &[FileChange],
        changeset_id: &str,
        description: &str,
    ) -> Result<String> {
        let checkpoint_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let mut snapshots = HashMap::new();
        let mut created_files = Vec::new();

        for change in changes {
            let full_path = self.workspace_root.join(&change.file_path);
            if full_path.exists() {
                let content = fs::read_to_string(&full_path)
                    .with_context(|| format!("snapshotting {}", full_path.display()))?;
                snapshots.insert(change.file_path.clone(), content);
            } else {
                match change.change_type {
                    ChangeType::Create => created_files.push(change.file_path.clone()),
                    ChangeType::Move => {
                        if let Some(new_path) = &change.new_content {
                            created_files.push(PathBuf::from(new_path));
                        }
                    }
                    _ => {}
                }
            }
        }

        let checkpoint = Checkpoint {
            id: checkpoint_id.clone(),
            changeset_id: changeset_id.to_string(),
            timestamp: now,
            file_snapshots: snapshots,
            created_files,
            metadata: {
                let mut m = HashMap::new();
                m.insert("description".to_string(), description.to_string());
                m
            },
        };

        self.checkpoints.write().unwrap().insert(checkpoint_id.clone(), checkpoint);

        // Limpiar checkpoints antiguos si excede el máximo
        self.cleanup_old_checkpoints()?;

        Ok(checkpoint_id)
    }

    /// Rollback a un checkpoint específico
    pub fn rollback(&self, checkpoint_id: &str) -> Result<()> {
        let checkpoint = self.checkpoints.read().unwrap().get(checkpoint_id).cloned()
            .ok_or_else(|| anyhow::anyhow!("Checkpoint not found: {}", checkpoint_id))?;

        // Remove files that did not exist before the changeset.
        for path in &checkpoint.created_files {
            let full_path = self.workspace_root.join(path);
            if full_path.is_dir() {
                fs::remove_dir_all(&full_path)?;
            } else if full_path.exists() {
                fs::remove_file(&full_path)?;
            }
        }

        for (path, content) in &checkpoint.file_snapshots {
            let full_path = self.workspace_root.join(path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&full_path, content)?;
        }

        // Eliminar checkpoint después de rollback
        self.checkpoints.write().unwrap().remove(checkpoint_id);
        
        Ok(())
    }

    /// Listar checkpoints disponibles
    pub fn list_checkpoints(&self) -> Vec<Checkpoint> {
        self.checkpoints.read().unwrap().values().cloned().collect()
    }

    /// Limpiar checkpoints antiguos
    fn cleanup_old_checkpoints(&self) -> Result<()> {
        let mut checkpoints = self.checkpoints.write().unwrap();
        if checkpoints.len() > self.config.max_checkpoints {
            let mut sorted: Vec<_> = checkpoints.iter().map(|(k, v)| (k.clone(), v.timestamp.clone())).collect();
            sorted.sort_by(|a, b| a.1.cmp(&b.1));
            
            let to_remove = sorted.len() - self.config.max_checkpoints;
            for (id, _) in sorted.iter().take(to_remove) {
                checkpoints.remove(id);
            }
        }
        Ok(())
    }

    /// Obtener cambios pendientes
    pub fn get_pending_changes(&self) -> Vec<FileChange> {
        self.pending_changes.read().unwrap().clone()
    }

    /// Limpiar cambios pendientes
    pub fn clear_pending_changes(&self) {
        self.pending_changes.write().unwrap().clear();
    }
}

/// Generar diff entre dos contenidos
pub fn generate_diff(old: &str, new: &str, file_path: &Path) -> String {
    use difference::{Changeset, Difference};
    
    let changeset = Changeset::new(old, new, "\n");
    let mut output = String::new();
    
    output.push_str(&format!("--- a/{}\n", file_path.display()));
    output.push_str(&format!("+++ b/{}\n", file_path.display()));
    
    for diff in changeset.diffs {
        match diff {
            Difference::Same(text) => {
                for line in text.lines() {
                    output.push_str(&format!(" {}\n", line));
                }
            }
            Difference::Add(text) => {
                for line in text.lines() {
                    output.push_str(&format!("+{}\n", line));
                }
            }
            Difference::Rem(text) => {
                for line in text.lines() {
                    output.push_str(&format!("-{}\n", line));
                }
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_change_engine_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config = ChangeEngineConfig::default();
        let engine = ChangeEngine::new(temp_dir.path().to_path_buf(), config).unwrap();

        // Test create
        engine.add_change(FileChange {
            change_type: ChangeType::Create,
            file_path: PathBuf::from("test.txt"),
            old_content: None,
            new_content: Some("Hello World".to_string()),
            diff: None,
        }).unwrap();

        let changeset = engine.apply_changes("Create test file").unwrap();
        assert_eq!(changeset.changes.len(), 1);
        assert!(temp_dir.path().join("test.txt").exists());

        // Test rollback
        let checkpoints = engine.list_checkpoints();
        assert!(!checkpoints.is_empty());
        
        let checkpoint_id = checkpoints[0].id.clone();
        engine.rollback(&checkpoint_id).unwrap();
        assert!(!temp_dir.path().join("test.txt").exists());
    }
}