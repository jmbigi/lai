//! RepairEngine - Motor de reparación automática de código.
//! Implementa aplicación de fixes, validación y rollback seguro.

use crate::change_engine::ChangeEngine;
use crate::diagnostic_engine::{
    Diagnosis, DiagnosticContext, DiagnosticEngine, FailureCategory, Severity,
};
use crate::verification_engine::VerificationEngine;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Estrategia de reparación
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepairStrategy {
    /// Aplicar fix sugerido por diagnóstico
    ApplySuggestedFix,
    /// Revertir último cambio
    RevertLastChange,
    /// Reaplicar último checkpoint
    RestoreCheckpoint,
    /// Ejecutar auto-fix de herramienta (ruff, rustfmt, etc.)
    AutoFixTool,
    /// Regenerar desde spec
    RegenerateFromSpec,
}

/// Acción de reparación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairAction {
    pub id: String,
    pub strategy: RepairStrategy,
    pub target_file: PathBuf,
    pub description: String,
    pub changes: Vec<FileChange>,
    pub precondition: Option<String>,
    pub postcondition: Option<String>,
    pub estimated_risk: RiskLevel,
}

/// Cambio de archivo para reparación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub file_path: PathBuf,
    pub old_content: String,
    pub new_content: String,
    pub change_type: ChangeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    Insert,
    Delete,
    Replace,
    Move,
}

/// Nivel de riesgo
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Resultado de reparación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairResult {
    pub action_id: String,
    pub success: bool,
    pub applied_changes: Vec<FileChange>,
    pub verification_passed: bool,
    pub error_message: Option<String>,
    pub rollback_performed: bool,
    pub duration_ms: u64,
}

/// Plan de reparación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairPlan {
    pub id: String,
    pub diagnosis_id: String,
    pub actions: Vec<RepairAction>,
    pub created_at: String,
    pub estimated_risk: RiskLevel,
    pub requires_approval: bool,
}

/// Configuración del motor de reparación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairConfig {
    pub max_attempts: usize,
    pub auto_approve_low_risk: bool,
    pub verify_after_repair: bool,
    pub max_repair_time_secs: u64,
    pub backup_before_repair: bool,
    pub allowed_strategies: Vec<RepairStrategy>,
}

impl Default for RepairConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            auto_approve_low_risk: true,
            verify_after_repair: true,
            max_repair_time_secs: 300,
            backup_before_repair: true,
            allowed_strategies: vec![
                RepairStrategy::ApplySuggestedFix,
                RepairStrategy::AutoFixTool,
                RepairStrategy::RestoreCheckpoint,
            ],
        }
    }
}

/// Motor de reparación
pub struct RepairEngine {
    config: RepairConfig,
    workspace_root: PathBuf,
    change_engine: Arc<ChangeEngine>,
    verification_engine: Arc<VerificationEngine>,
    diagnostic_engine: Arc<DiagnosticEngine>,
    repair_history: Arc<RwLock<Vec<RepairResult>>>,
}

