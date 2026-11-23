//! testing/mutation.rs - Mutation testing integration
//! 
//! Integra mutest (mutation testing para Rust) y detecta si las pruebas
//! detectan mutaciones deliberadamente introducidas.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Configuración para mutation testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationTestConfig {
    pub target_dir: PathBuf,
    pub test_command: String,
    pub timeout_seconds: u64,
    pub max_mutants: usize,
    pub excluded_patterns: Vec<String>,
    pub mutators: Vec<Mutator>,
}

impl Default for MutationTestConfig {
    fn default() -> Self {
        Self {
            target_dir: PathBuf::from("."),
            test_command: "cargo test".to_string(),
            timeout_seconds: 300,
            max_mutants: 1000,
            excluded_patterns: vec![
                "target/".to_string(),
                ".git/".to_string(),
                "tests/".to_string(),
            ],
            mutators: vec![
                Mutator::Arithmetic,
                Mutator::Comparison,
                Mutator::Logical,
                Mutator::Literal,
                Mutator::Negation,
            ],
        }
    }
}

/// Tipos de mutadores disponibles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mutator {
    /// Reemplaza + con -, * con /, etc.
    Arithmetic,
    /// Reemplaza < con >, == con !=, etc.
    Comparison,
    /// Reemplaza && con ||, ! con vacío
    Logical,
    /// Reemplaza literales (true/false, 0/1, ""/"mutated")
    Literal,
    /// Nega expresiones booleanas
    Negation,
    /// Remueve llamadas a funciones
    RemoveCall,
    /// Cambia return por panic
    ReturnPanic,
}

/// Resultado de mutation testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationTestResult {
    pub total_mutants: usize,
    pub killed_mutants: usize,
    pub survived_mutants: usize,
    pub timeout_mutants: usize,
    pub error_mutants: usize,
    pub mutation_score: f64, // killed / total
    pub mutants: Vec<MutantResult>,
    pub duration_ms: u64,
}

/// Resultado individual de un mutante
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutantResult {
    pub id: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub mutator: Mutator,
    pub original_code: String,
    pub mutated_code: String,
    pub status: MutantStatus,
    pub test_output: Option<String>,
    pub execution_time_ms: u64,
}

/// Estado de un mutante
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutantStatus {
    Killed,      // Tests detectaron la mutación (fallaron)
    Survived,    // Tests NO detectaron la mutación (pasaron)
    Timeout,     // Test timeout
    Error,       // Error de compilación/ejecución
    Skipped,     // Mutante excluido
}

/// Motor de mutation testing
pub struct MutationTestEngine {
    config: MutationTestConfig,
}

impl MutationTestEngine {
    pub fn new(config: MutationTestConfig) -> Self {
        Self { config }
    }

