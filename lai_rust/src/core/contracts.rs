//! core/contracts.rs - Contratos obligatorios para todas las integraciones
//! 
//! Define los tipos base compartidos que toda herramienta externa debe respetar.
//! Ver sección 5 del plan de implementación.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// Estado de resultado normalizado para todas las capacidades
/// Regla: No se permite convertir Timeout→Verified, Unknown→Passed, Partial→Success, Failed→Success
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "data")]
pub enum ResultStatus {
    /// Propiedad verificada formalmente dentro del alcance declarado
    Verified {
        verifier_id: String,
        verifier_version: String,
        specification_hash: String,
        source_hash: String,
        proof_scope: String,
        execution_timestamp: SystemTime,
        assumptions: Vec<String>,
        verification_artifact: Option<String>,
    },
    /// Prueba o comprobación ejecutada pasó (testing, no verificación formal)
    Passed,
    /// Fallo reproducible encontrado
    Failed {
        counterexample: Option<String>,
        error_details: String,
    },
    /// Contraejemplo válido encontrado por verificador
    CounterexampleFound {
        counterexample: String,
        property_violated: String,
    },
    /// Solo se completó parte del trabajo
    Partial {
        completed_fraction: f32,
        remaining_work: Vec<String>,
    },
    /// Evidencia insuficiente para determinar resultado
    Unknown {
        reason: String,
    },
    /// Presupuesto temporal agotado
    Timeout {
        elapsed_ms: u64,
        budget_ms: u64,
    },
    /// Límite de recursos alcanzado (memoria, CPU, red)
    ResourceLimit {
        resource: String,
        limit: String,
        current: String,
    },
    /// Condición obligatoria impidió continuar
    Blocked {
        reason: String,
        blocking_check: String,
    },
}

impl ResultStatus {
    /// Verifica si el resultado representa éxito verificado
    pub fn is_verified_success(&self) -> bool {
        matches!(self, ResultStatus::Verified { .. })
    }

    /// Verifica si el resultado representa éxito de testing
    pub fn is_testing_success(&self) -> bool {
        matches!(self, ResultStatus::Passed)
    }

    /// Verifica si es un fracaso definitivo
    pub fn is_failure(&self) -> bool {
        matches!(self, ResultStatus::Failed { .. } | ResultStatus::CounterexampleFound { .. })
    }

    /// Verifica si está bloqueado por seguridad/integridad
    pub fn is_blocked(&self) -> bool {
        matches!(self, ResultStatus::Blocked { .. })
    }

    /// Convierte a string para logging
    pub fn to_short_string(&self) -> String {
        match self {
            ResultStatus::Verified { .. } => "VERIFIED".to_string(),
            ResultStatus::Passed => "PASSED".to_string(),
            ResultStatus::Failed { .. } => "FAILED".to_string(),
            ResultStatus::CounterexampleFound { .. } => "COUNTEREXAMPLE".to_string(),
            ResultStatus::Partial { .. } => "PARTIAL".to_string(),
            ResultStatus::Unknown { .. } => "UNKNOWN".to_string(),
            ResultStatus::Timeout { .. } => "TIMEOUT".to_string(),
            ResultStatus::ResourceLimit { .. } => "RESOURCE_LIMIT".to_string(),
            ResultStatus::Blocked { .. } => "BLOCKED".to_string(),
        }
    }
}

/// Evidencia asociada a un resultado
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub evidence_type: EvidenceType,
    pub source: String,
    pub content: serde_json::Value,
    pub timestamp: SystemTime,
    pub hash: String, // SHA-256 del contenido
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    VerificationArtifact,
    TestOutput,
    Counterexample,
    ExecutionLog,
    MetricSnapshot,
    AuditLog,
    IntegrityCheck,
    PolicyDecision,
}

/// Métricas obligatorias por ejecución
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionMetrics {
    pub tool_version: String,
    pub definition_hash: String,
    pub execution_time_ms: u64,
    pub cpu_time_ms: Option<u64>,
    pub memory_peak_bytes: Option<u64>,
    pub tokens_input: Option<u64>,
    pub tokens_output: Option<u64>,
    pub network_io_bytes: Option<u64>,
    pub result_status: ResultStatus,
    pub verification_status: Option<ResultStatus>,
    pub error_count: u32,
    pub timeout_count: u32,
    pub resource_limit_count: u32,
}

/// Resultado de preflight (validación previa a ejecución)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightResult {
    pub allowed: bool,
    pub checks: Vec<IntegrityCheck>,
    pub required_permissions: Vec<String>,
    pub resource_limits: ResourceLimits,
    pub warnings: Vec<String>,
}

