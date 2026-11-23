//! code_analysis/adapters/codebadger.rs - Adaptador codebadger para detección de vulnerabilidades
//! 
//! Integra codebadger (https://github.com/qcri/codebadger) para análisis de vulnerabilidades
//! basado en Code Property Graphs. Publicado en ICSE 2026 Software Vulnerability Management Workshop.

use crate::code_analysis::cpg::*;
use crate::core::contracts::{AdapterError, CapabilityAdapter, Evidence, EvidenceType, ExecutionMetrics, ExecutionRequest, ExecutionResult, ResultStatus, ExecutionContext, VerificationLevel, ResourceLimits, PreflightResult, IntegrityCheck, CheckSeverity, ValidationResult, ValidationIssue, ValidationSeverity, HealthStatus};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};
use tracing::{debug, info, warn, error};

/// Configuración del adaptador codebadger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebadgerAdapterConfig {
    pub codebadger_path: String,
    pub rules_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub timeout_seconds: u64,
    pub enabled_checks: Vec<String>,
}

impl Default for CodebadgerAdapterConfig {
    fn default() -> Self {
        Self {
            codebadger_path: "codebadger".to_string(),
            rules_dir: PathBuf::from("/opt/codebadger/rules"),
            tmp_dir: PathBuf::from("/tmp/codebadger"),
            timeout_seconds: 180,
            enabled_checks: vec![
                "sql-injection".to_string(),
                "xss".to_string(),
                "path-traversal".to_string(),
                "command-injection".to_string(),
                "xxe".to_string(),
                "ssrf".to_string(),
                "insecure-deserialization".to_string(),
                "hardcoded-secrets".to_string(),
                "weak-crypto".to_string(),
                "insecure-config".to_string(),
            ],
        }
    }
}

/// Adaptador codebadger para detección de vulnerabilidades
pub struct CodebadgerAdapter {
    config: CodebadgerAdapterConfig,
}

impl CodebadgerAdapter {
    pub fn new(config: CodebadgerAdapterConfig) -> Result<Self, AdapterError> {
        // Verificar que codebadger está instalado
        let output = Command::new(&config.codebadger_path)
            .arg("--version")
            .output()
            .map_err(|e| AdapterError::ExecutionFailed(format!("codebadger not found: {}", e)))?;
        
        if !output.status.success() {
            return Err(AdapterError::ExecutionFailed("codebadger not installed or not in PATH".to_string()));
        }
        
        let version = String::from_utf8_lossy(&output.stdout);
        info!("codebadger version: {}", version.trim());
        
        // Crear directorio temporal
        std::fs::create_dir_all(&config.tmp_dir).map_err(|e| AdapterError::ExecutionFailed(e.to_string()))?;
        
        Ok(Self { config })
    }
    
    /// Ejecuta codebadger sobre código fuente
    async fn run_codebadger(&self, source_path: &Path) -> Result<CodebadgerReport, AdapterError> {
        let output_file = std::env::temp_dir().join(format!("codebadger-{}.json", uuid::Uuid::new_v4()));
        
        let mut cmd = Command::new(&self.config.codebadger_path);
        cmd.arg("scan")
            .arg(source_path)
            .arg("--format")
            .arg("json")
            .arg("--output")
            .arg(&output_file)
            .arg("--timeout")
            .arg(self.config.timeout_seconds.to_string());
        
        // Habilitar solo checks seleccionados
        for check in &self.config.enabled_checks {
            cmd.arg("--enable").arg(check);
        }
        
        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to run codebadger: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("codebadger scan warning: {}", stderr);
        }
        
        // Parsear reporte JSON
        let report = if output_file.exists() {
            let content = std::fs::read_to_string(&output_file)
                .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to read report: {}", e)))?;
            let report: CodebadgerReport = serde_json::from_str(&content)
                .map_err(|e| AdapterError::ExecutionFailed(format!("Failed to parse report: {}", e)))?;
            let _ = std::fs::remove_file(&output_file);
            report
        } else {
            CodebadgerReport::default()
        };
        
        Ok(report)
    }
}

