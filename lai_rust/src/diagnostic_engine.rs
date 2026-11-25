//! DiagnosticEngine - Motor de diagnóstico y localización de fallos.
//! Implementa análisis de fallos, localización de causas raíz y clasificación.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Categoría de fallo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureCategory {
    SyntaxError,
    TypeError,
    RuntimeError,
    LogicError,
    PerformanceIssue,
    SecurityVulnerability,
    ResourceLeak,
    ConcurrencyIssue,
    ConfigurationError,
    DependencyError,
    Unknown,
}

impl FailureCategory {
    pub fn name(&self) -> &'static str {
        match self {
            FailureCategory::SyntaxError => "SyntaxError",
            FailureCategory::TypeError => "TypeError",
            FailureCategory::RuntimeError => "RuntimeError",
            FailureCategory::LogicError => "LogicError",
            FailureCategory::PerformanceIssue => "PerformanceIssue",
            FailureCategory::SecurityVulnerability => "SecurityVulnerability",
            FailureCategory::ResourceLeak => "ResourceLeak",
            FailureCategory::ConcurrencyIssue => "ConcurrencyIssue",
            FailureCategory::ConfigurationError => "ConfigurationError",
            FailureCategory::DependencyError => "DependencyError",
            FailureCategory::Unknown => "Unknown",
        }
    }
}

/// Severidad del fallo
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

/// Ubicación del fallo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureLocation {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub function: Option<String>,
    pub context: Option<String>,
}

/// Diagnóstico de un fallo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnosis {
    pub id: String,
    pub category: FailureCategory,
    pub severity: Severity,
    pub message: String,
    pub location: Option<FailureLocation>,
    pub root_cause: Option<String>,
    pub suggested_fix: Option<String>,
    pub confidence: f64,
    pub related_symbols: Vec<String>,
    pub stack_trace: Option<String>,
}

/// Configuración del motor de diagnóstico
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticConfig {
    pub max_depth: usize,
    pub include_stack_trace: bool,
    pub analyze_dependencies: bool,
    pub semantic_analysis: bool,
    pub max_candidates: usize,
    pub confidence_threshold: f64,
}

impl Default for DiagnosticConfig {
    fn default() -> Self {
        Self {
            max_depth: 5,
            include_stack_trace: true,
            analyze_dependencies: true,
            semantic_analysis: true,
            max_candidates: 10,
            confidence_threshold: 0.5,
        }
    }
}

/// Motor de diagnóstico
pub struct DiagnosticEngine {
    config: DiagnosticConfig,
    workspace_root: PathBuf,
    knowledge_base: Arc<RwLock<DiagnosticKnowledgeBase>>,
}