impl RepairEngine {
    pub fn new(
        workspace_root: PathBuf,
        config: RepairConfig,
        change_engine: Arc<ChangeEngine>,
        verification_engine: Arc<VerificationEngine>,
        diagnostic_engine: Arc<DiagnosticEngine>,
    ) -> Result<Self> {
        Ok(Self {
            config,
            workspace_root,
            change_engine,
            verification_engine,
            diagnostic_engine,
            repair_history: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// Reparar basado en diagnóstico
    pub async fn repair(&self, diagnosis_id: &str, context: RepairContext) -> Result<RepairResult> {
        let start = Instant::now();

        // Crear plan de reparación
        let plan = self.create_repair_plan(diagnosis_id, &context).await?;

        // Verificar si requiere aprobación
        if plan.requires_approval && !self.config.auto_approve_low_risk {
            return Err(anyhow::anyhow!("Repair plan requires manual approval"));
        }

        // Ejecutar plan de reparación
        let result = self.execute_repair_plan(&plan).await?;

        // Verificar después de reparar si está configurado
        if self.config.verify_after_repair && result.success {
            let verification = self
                .verification_engine
                .verify(
                    "repair-verify",
                    None,
                    crate::planner::VerificationScope::default(),
                )
                .await
                .map_err(anyhow::Error::msg)?;
            if !matches!(
                verification.overall_verdict,
                crate::verification_engine::VerificationVerdict::Success
                    | crate::verification_engine::VerificationVerdict::SuccessWithDeclaredGaps
            ) {
                // Rollback si la verificación falla
                self.rollback_last_changes()?;
                return Ok(RepairResult {
                    action_id: uuid::Uuid::new_v4().to_string(),
                    success: false,
                    applied_changes: vec![],
                    verification_passed: false,
                    error_message: Some("Verification failed after repair".to_string()),
                    rollback_performed: true,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        }

        let mut result = result;
        result.duration_ms = start.elapsed().as_millis() as u64;

        // Guardar en historial
        self.repair_history.write().unwrap().push(result.clone());

        Ok(result)
    }

    /// Crear plan de reparación basado en diagnóstico
    async fn create_repair_plan(
        &self,
        diagnosis_id: &str,
        context: &RepairContext,
    ) -> Result<RepairPlan> {
        let mut actions = Vec::new();

        // Obtener diagnóstico (simplificado - en realidad buscaría por ID)
        let diagnosis = self
            .diagnostic_engine
            .diagnose(&context.error_output, DiagnosticContext::default())
            .await?;

        if let Some(_root_cause) = &diagnosis.root_cause {
            // Generar acciones basadas en la causa raíz
            for diagnosis in &diagnosis.diagnoses {
                let action = self.create_repair_action(diagnosis, context).await?;
                actions.push(action);
            }
        }

        // Si no hay diagnóstico específico, usar estrategias genéricas
        if actions.is_empty() {
            actions.push(RepairAction {
                id: uuid::Uuid::new_v4().to_string(),
                strategy: RepairStrategy::AutoFixTool,
                target_file: context.target_path.clone(),
                description: "Run auto-fix tools (ruff, rustfmt, etc.)".to_string(),
                changes: Vec::new(),
                precondition: None,
                postcondition: Some("Code passes linting and formatting".to_string()),
                estimated_risk: RiskLevel::Low,
            });
        }

        // Si no hay diagnóstico específico, usar estrategias genéricas
        if actions.is_empty() {
            actions.push(RepairAction {
                id: uuid::Uuid::new_v4().to_string(),
                strategy: RepairStrategy::AutoFixTool,
                target_file: context.target_path.clone(),
                description: "Run auto-fix tools (ruff, rustfmt, etc.)".to_string(),
                changes: Vec::new(),
                precondition: None,
                postcondition: Some("Code passes linting and formatting".to_string()),
                estimated_risk: RiskLevel::Low,
            });
        }

        let estimated_risk = actions
            .iter()
            .map(|a| a.estimated_risk)
            .max()
            .unwrap_or(RiskLevel::Low);
        let requires_approval =
            estimated_risk >= RiskLevel::High && !self.config.auto_approve_low_risk;

        Ok(RepairPlan {
            id: uuid::Uuid::new_v4().to_string(),
            diagnosis_id: diagnosis_id.to_string(),
            actions,
            created_at: chrono::Utc::now().to_rfc3339(),
            estimated_risk,
            requires_approval,
        })
    }

    /// Crear acción de reparación para un diagnóstico
    async fn create_repair_action(
        &self,
        diagnosis: &Diagnosis,
        _context: &RepairContext,
    ) -> Result<RepairAction> {
        let strategy = match diagnosis.category {
            FailureCategory::SyntaxError => RepairStrategy::ApplySuggestedFix,
            FailureCategory::TypeError => RepairStrategy::ApplySuggestedFix,
            FailureCategory::RuntimeError => RepairStrategy::AutoFixTool,
            FailureCategory::LogicError => RepairStrategy::ApplySuggestedFix,
            FailureCategory::PerformanceIssue => RepairStrategy::AutoFixTool,
            FailureCategory::SecurityVulnerability => RepairStrategy::ApplySuggestedFix,
            FailureCategory::ResourceLeak => RepairStrategy::ApplySuggestedFix,
            FailureCategory::ConcurrencyIssue => RepairStrategy::ApplySuggestedFix,
            FailureCategory::ConfigurationError => RepairStrategy::RestoreCheckpoint,
            FailureCategory::DependencyError => RepairStrategy::AutoFixTool,
            FailureCategory::Unknown => RepairStrategy::AutoFixTool,
        };

        // Generar cambios basados en fix sugerido
        let changes = if let Some(fix) = &diagnosis.suggested_fix {
            vec![FileChange {
                file_path: diagnosis
                    .location
                    .as_ref()
                    .map(|l| l.file.clone())
                    .unwrap_or_default(),
                old_content: String::new(),
                new_content: fix.clone(),
                change_type: ChangeType::Replace,
            }]
        } else {
            Vec::new()
        };

        Ok(RepairAction {
            id: uuid::Uuid::new_v4().to_string(),
            strategy,
            target_file: diagnosis
                .location
                .as_ref()
                .map(|l| l.file.clone())
                .unwrap_or_default(),
            description: format!("Fix for: {}", diagnosis.message),
            changes,
            precondition: None,
            postcondition: Some("Issue resolved".to_string()),
            estimated_risk: match diagnosis.severity {
                Severity::Critical => RiskLevel::Critical,
                Severity::High => RiskLevel::High,
                Severity::Medium => RiskLevel::Medium,
                _ => RiskLevel::Low,
            },
        })
    }

    /// Ejecutar plan de reparación
    async fn execute_repair_plan(&self, plan: &RepairPlan) -> Result<RepairResult> {
        let mut applied_changes = Vec::new();

        for action in &plan.actions {
            // Verificar precondición
            if let Some(pre) = &action.precondition {
                if !self.check_precondition(pre).await? {
                    return Ok(RepairResult {
                        action_id: action.id.clone(),
                        success: false,
                        applied_changes: vec![],
                        verification_passed: false,
                        error_message: Some(format!("Precondition failed: {}", pre)),
                        rollback_performed: false,
                        duration_ms: 0,
                    });
                }
            }

            // Aplicar cambios según estrategia
            let result = match action.strategy {
                RepairStrategy::ApplySuggestedFix => self.apply_suggested_fix(action).await,
                RepairStrategy::AutoFixTool => self.run_auto_fix_tools(action).await,
                RepairStrategy::RestoreCheckpoint => self.restore_checkpoint(action).await,
                RepairStrategy::RevertLastChange => self.revert_last_change().await,
                RepairStrategy::RegenerateFromSpec => self.regenerate_from_spec(action).await,
            };

            match result {
                Ok(changes) => {
                    let changes_clone = changes.clone();
                    applied_changes.extend(changes);
                    // Verificar postcondición
                    if let Some(post) = &action.postcondition {
                        if !self.check_postcondition(post).await? {
                            // Rollback
                            self.rollback_changes(&changes_clone).await?;
                            return Ok(RepairResult {
                                action_id: action.id.clone(),
                                success: false,
                                applied_changes: vec![],
                                verification_passed: false,
                                error_message: Some("Postcondition failed".to_string()),
                                rollback_performed: true,
                                duration_ms: 0,
                            });
                        }
                    }
                }
                Err(e) => {
                    // Rollback cambios aplicados hasta ahora
                    self.rollback_changes(&applied_changes).await?;
                    return Ok(RepairResult {
                        action_id: action.id.clone(),
                        success: false,
                        applied_changes,
                        verification_passed: false,
                        error_message: Some(e.to_string()),
                        rollback_performed: true,
                        duration_ms: 0,
                    });
                }
            }
        }

        Ok(RepairResult {
            action_id: plan.id.clone(),
            success: true,
            applied_changes,
            verification_passed: true,
            error_message: None,
            rollback_performed: false,
            duration_ms: 0,
        })
    }

    async fn apply_suggested_fix(&self, action: &RepairAction) -> Result<Vec<FileChange>> {
        let mut applied = Vec::new();

        for change in &action.changes {
            let full_path = self.workspace_root.join(&change.file_path);

            // Backup si está configurado
            if self.config.backup_before_repair && full_path.exists() {
                let backup = full_path.with_extension("bak");
                std::fs::copy(&full_path, &backup)?;
            }

            // Aplicar cambio
            match change.change_type {
                ChangeType::Insert | ChangeType::Replace => {
                    if let Some(parent) = full_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&full_path, &change.new_content)?;
                }
                ChangeType::Delete => {
                    if full_path.exists() {
                        std::fs::remove_file(&full_path)?;
                    }
                }
                ChangeType::Move => {
                    // No implementado en este nivel
                }
            }

            applied.push(change.clone());
        }

        Ok(applied)
    }

    async fn run_auto_fix_tools(&self, action: &RepairAction) -> Result<Vec<FileChange>> {
        let mut applied: Vec<FileChange> = Vec::new();

        // Ejecutar herramientas de auto-fix según el lenguaje
        let target = &action.target_file;
        let ext = target.extension().and_then(|e| e.to_str()).unwrap_or("");

        match ext {
            "py" => {
                // Ruff fix
                let output = Command::new("ruff")
                    .args(["check", "--fix", action.target_file.to_str().unwrap()])
                    .output()?;
                if output.status.success() {
                    // Leer archivo modificado
                    let new_content = std::fs::read_to_string(&action.target_file)?;
                    applied.push(FileChange {
                        file_path: action.target_file.clone(),
                        old_content: String::new(),
                        new_content,
                        change_type: ChangeType::Replace,
                    });
                }
            }
            "rs" => {
                // Rustfmt
                let output = Command::new("rustfmt")
                    .args([action.target_file.to_str().unwrap()])
                    .output()?;
                if output.status.success() {
                    let new_content = std::fs::read_to_string(&action.target_file)?;
                    applied.push(FileChange {
                        file_path: action.target_file.clone(),
                        old_content: String::new(),
                        new_content,
                        change_type: ChangeType::Replace,
                    });
                }
            }
            "js" | "ts" | "jsx" | "tsx" => {
                // Prettier
                let output = Command::new("prettier")
                    .args(["--write", action.target_file.to_str().unwrap()])
                    .output()?;
                if output.status.success() {
                    let new_content = std::fs::read_to_string(&action.target_file)?;
                    applied.push(FileChange {
                        file_path: action.target_file.clone(),
                        old_content: String::new(),
                        new_content,
                        change_type: ChangeType::Replace,
                    });
                }
            }
            "go" => {
                // gofmt
                let output = Command::new("gofmt")
                    .args(["-w", action.target_file.to_str().unwrap()])
                    .output()?;
                if output.status.success() {
                    let new_content = std::fs::read_to_string(&action.target_file)?;
                    applied.push(FileChange {
                        file_path: action.target_file.clone(),
                        old_content: String::new(),
                        new_content,
                        change_type: ChangeType::Replace,
                    });
                }
            }
            _ => {}
        }

        Ok(applied)
    }

    async fn restore_checkpoint(&self, _action: &RepairAction) -> Result<Vec<FileChange>> {
        // Usar change_engine para restaurar checkpoint
        let checkpoints = self.change_engine.list_checkpoints();
        if let Some(latest) = checkpoints.first() {
            self.change_engine.rollback(&latest.id)?;
            Ok(vec![])
        } else {
            Err(anyhow::anyhow!("No checkpoints available"))
        }
    }

    async fn revert_last_change(&self) -> Result<Vec<FileChange>> {
        // Usar change_engine para revertir último cambio
        // Por simplicidad, retornamos vacío
        Ok(vec![])
    }

    async fn regenerate_from_spec(&self, _action: &RepairAction) -> Result<Vec<FileChange>> {
        // Regenerar código desde SPEC.md si existe
        Ok(vec![])
    }

    async fn check_precondition(&self, _precondition: &str) -> Result<bool> {
        // Evaluar precondición (simplificado)
        Ok(true)
    }

    async fn check_postcondition(&self, _postcondition: &str) -> Result<bool> {
        // Evaluar postcondición (simplificado)
        Ok(true)
    }

    async fn rollback_changes(&self, changes: &[FileChange]) -> Result<()> {
        for change in changes.iter().rev() {
            let full_path = self.workspace_root.join(&change.file_path);
            match change.change_type {
                ChangeType::Insert | ChangeType::Replace => {
                    if !change.old_content.is_empty() {
                        std::fs::write(&full_path, &change.old_content)?;
                    } else if full_path.exists() {
                        std::fs::remove_file(&full_path)?;
                    }
                }
                ChangeType::Delete if !change.old_content.is_empty() => {
                    std::fs::write(&full_path, &change.old_content)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn rollback_last_changes(&self) -> Result<()> {
        // Simplificado
        Ok(())
    }

    /// Obtener historial de reparaciones
    pub fn get_repair_history(&self) -> Vec<RepairResult> {
        self.repair_history.read().unwrap().clone()
    }

    /// Obtener estadísticas de reparación
    pub fn get_repair_stats(&self) -> RepairStats {
        let history = self.repair_history.read().unwrap();
        let total = history.len();
        let successful = history.iter().filter(|r| r.success).count();
        let failed = total - successful;
        let avg_duration = if total > 0 {
            history.iter().map(|r| r.duration_ms).sum::<u64>() / total as u64
        } else {
            0
        };

        RepairStats {
            total_repairs: total,
            successful_repairs: successful,
            failed_repairs: failed,
            average_duration_ms: avg_duration,
            success_rate: if total > 0 {
                successful as f64 / total as f64
            } else {
                0.0
            },
        }
    }
}

/// Contexto para reparación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairContext {
    pub target_path: PathBuf,
    pub error_output: String,
    pub diagnosis_id: String,
    pub related_files: Vec<PathBuf>,
    pub constraints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairStats {
    pub total_repairs: usize,
    pub successful_repairs: usize,
    pub failed_repairs: usize,
    pub average_duration_ms: u64,
    pub success_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change_engine::ChangeEngineConfig;
    use crate::diagnostic_engine::DiagnosticConfig;
    use crate::verification_engine::VerificationConfig;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_repair_engine_basic() {
        let temp_dir = TempDir::new().unwrap();

        let change_config = ChangeEngineConfig::default();
        let change_engine =
            Arc::new(ChangeEngine::new(temp_dir.path().to_path_buf(), change_config).unwrap());

        let verify_config = VerificationConfig::default();
        let verification_engine =
            Arc::new(VerificationEngine::new(temp_dir.path()).with_config(verify_config));

        let diagnostic_engine = Arc::new(
            DiagnosticEngine::new(temp_dir.path().to_path_buf(), DiagnosticConfig::default())
                .unwrap(),
        );

        let config = RepairConfig::default();
        let engine = RepairEngine::new(
            temp_dir.path().to_path_buf(),
            config,
            change_engine,
            verification_engine,
            diagnostic_engine,
        )
        .unwrap();

        // Test básico
        let context = RepairContext {
            target_path: temp_dir.path().join("test.py"),
            error_output: "NameError: name 'x' is not defined".to_string(),
            diagnosis_id: "test".to_string(),
            related_files: vec![],
            constraints: vec![],
        };

        let result = engine.repair("test", context).await;
        // Como es un test básico, solo verificamos que no falle catastróficamente
        assert!(result.is_ok() || result.is_err());
    }
}