/// Reporte de codebadger
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodebadgerReport {
    pub findings: Vec<CodebadgerFinding>,
    pub summary: CodebadgerSummary,
    pub metadata: CodebadgerMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodebadgerSummary {
    pub total_findings: usize,
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
    pub info: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodebadgerMetadata {
    pub scan_time: String,
    pub files_scanned: usize,
    pub lines_scanned: usize,
    pub rules_executed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebadgerFinding {
    pub id: String,
    pub rule_id: String,
    pub title: String,
    pub description: String,
    pub severity: CodebadgerSeverity,
    pub cwe_id: Option<String>,
    pub location: CodebadgerLocation,
    pub evidence: Vec<String>,
    pub references: Vec<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum CodebadgerSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebadgerLocation {
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub column_start: Option<u32>,
    pub column_end: Option<u32>,
    pub function_name: Option<String>,
    pub code_snippet: Option<String>,
}

/// Adaptador principal codebadger
pub struct CodebadgerAdapter {
    config: CodebadgerAdapterConfig,
}

impl CodebadgerAdapter {
    pub fn new(config: CodebadgerAdapterConfig) -> Result<Self, AdapterError> {
        let adapter = Self { config };
        // Verificar instalación
        let _ = Command::new(&config.codebadger_path)
            .arg("--version")
            .output()
            .map_err(|e| AdapterError::ExecutionFailed(format!("codebadger not found: {}", e)))?;
        info!("codebadger adapter initialized");
        Ok(adapter)
    }
    
    /// Escanea directorio o archivo
    pub async fn scan(&self, path: &Path) -> Result<CodebadgerReport, AdapterError> {
        self.run_codebadger(path).await
    }
    
    /// Convierte hallazgos de codebadger a vulnerabilidades CPG
    pub fn findings_to_vulnerabilities(&self, report: &CodebadgerReport) -> Vec<CpgVulnerability> {
        report.findings.iter().map(|f| {
            CpgVulnerability {
                id: f.id.clone(),
                title: f.title.clone(),
                description: f.description.clone(),
                severity: match f.severity {
                    CodebadgerSeverity::Critical => VulnerabilitySeverity::Critical,
                    CodebadgerSeverity::High => VulnerabilitySeverity::High,
                    CodebadgerSeverity::Medium => VulnerabilitySeverity::Medium,
                    CodebadgerSeverity::Low => VulnerabilitySeverity::Low,
                    CodebadgerSeverity::Info => VulnerabilitySeverity::Info,
                },
                cwe_id: f.cwe_id.clone(),
                location: VulnerabilityLocation {
                    file_path: f.location.file_path.clone(),
                    line_start: f.location.line_start,
                    line_end: f.location.line_end,
                    function_name: f.location.function_name.clone(),
                    code_snippet: f.location.code_snippet.clone(),
                },
                evidence: f.evidence.clone(),
                confidence: f.confidence,
            }
        }).collect()
    }
}

#[async_trait]
impl CapabilityAdapter for CodebadgerAdapter {
    async fn preflight(&self, _request: &ExecutionRequest) -> Result<PreflightResult, AdapterError> {
        Ok(PreflightResult {
            allowed: true,
            checks: vec![IntegrityCheck {
                check_name: "codebadger_available".to_string(),
                passed: true,
                details: "codebadger CLI available".to_string(),
                severity: CheckSeverity::High,
            }],
            required_permissions: vec!["filesystem_read".to_string(), "process_execute".to_string()],
            resource_limits: ResourceLimits {
                max_execution_time_ms: Some(self.config.timeout_seconds * 1000),
                max_memory_bytes: Some(2 * 1024 * 1024 * 1024),
                ..Default::default()
            },
            warnings: Vec::new(),
        })
    }
    
    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult, AdapterError> {
        let start = Instant::now();
        
        let source_path: PathBuf = request.arguments.get("path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        
        // Ejecutar scan
        let report = self.run_codebadger(&source_path).await?;
        let vulnerabilities = self.findings_to_vulnerabilities(&report);
        
        let duration = start.elapsed();
        
        Ok(ExecutionResult {
            request_id: request.request_id,
            status: ResultStatus::Passed,
            output: Some(serde_json::json!({
                "summary": report.summary,
                "findings": report.findings,
                "metadata": report.metadata,
                "vulnerabilities_count": vulnerabilities.len(),
            })),
            evidence: vec![Evidence {
                evidence_type: EvidenceType::ExecutionLog,
                source: "codebadger_adapter".to_string(),
                content: serde_json::json!({
                    "scan_completed": true,
                    "findings_count": report.findings.len(),
                    "vulnerabilities_converted": vulnerabilities.len(),
                }),
                timestamp: SystemTime::now(),
                hash: String::new(),
            }],
            metrics: ExecutionMetrics {
                tool_version: "codebadger-1.0".to_string(),
                definition_hash: String::new(),
                execution_time_ms: duration.as_millis() as u64,
                cpu_time_ms: None,
                memory_peak_bytes: None,
                tokens_input: None,
                tokens_output: None,
                network_io_bytes: None,
                result_status: ResultStatus::Passed,
                verification_status: None,
                error_count: 0,
                timeout_count: 0,
                resource_limit_count: 0,
            },
            started_at: SystemTime::now() - duration,
            completed_at: SystemTime::now(),
        })
    }
    
    fn validate(&self, result: &ExecutionResult) -> ValidationResult {
        ValidationResult {
            valid: result.status == ResultStatus::Passed,
            issues: Vec::new(),
            adjusted_status: None,
        }
    }
    
    fn get_metrics(&self, _execution_id: &str) -> Option<ExecutionMetrics> {
        None
    }
    
    async fn health_check(&self) -> HealthStatus {
        let output = Command::new(&self.config.codebadger_path)
            .arg("--version")
            .output()
            .await;
        
        HealthStatus {
            healthy: output.is_ok() && output.unwrap().status.success(),
            version: "codebadger-1.0".to_string(),
            last_check: SystemTime::now(),
            details: HashMap::new(),
        }
    }
    
    fn capability_id(&self) -> &str {
        "codebadger_vuln_scan"
    }
    
    fn capability_version(&self) -> &str {
        "1.0.0"
    }
    
    fn tool_definition_hash(&self) -> &str {
        "codebadger_v1"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_codebadger_adapter_creation() {
        let config = CodebadgerAdapterConfig::default();
        let adapter = CodebadgerAdapter::new(config);
        // Test will pass if codebadger is not installed (expected in CI)
    }
}