/// Base de conocimiento para diagnóstico
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiagnosticKnowledgeBase {
    pub patterns: HashMap<String, DiagnosticPattern>,
    pub rules: Vec<DiagnosticRule>,
    pub statistics: DiagnosticStatistics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticPattern {
    pub pattern: String,
    pub category: FailureCategory,
    pub severity: Severity,
    pub description: String,
    pub suggested_fix: String,
    pub confidence: f64,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticRule {
    pub name: String,
    pub condition: String, // Expresión o patrón
    pub category: FailureCategory,
    pub severity: Severity,
    pub action: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiagnosticStatistics {
    pub total_diagnoses: usize,
    pub by_category: HashMap<String, usize>,
    pub by_severity: HashMap<String, usize>,
    pub avg_confidence: f64,
}

/// Resultado de diagnóstico
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticResult {
    pub diagnoses: Vec<Diagnosis>,
    pub root_cause: Option<Diagnosis>,
    pub execution_time_ms: u64,
    pub files_analyzed: usize,
    pub symbols_analyzed: usize,
}

impl DiagnosticEngine {
    pub fn new(workspace_root: PathBuf, config: DiagnosticConfig) -> Result<Self> {
        Ok(Self {
            config,
            workspace_root,
            knowledge_base: Arc::new(RwLock::new(DiagnosticKnowledgeBase::default())),
        })
    }

    /// Diagnosticar un error dado su output y contexto
    pub async fn diagnose(
        &self,
        error_output: &str,
        context: DiagnosticContext,
    ) -> Result<DiagnosticResult> {
        let start = Instant::now();
        let mut diagnoses = Vec::new();

        // 1. Análisis sintáctico del error
        let syntax_diagnoses = self.analyze_syntax_error(error_output, &context).await?;
        diagnoses.extend(syntax_diagnoses);

        // 2. Análisis semántico si está habilitado
        if self.config.semantic_analysis {
            let semantic_diagnoses = self.analyze_semantic_error(error_output, &context).await?;
            diagnoses.extend(semantic_diagnoses);
        }

        // 3. Análisis de dependencias
        if self.config.analyze_dependencies {
            let dep_diagnoses = self
                .analyze_dependency_errors(error_output, &context)
                .await?;
            diagnoses.extend(dep_diagnoses);
        }

        // 4. Análisis de patrones conocidos
        let pattern_diagnoses = self.match_known_patterns(error_output).await?;
        diagnoses.extend(pattern_diagnoses);

        // 5. Filtrar y ordenar por confianza
        diagnoses.retain(|d| d.confidence >= self.config.confidence_threshold);
        diagnoses.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        diagnoses.truncate(self.config.max_candidates);

        // Identificar causa raíz
        let root_cause = diagnoses.first().cloned();

        Ok(DiagnosticResult {
            diagnoses,
            root_cause,
            execution_time_ms: start.elapsed().as_millis() as u64,
            files_analyzed: context.related_files.len(),
            symbols_analyzed: context.related_symbols.len(),
        })
    }

    /// Analizar error de sintaxis
    async fn analyze_syntax_error(
        &self,
        error: &str,
        context: &DiagnosticContext,
    ) -> Result<Vec<Diagnosis>> {
        let mut diagnoses = Vec::new();

        // Patrones comunes de errores de sintaxis
        let patterns = [
            (
                r"SyntaxError|syntax error",
                FailureCategory::SyntaxError,
                Severity::High,
            ),
            (
                r"IndentationError",
                FailureCategory::SyntaxError,
                Severity::High,
            ),
            (
                r"unexpected EOF",
                FailureCategory::SyntaxError,
                Severity::High,
            ),
            (
                r"invalid syntax",
                FailureCategory::SyntaxError,
                Severity::High,
            ),
            (
                r"unexpected token",
                FailureCategory::SyntaxError,
                Severity::High,
            ),
        ];

        for (pattern, category, severity) in patterns {
            if regex::Regex::new(pattern).unwrap().is_match(error) {
                diagnoses.push(Diagnosis {
                    id: uuid::Uuid::new_v4().to_string(),
                    category,
                    severity,
                    message: format!("Syntax error detected: {}", pattern),
                    location: self.extract_location(error, context),
                    root_cause: Some("Invalid syntax in source code".to_string()),
                    suggested_fix: Some("Check syntax around the reported line".to_string()),
                    confidence: 0.9,
                    related_symbols: Vec::new(),
                    stack_trace: None,
                });
            }
        }

        Ok(diagnoses)
    }

    /// Analizar error semántico
    async fn analyze_semantic_error(
        &self,
        error: &str,
        _context: &DiagnosticContext,
    ) -> Result<Vec<Diagnosis>> {
        let mut diagnoses = Vec::new();

        let patterns = [
            (
                r"TypeError|type error",
                FailureCategory::TypeError,
                Severity::High,
            ),
            (
                r"NameError|name '.*' is not defined",
                FailureCategory::TypeError,
                Severity::High,
            ),
            (
                r"AttributeError|'.*' object has no attribute",
                FailureCategory::TypeError,
                Severity::High,
            ),
            (
                r"KeyError|key.*not found",
                FailureCategory::RuntimeError,
                Severity::Medium,
            ),
            (
                r"IndexError|index.*out of range",
                FailureCategory::RuntimeError,
                Severity::Medium,
            ),
            (
                r"ValueError|value error",
                FailureCategory::RuntimeError,
                Severity::Medium,
            ),
            (
                r"ZeroDivisionError",
                FailureCategory::RuntimeError,
                Severity::Medium,
            ),
            (
                r"ImportError|ModuleNotFoundError",
                FailureCategory::DependencyError,
                Severity::High,
            ),
            (
                r"FileNotFoundError",
                FailureCategory::RuntimeError,
                Severity::Medium,
            ),
            (
                r"PermissionError",
                FailureCategory::RuntimeError,
                Severity::Medium,
            ),
            (
                r"MemoryError|OutOfMemory",
                FailureCategory::ResourceLeak,
                Severity::Critical,
            ),
            (
                r"TimeoutError|timeout",
                FailureCategory::PerformanceIssue,
                Severity::High,
            ),
            (
                r"RecursionError|maximum recursion depth",
                FailureCategory::LogicError,
                Severity::High,
            ),
            (
                r"deadlock|Deadlock",
                FailureCategory::ConcurrencyIssue,
                Severity::Critical,
            ),
            (
                r"race condition|RaceCondition",
                FailureCategory::ConcurrencyIssue,
                Severity::Critical,
            ),
            (
                r"SQL injection|XSS|CSRF",
                FailureCategory::SecurityVulnerability,
                Severity::Critical,
            ),
        ];

        for (pattern, category, severity) in patterns {
            if regex::Regex::new(pattern).unwrap().is_match(error) {
                let location = self.extract_location(error, &DiagnosticContext::default());
                diagnoses.push(Diagnosis {
                    id: uuid::Uuid::new_v4().to_string(),
                    category,
                    severity,
                    message: format!("Error detected: {}", pattern),
                    location,
                    root_cause: Some(format!("Pattern matched: {}", pattern)),
                    suggested_fix: self.suggest_fix_for_category(category),
                    confidence: 0.8,
                    related_symbols: Vec::new(),
                    stack_trace: self.extract_stack_trace(error),
                });
            }
        }

        Ok(diagnoses)
    }

    /// Analizar errores de dependencias
    async fn analyze_dependency_errors(
        &self,
        error: &str,
        _context: &DiagnosticContext,
    ) -> Result<Vec<Diagnosis>> {
        let mut diagnoses = Vec::new();

        if error.contains("ImportError")
            || error.contains("ModuleNotFoundError")
            || error.contains("Cargo.toml")
            || error.contains("package.json")
        {
            diagnoses.push(Diagnosis {
                id: uuid::Uuid::new_v4().to_string(),
                category: FailureCategory::DependencyError,
                severity: Severity::High,
                message: "Dependency resolution error".to_string(),
                location: self.extract_location(error, &DiagnosticContext::default()),
                root_cause: Some("Missing or incompatible dependency".to_string()),
                suggested_fix: Some("Check dependency versions and installation".to_string()),
                confidence: 0.85,
                related_symbols: Vec::new(),
                stack_trace: self.extract_stack_trace(error),
            });
        }

        Ok(diagnoses)
    }

    /// Match known error patterns
    async fn match_known_patterns(&self, error: &str) -> Result<Vec<Diagnosis>> {
        let kb = self.knowledge_base.read().unwrap();
        let mut diagnoses = Vec::new();

        for pattern in kb.patterns.values() {
            if regex::Regex::new(&pattern.pattern).unwrap().is_match(error) {
                diagnoses.push(Diagnosis {
                    id: uuid::Uuid::new_v4().to_string(),
                    category: pattern.category,
                    severity: pattern.severity,
                    message: pattern.description.clone(),
                    location: None,
                    root_cause: Some(pattern.description.clone()),
                    suggested_fix: Some(pattern.suggested_fix.clone()),
                    confidence: pattern.confidence,
                    related_symbols: Vec::new(),
                    stack_trace: None,
                });
            }
        }

        Ok(diagnoses)
    }

    /// Extraer ubicación del error
    fn extract_location(
        &self,
        error: &str,
        context: &DiagnosticContext,
    ) -> Option<FailureLocation> {
        // Intentar extraer archivo y línea del error
        let re_file_line = regex::Regex::new(r#"File "([^"]+)", line (\d+)"#).unwrap();
        if let Some(caps) = re_file_line.captures(error) {
            return Some(FailureLocation {
                file: PathBuf::from(caps.get(1)?.as_str()),
                line: caps.get(2)?.as_str().parse().unwrap_or(0),
                column: 0,
                function: None,
                context: None,
            });
        }

        // Intentar con contexto
        if let Some(file) = context.related_files.first() {
            return Some(FailureLocation {
                file: PathBuf::from(file),
                line: 0,
                column: 0,
                function: None,
                context: None,
            });
        }

        None
    }

    /// Extraer stack trace
    fn extract_stack_trace(&self, error: &str) -> Option<String> {
        if error.contains("Traceback") || error.contains("stack trace") {
            Some(error.to_string())
        } else {
            None
        }
    }

    /// Sugerir fix basado en categoría
    fn suggest_fix_for_category(&self, category: FailureCategory) -> Option<String> {
        match category {
            FailureCategory::SyntaxError => Some("Check syntax around reported line; check for missing brackets, colons, or indentation".to_string()),
            FailureCategory::TypeError => Some("Check variable types and ensure correct types are used".to_string()),
            FailureCategory::RuntimeError => Some("Check runtime conditions and add proper error handling".to_string()),
            FailureCategory::LogicError => Some("Review algorithm logic and add assertions".to_string()),
            FailureCategory::PerformanceIssue => Some("Profile code and optimize bottlenecks".to_string()),
            FailureCategory::SecurityVulnerability => Some("Apply security patches and validate inputs".to_string()),
            FailureCategory::ResourceLeak => Some("Ensure proper resource cleanup with RAII/try-finally".to_string()),
            FailureCategory::ConcurrencyIssue => Some("Add proper synchronization and avoid shared mutable state".to_string()),
            FailureCategory::ConfigurationError => Some("Verify configuration files and environment variables".to_string()),
            FailureCategory::DependencyError => Some("Update or pin dependency versions".to_string()),
            FailureCategory::Unknown => Some("Review error message and context for clues".to_string()),
        }
    }
}

/// Standalone function for diagnosing errors from commands
pub async fn diagnose_error(
    error_message: &str,
    working_dir: &Path,
    context: Option<&str>,
) -> Diagnosis {
    let config = DiagnosticConfig::default();
    let engine = DiagnosticEngine::new(working_dir.to_path_buf(), config).unwrap();

    let mut diag_context = DiagnosticContext::default();
    if let Some(ctx) = context {
        diag_context.related_files.push(ctx.to_string());
    }

    let result = engine.diagnose(error_message, diag_context).await.unwrap();

    // Return the first (highest confidence) diagnosis
    result.diagnoses.into_iter().next().unwrap_or(Diagnosis {
        id: uuid::Uuid::new_v4().to_string(),
        category: FailureCategory::Unknown,
        severity: Severity::Info,
        message: error_message.to_string(),
        location: None,
        root_cause: None,
        suggested_fix: None,
        confidence: 0.0,
        related_symbols: Vec::new(),
        stack_trace: None,
    })
}

/// Contexto para diagnóstico
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiagnosticContext {
    pub related_files: Vec<String>,
    pub related_symbols: Vec<String>,
    pub recent_changes: Vec<String>,
    pub test_results: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_diagnostic_engine_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config = DiagnosticConfig::default();
        let engine = DiagnosticEngine::new(temp_dir.path().to_path_buf(), config).unwrap();

        let error = "NameError: name 'undefined_var' is not defined";
        let result = engine
            .diagnose(error, DiagnosticContext::default())
            .await
            .unwrap();

        assert!(!result.diagnoses.is_empty());
        assert_eq!(result.diagnoses[0].category, FailureCategory::TypeError);
    }
}