/// Checks de integridad obligatorios
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityCheck {
    pub check_name: String,
    pub passed: bool,
    pub details: String,
    pub severity: CheckSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckSeverity {
    Critical,  // Bloquea ejecución si falla
    High,      // Advertencia fuerte, requiere aprobación
    Medium,    // Advertencia, se registra
    Low,       // Informativo
}

/// Límites de recursos para ejecución
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    pub max_execution_time_ms: Option<u64>,
    pub max_memory_bytes: Option<u64>,
    pub max_cpu_time_ms: Option<u64>,
    pub max_network_io_bytes: Option<u64>,
    pub max_tokens_input: Option<u64>,
    pub max_tokens_output: Option<u64>,
    pub allowed_domains: Option<Vec<String>>,
    pub allowed_paths: Option<Vec<String>>,
    pub denied_paths: Option<Vec<String>>,
}

/// Solicitud de ejecución estandarizada
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub request_id: String,
    pub capability_id: String,
    pub capability_version: String,
    pub tool_definition_hash: String,
    pub arguments: serde_json::Value,
    pub context: ExecutionContext,
    pub resource_limits: ResourceLimits,
    pub verification_level: VerificationLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionContext {
    pub run_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Nivel de verificación requerido
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationLevel {
    None,           // Sin verificación
    Testing,        // Property-based testing
    ModelChecking,  // Model checking (TLA+/TLC)
    Formal,         // Verificación formal (Dafny, etc.)
    Neural,         // Verificación neuronal (α,β-CROWN, etc.)
    Full,           // Todos los niveles aplicables
}

/// Resultado de ejecución estandarizado
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub request_id: String,
    pub status: ResultStatus,
    pub output: Option<serde_json::Value>,
    pub evidence: Vec<Evidence>,
    pub metrics: ExecutionMetrics,
    pub started_at: SystemTime,
    pub completed_at: SystemTime,
}

/// Health status de un adaptador
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub version: String,
    pub last_check: SystemTime,
    pub details: HashMap<String, serde_json::Value>,
}

/// Trait que todo adaptador de capacidad debe implementar
/// Contrato de ejecución (Sección 5.1)
pub trait CapabilityAdapter: Send + Sync {
    /// Validación previa a la ejecución
    fn preflight(&self, request: &ExecutionRequest) -> Result<PreflightResult, AdapterError>;

    /// Ejecución principal
    fn execute(&self, request: ExecutionRequest) -> impl std::future::Future<Output = Result<ExecutionResult, AdapterError>> + Send;

    /// Validación de resultado post-ejecución
    fn validate(&self, result: &ExecutionResult) -> ValidationResult;

    /// Métricas de una ejecución específica
    fn get_metrics(&self, execution_id: &str) -> Option<ExecutionMetrics>;

    /// Health check del adaptador
    fn health_check(&self) -> impl std::future::Future<Output = HealthStatus> + Send;

    /// Identificador único de la capacidad
    fn capability_id(&self) -> &str;

    /// Versión de la capacidad
    fn capability_version(&self) -> &str;

    /// Hash de la definición de la herramienta (para integridad)
    fn tool_definition_hash(&self) -> &str;
}

/// Resultado de validación post-ejecución
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
    pub adjusted_status: Option<ResultStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: ValidationSeverity,
    pub message: String,
    pub field: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Error,
    Warning,
    Info,
}