    /// Ejecuta mutation testing completo
    pub fn run(&self) -> Result<MutationTestResult, MutationTestError> {
        let start = Instant::now();
        
        // 1. Descubrir mutantes
        let mutants = self.discover_mutants()?;
        
        // 2. Filtrar por límite
        let mutants: Vec<_> = mutants.into_iter().take(self.config.max_mutants).collect();
        
        // 3. Ejecutar tests para cada mutante
        let mut results = Vec::new();
        let mut killed = 0;
        let mut survived = 0;
        let mut timeout = 0;
        let mut error = 0;
        
        for (i, mutant) in mutants.iter().enumerate() {
            println!("Testing mutant {}/{}: {} at {}:{}", 
                i + 1, mutants.len(), mutant.id, mutant.file, mutant.line);
            
            let result = self.test_mutant(mutant)?;
            
            match result.status {
                MutantStatus::Killed => killed += 1,
                MutantStatus::Survived => survived += 1,
                MutantStatus::Timeout => timeout += 1,
                MutantStatus::Error => error += 1,
                MutantStatus::Skipped => {}
            }
            
            results.push(result);
        }
        
        let total = killed + survived + timeout + error;
        let mutation_score = if total > 0 { killed as f64 / total as f64 } else { 0.0 };
        
        Ok(MutationTestResult {
            total_mutants: total,
            killed_mutants: killed,
            survived_mutants: survived,
            timeout_mutants: timeout,
            error_mutants: error,
            mutation_score,
            mutants: results,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Descubre mutantes aplicando mutadores al código fuente
    fn discover_mutants(&self) -> Result<Vec<MutantResult>, MutationTestError> {
        let mut mutants = Vec::new();
        let mut id_counter = 0;
        
        // Buscar archivos Rust en el directorio target
        for entry in walkdir::WalkDir::new(&self.config.target_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
            .filter(|e| !self.is_excluded(e.path()))
        {
            let path = entry.path();
            let content = std::fs::read_to_string(path)?;
            
            for mutator in &self.config.mutators {
                let mut_results = self.apply_mutator(path, &content, *mutator, &mut id_counter)?;
                mutants.extend(mut_results);
            }
        }
        
        Ok(mutants)
    }

    fn is_excluded(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        self.config.excluded_patterns.iter().any(|p| path_str.contains(p))
    }

    /// Aplica un mutador específico al código
    fn apply_mutator(
        &self,
        file: &Path,
        content: &str,
        mutator: Mutator,
        id_counter: &mut usize,
    ) -> Result<Vec<MutantResult>, MutationTestError> {
        let mut results = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        for (line_idx, line) in lines.iter().enumerate() {
            let mutations = self.generate_mutations(line, mutator);
            
            for (original, mutated, column) in mutations {
                if original == mutated {
                    continue;
                }
                
                *id_counter += 1;
                results.push(MutantResult {
                    id: format!("M{:04}", id_counter),
                    file: file.to_string_lossy().to_string(),
                    line: line_idx + 1,
                    column,
                    mutator,
                    original_code: original.to_string(),
                    mutated_code: mutated.to_string(),
                    status: MutantStatus::Skipped,
                    test_output: None,
                    execution_time_ms: 0,
                });
            }
        }
        
        Ok(results)
    }

    /// Genera mutaciones para una línea según el mutador
    fn generate_mutations(&self, line: &str, mutator: Mutator) -> Vec<(&str, String, usize)> {
        let mut results = Vec::new();
        
        match mutator {
            Mutator::Arithmetic => {
                // + -> -, - -> +, * -> /, / -> *, % -> *
                for (op, replacement) in [
                    ("+", "-"), ("-", "+"), ("*", "/"), ("/", "*"), ("%", "*"),
                    ("+=", "-="), ("-=", "+="), ("*=", "/="), ("/=", "*="),
                ] {
                    if let Some(pos) = line.find(op) {
                        let mut mutated = line.to_string();
                        mutated.replace_range(pos..pos+op.len(), replacement);
                        results.push((line, mutated, pos));
                    }
                }
            }
            Mutator::Comparison => {
                // < -> >, > -> <, <= -> >=, >= -> <=, == -> !=, != -> ==
                for (op, replacement) in [
                    ("<", ">"), (">", "<"), ("<=", ">="), (">=", "<="),
                    ("==", "!="), ("!=", "=="),
                ] {
                    if let Some(pos) = line.find(op) {
                        let mut mutated = line.to_string();
                        mutated.replace_range(pos..pos+op.len(), replacement);
                        results.push((line, mutated, pos));
                    }
                }
            }
            Mutator::Logical => {
                // && -> ||, || -> &&, ! -> (remove)
                if let Some(pos) = line.find("&&") {
                    let mut mutated = line.to_string();
                    mutated.replace_range(pos..pos+2, "||");
                    results.push((line, mutated, pos));
                }
                if let Some(pos) = line.find("||") {
                    let mut mutated = line.to_string();
                    mutated.replace_range(pos..pos+2, "&&");
                    results.push((line, mutated, pos));
                }
                // !expression -> expression (simplified)
                if let Some(pos) = line.find('!') {
                    if pos + 1 < line.len() && !line.chars().nth(pos + 1).unwrap_or(' ').is_whitespace() {
                        let mut mutated = line.to_string();
                        mutated.remove(pos);
                        results.push((line, mutated, pos));
                    }
                }
            }
            Mutator::Literal => {
                // true -> false, false -> true, 0 -> 1, 1 -> 0, "" -> "mutated"
                if let Some(pos) = line.find("true") {
                    let mut mutated = line.to_string();
                    mutated.replace_range(pos..pos+4, "false");
                    results.push((line, mutated, pos));
                }
                if let Some(pos) = line.find("false") {
                    let mut mutated = line.to_string();
                    mutated.replace_range(pos..pos+5, "true");
                    results.push((line, mutated, pos));
                }
                if let Some(pos) = line.find("0") {
                    // Avoid replacing in larger numbers
                    let before = line.get(pos.saturating_sub(1)..pos).unwrap_or("");
                    let after = line.get(pos+1..pos+2).unwrap_or("");
                    if !before.chars().next().unwrap_or(' ').is_ascii_digit() && 
                       !after.chars().next().unwrap_or(' ').is_ascii_digit() {
                        let mut mutated = line.to_string();
                        mutated.replace_range(pos..pos+1, "1");
                        results.push((line, mutated, pos));
                    }
                }
            }
            Mutator::Negation => {
                // !x -> x, x -> !x (for boolean expressions)
                if let Some(pos) = line.find("!") {
                    if pos + 1 < line.len() && line.chars().nth(pos + 1).unwrap_or(' ').is_alphabetic() {
                        let mut mutated = line.to_string();
                        mutated.remove(pos);
                        results.push((line, mutated, pos));
                    }
                }
            }
            _ => {} // Other mutators not implemented in this simplified version
        }
        
        results
    }

    /// Ejecuta tests con un mutante aplicado
    fn test_mutant(&self, mutant: &MutantResult) -> Result<MutantResult, MutationTestError> {
        let start = Instant::now();
        
        // 1. Backup original file
        let original_content = std::fs::read_to_string(&mutant.file)?;
        
        // 2. Apply mutation
        let mutated_content = original_content.replace(&mutant.original_code, &mutant.mutated_code);
        std::fs::write(&mutant.file, &mutated_content)?;
        
        // 3. Run tests
        let mut result = mutant.clone();
        result.execution_time_ms = start.elapsed().as_millis() as u64;
        
        let test_result = Command::new("sh")
            .arg("-c")
            .arg(&self.config.test_command)
            .current_dir(&self.config.target_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .output();
        
        // 4. Restore original file
        std::fs::write(&mutant.file, &original_content)?;
        
        match test_result {
            Ok(output) => {
                if output.status.success() {
                    result.status = MutantStatus::Survived;
                } else {
                    result.status = MutantStatus::Killed;
                    result.test_output = Some(String::from_utf8_lossy(&output.stderr).to_string());
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::TimedOut {
                    result.status = MutantStatus::Timeout;
                } else {
                    result.status = MutantStatus::Error;
                    result.test_output = Some(e.to_string());
                }
            }
        }
        
        Ok(result)
    }
}

/// Errores de mutation testing
#[derive(Debug, thiserror::Error)]
pub enum MutationTestError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Walkdir error: {0}")]
    Walkdir(#[from] walkdir::Error),
    #[error("Mutation error: {0}")]
    Mutation(String),
}

/// Report generator para mutation testing
pub struct MutationReportGenerator;

impl MutationReportGenerator {
    pub fn generate_html(result: &MutationTestResult) -> String {
        let mut html = String::new();
        html.push_str(r#"
<!DOCTYPE html>
<html>
<head>
    <title>Mutation Testing Report</title>
    <style>
        body { font-family: sans-serif; margin: 20px; }
        .summary { background: #f5f5f5; padding: 20px; border-radius: 5px; }
        .score { font-size: 2em; font-weight: bold; }
        .killed { color: green; }
        .survived { color: red; }
        table { border-collapse: collapse; width: 100%; margin-top: 20px; }
        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
        th { background: #4CAF50; color: white; }
        .mutant-killed { background: #e8f5e9; }
        .mutant-survived { background: #ffebee; }
        .mutant-timeout { background: #fff3e0; }
        .mutant-error { background: #fafafa; }
    </style>
</head>
<body>
        "#);
        
        html.push_str(&format!(r#"
    <div class="summary">
        <h1>Mutation Testing Report</h1>
        <div class="score">Mutation Score: {:.1}%</div>
        <p>Total: {} | Killed: <span class="killed">{}</span> | Survived: <span class="survived">{}</span> | Timeout: {} | Error: {}</p>
    </div>
        "#, 
            result.mutation_score * 100.0,
            result.total_mutants,
            result.killed_mutants,
            result.survived_mutants,
            result.timeout_mutants,
            result.error_mutants
        ));
        
        html.push_str(r#"
    <table>
        <tr>
            <th>ID</th>
            <th>File</th>
            <th>Line</th>
            <th>Mutator</th>
            <th>Original</th>
            <th>Mutated</th>
            <th>Status</th>
        </tr>
        "#);
        
        for mutant in &result.mutants {
            let row_class = match mutant.status {
                MutantStatus::Killed => "mutant-killed",
                MutantStatus::Survived => "mutant-survived",
                MutantStatus::Timeout => "mutant-timeout",
                _ => "mutant-error",
            };
            
            html.push_str(&format!(r#"
        <tr class="{}">
            <td>{}</td>
            <td>{}</td>
            <td>{}</td>
            <td>{:?}</td>
            <td><code>{}</code></td>
            <td><code>{}</code></td>
            <td>{:?}</td>
        </tr>
            "#, 
                row_class,
                mutant.id,
                mutant.file,
                mutant.line,
                mutant.mutator,
                html_escape(&mutant.original_code),
                html_escape(&mutant.mutated_code),
                mutant.status
            ));
        }
        
        html.push_str("</table></body></html>");
        html
    }
    
    pub fn generate_json(result: &MutationTestResult) -> String {
        serde_json::to_string_pretty(result).unwrap_or_default()
    }
    
    pub fn generate_markdown(result: &MutationTestResult) -> String {
        let mut md = String::new();
        md.push_str("# Mutation Testing Report\n\n");
        md.push_str(&format!("**Mutation Score:** {:.1}%\n\n", result.mutation_score * 100.0));
        md.push_str(&format!("| Metric | Count |\n|--------|-------|\n"));
        md.push_str(&format!("| Total | {} |\n", result.total_mutants));
        md.push_str(&format!("| Killed | {} |\n", result.killed_mutants));
        md.push_str(&format!("| Survived | {} |\n", result.survived_mutants));
        md.push_str(&format!("| Timeout | {} |\n", result.timeout_mutants));
        md.push_str(&format!("| Error | {} |\n\n", result.error_mutants));
        
        md.push_str("## Mutants\n\n");
        md.push_str("| ID | File | Line | Mutator | Original | Mutated | Status |\n");
        md.push_str("|----|------|------|---------|----------|---------|--------|\n");
        
        for mutant in &result.mutants {
            let status_emoji = match mutant.status {
                MutantStatus::Killed => "✅",
                MutantStatus::Survived => "❌",
                MutantStatus::Timeout => "⏱️",
                _ => "⚠️",
            };
            md.push_str(&format!("| {} | {} | {} | {:?} | `{}` | `{}` | {} |\n",
                mutant.id,
                mutant.file,
                mutant.line,
                mutant.mutator,
                mutant.original_code,
                mutant.mutated_code,
                status_emoji
            ));
        }
        
        md
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&")
     .replace('<', "<")
     .replace('>', ">")
     .replace('"', """)
     .replace('\'', "'")
}

/// Mutaciones objetivo prioritarias para testing
pub const PRIORITY_MUTATIONS: &[(&str, &str)] = &[
    ("<=", "<"),     // Off-by-one en límites
    (">=", ">"),     // Off-by-one en límites
    ("<", "<="),     // Inversión de límite
    (">", ">="),     // Inversión de límite
    ("==", "!="),    // Inversión de igualdad
    ("!=", "=="),    // Inversión de desigualdad
    ("&&", "||"),    // AND -> OR
    ("||", "&&"),    // OR -> AND
    ("!", ""),       // Remoción de negación
    ("+", "-"),      // Suma -> Resta
    ("-", "+"),      // Resta -> Suma
    ("*", "/"),      // Multiplicación -> División
    ("/", "*"),      // División -> Multiplicación
    ("true", "false"), // Boolean literals
    ("false", "true"), // Boolean literals
    ("0", "1"),      // Zero -> One
    ("1", "0"),      // One -> Zero
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_mutation_engine_creation() {
        let config = MutationTestConfig::default();
        let engine = MutationTestEngine::new(config);
        // Just verify it can be created
    }

    #[test]
    fn test_generate_arithmetic_mutations() {
        let config = MutationTestConfig::default();
        let engine = MutationTestEngine::new(config);
        
        let line = "let x = a + b * c - d / e;";
        let mutations = engine.generate_mutations(line, Mutator::Arithmetic);
        
        assert!(!mutations.is_empty());
        // Should find +, *, -, / operators
        let mutated_codes: Vec<_> = mutations.iter().map(|(_, m, _)| m).collect();
        assert!(mutated_codes.iter().any(|m| m.contains("-")));
        assert!(mutated_codes.iter().any(|m| m.contains("/")));
    }

    #[test]
    fn test_generate_comparison_mutations() {
        let config = MutationTestConfig::default();
        let engine = MutationTestEngine::new(config);
        
        let line = "if x < y && a >= b { }";
        let mutations = engine.generate_mutations(line, Mutator::Comparison);
        
        assert!(!mutations.is_empty());
    }

    #[test]
    fn test_generate_logical_mutations() {
        let config = MutationTestConfig::default();
        let engine = MutationTestEngine::new(config);
        
        let line = "if a && b || !c { }";
        let mutations = engine.generate_mutations(line, Mutator::Logical);
        
        assert!(!mutations.is_empty());
    }

    #[test]
    fn test_mutation_report_html() {
        let result = MutationTestResult {
            total_mutants: 10,
            killed_mutants: 8,
            survived_mutants: 2,
            timeout_mutants: 0,
            error_mutants: 0,
            mutation_score: 0.8,
            mutants: vec![
                MutantResult {
                    id: "M0001".to_string(),
                    file: "src/main.rs".to_string(),
                    line: 10,
                    column: 5,
                    mutator: Mutator::Comparison,
                    original_code: "x < y".to_string(),
                    mutated_code: "x > y".to_string(),
                    status: MutantStatus::Killed,
                    test_output: None,
                    execution_time_ms: 100,
                },
            ],
            duration_ms: 5000,
        };
        
        let html = MutationReportGenerator::generate_html(&result);
        assert!(html.contains("Mutation Score: 80.0%"));
        assert!(html.contains("M0001"));
        assert!(html.contains("Killed"));
    }

    #[test]
    fn test_mutation_report_markdown() {
        let result = MutationTestResult {
            total_mutants: 10,
            killed_mutants: 8,
            survived_mutants: 2,
            timeout_mutants: 0,
            error_mutants: 0,
            mutation_score: 0.8,
            mutants: vec![],
            duration_ms: 5000,
        };
        
        let md = MutationReportGenerator::generate_markdown(&result);
        assert!(md.contains("Mutation Score: 80.0%"));
        assert!(md.contains("Total | 10"));
        assert!(md.contains("Killed | 8"));
    }
}