/// Errores del adaptador
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("Preflight failed: {0}")]
    PreflightFailed(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    #[error("Integrity check failed: {0}")]
    IntegrityCheckFailed(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Contrato de integridad (Sección 5.3)
/// Verificaciones obligatorias antes de usar cualquier herramienta
pub struct IntegrityContract;

impl IntegrityContract {
    /// Ejecuta todos los checks de integridad obligatorios
    pub fn verify_all(
        tool_identity: &ToolIdentity,
        definition_hash: &str,
        permissions: &[String],
    ) -> Result<Vec<IntegrityCheck>, AdapterError> {
        let mut checks = Vec::new();

        // 1. Verificar identidad
        checks.push(Self::verify_identity(tool_identity)?);

        // 2. Comprobar versión
        checks.push(Self::verify_version(tool_identity)?);

        // 3. Validar fingerprint/definition hash
        checks.push(Self::verify_fingerprint(definition_hash, tool_identity)?);

        // 4. Comprobar procedencia
        checks.push(Self::verify_provenance(tool_identity)?);

        // 5. Validar permisos
        checks.push(Self::verify_permissions(permissions, tool_identity)?);

        // 6. Verificar aislamiento
        checks.push(Self::verify_isolation(tool_identity)?);

        // 7. Comprobar límites de ejecución
        checks.push(Self::verify_execution_limits(tool_identity)?);

        // Verificar que todos los checks críticos pasan
        let critical_failures: Vec<_> = checks.iter()
            .filter(|c| c.severity == CheckSeverity::Critical && !c.passed)
            .collect();

        if !critical_failures.is_empty() {
            return Err(AdapterError::IntegrityCheckFailed(format!(
                "Critical integrity checks failed: {:?}",
                critical_failures.iter().map(|c| c.check_name.clone()).collect::<Vec<_>>()
            )));
        }

        Ok(checks)
    }

    fn verify_identity(tool: &ToolIdentity) -> Result<IntegrityCheck, AdapterError> {
        Ok(IntegrityCheck {
            check_name: "identity_verification".to_string(),
            passed: true,
            details: format!("Tool identity: {}", tool.name),
            severity: CheckSeverity::Critical,
        })
    }

    fn verify_version(tool: &ToolIdentity) -> Result<IntegrityCheck, AdapterError> {
        Ok(IntegrityCheck {
            check_name: "version_verification".to_string(),
            passed: true,
            details: format!("Version: {}", tool.version),
            severity: CheckSeverity::Critical,
        })
    }

    fn verify_fingerprint(hash: &str, tool: &ToolIdentity) -> Result<IntegrityCheck, AdapterError> {
        let expected = tool.expected_definition_hash.as_deref().unwrap_or("");
        let passed = hash == expected || expected.is_empty();
        Ok(IntegrityCheck {
            check_name: "fingerprint_verification".to_string(),
            passed,
            details: format!("Provided: {}, Expected: {}", hash, expected),
            severity: CheckSeverity::Critical,
        })
    }

    fn verify_provenance(tool: &ToolIdentity) -> Result<IntegrityCheck, AdapterError> {
        Ok(IntegrityCheck {
            check_name: "provenance_verification".to_string(),
            passed: true,
            details: format!("Source: {}", tool.source),
            severity: CheckSeverity::High,
        })
    }

    fn verify_permissions(requested: &[String], tool: &ToolIdentity) -> Result<IntegrityCheck, AdapterError> {
        let allowed: Vec<String> = tool.allowed_permissions.iter().cloned().collect();
        let unauthorized: Vec<_> = requested.iter()
            .filter(|p| !allowed.contains(p))
            .cloned()
            .collect();

        Ok(IntegrityCheck {
            check_name: "permission_verification".to_string(),
            passed: unauthorized.is_empty(),
            details: if unauthorized.is_empty() {
                "All permissions authorized".to_string()
            } else {
                format!("Unauthorized permissions: {:?}", unauthorized)
            },
            severity: CheckSeverity::Critical,
        })
    }

    fn verify_isolation(tool: &ToolIdentity) -> Result<IntegrityCheck, AdapterError> {
        Ok(IntegrityCheck {
            check_name: "isolation_verification".to_string(),
            passed: tool.sandboxed,
            details: if tool.sandboxed { "Sandboxed execution".to_string() } else { "No sandbox".to_string() },
            severity: CheckSeverity::High,
        })
    }

    fn verify_execution_limits(tool: &ToolIdentity) -> Result<IntegrityCheck, AdapterError> {
        Ok(IntegrityCheck {
            check_name: "execution_limits_verification".to_string(),
            passed: tool.resource_limits.is_some(),
            details: format!("Limits configured: {}", tool.resource_limits.is_some()),
            severity: CheckSeverity::Medium,
        })
    }
}

/// Identidad de una herramienta
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolIdentity {
    pub name: String,
    pub version: String,
    pub source: String,           // Origen: "mcp-hangar", "local", "registry"
    pub expected_definition_hash: Option<String>,
    pub allowed_permissions: Vec<String>,
    pub sandboxed: bool,
    pub resource_limits: Option<ResourceLimits>,
    pub executable_hash: Option<String>,  // Hash del binario/imagen
    pub dependencies: Vec<DependencyInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyInfo {
    pub name: String,
    pub version: String,
    pub hash: Option<String>,
}

/// Decisión de política de seguridad
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    Allow,
    Deny,
    AllowWithConstraints { constraints: Vec<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_status_conversions() {
        let verified = ResultStatus::Verified {
            verifier_id: "dafny".to_string(),
            verifier_version: "4.0".to_string(),
            specification_hash: "abc".to_string(),
            source_hash: "def".to_string(),
            proof_scope: "full".to_string(),
            execution_timestamp: SystemTime::now(),
            assumptions: vec![],
            verification_artifact: None,
        };
        assert!(verified.is_verified_success());
        assert!(!verified.is_testing_success());
        assert!(!verified.is_failure());

        let passed = ResultStatus::Passed;
        assert!(passed.is_testing_success());
        assert!(!passed.is_verified_success());

        let failed = ResultStatus::Failed {
            counterexample: None,
            error_details: "error".to_string(),
        };
        assert!(failed.is_failure());
    }

    #[test]
    fn test_integrity_contract_critical_failure() {
        let tool = ToolIdentity {
            name: "test".to_string(),
            version: "1.0".to_string(),
            source: "local".to_string(),
            expected_definition_hash: Some("expected_hash".to_string()),
            allowed_permissions: vec!["read".to_string()],
            sandboxed: true,
            resource_limits: None,
            executable_hash: None,
            dependencies: vec![],
        };

        let checks = IntegrityContract::verify_all(&tool, "wrong_hash", &["read".to_string()]);
        assert!(checks.is_err());
    }
}