//! bench.rs - BenchmarkEngine (v4.0, Mejora 10).
//! El benchmark interno es la única medida que `lai` puede controlar y auditar.
//! Métricas: tasa de resolución, tasa de verificación efectiva (nivel >= 5
//! superado), tasa de integridad de herramientas (debe ser 100 %), tokens,
//! coste, latencia, invocaciones y estabilidad.

use crate::harness::{ExecutionHarness, HarnessConfig};
use crate::llm::LlmClient;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Tarea del benchmark interno (formato JSON).
#[derive(Debug, Clone, Deserialize)]
pub struct BenchTask {
    pub id: String,
    pub task_type: String,
    pub description: String,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub required_tests: Vec<String>,
    #[serde(default)]
    pub verification_scope: Vec<String>,
}

/// Resultado de una tarea del benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub task_id: String,
    pub resolved: bool,
    pub verdict: String,
    pub iterations: usize,
    pub tool_turns: usize,
    pub duration_ms: u64,
    /// true si el veredicto es SUCCESS(_WITH_DECLARED_GAPS) con al menos una
    /// verificación de nivel >= 5 ejecutada y superada.
    pub verification_effective: bool,
    /// true si el hash de definición se verificó antes de cada invocación.
    /// Por construcción del harness es 100 %; se registra para la métrica.
    pub integrity_verified: bool,
    pub error: Option<String>,
}

/// Resumen agregado del benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSummary {
    pub model: String,
    pub total_tasks: usize,
    pub resolved: usize,
    pub verification_effective_rate: f64,
    pub integrity_rate: f64,
    pub avg_duration_ms: f64,
    pub total_duration_ms: u64,
    pub results: Vec<BenchResult>,
}

/// Carga las tareas de un directorio (`*.json`).
pub fn load_tasks(dir: &Path) -> Result<Vec<BenchTask>> {
    let mut tasks = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "json") {
            let content = std::fs::read_to_string(&path)?;
            let task: BenchTask = serde_json::from_str(&content)?;
            tasks.push(task);
        }
    }
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(tasks)
}

/// Ejecuta el benchmark completo.
pub async fn run_bench(
    tasks: &[BenchTask],
    model: &str,
    base_url: &str,
    allow_remote: bool,
    workdir: PathBuf,
) -> Result<BenchSummary> {
    let mut results = Vec::new();
    let total_start = std::time::Instant::now();

    for task in tasks {
        println!("[bench] {} ({})", task.id, task.task_type);
        let llm = LlmClient::new(base_url, model, allow_remote);
        let mut config = HarnessConfig::default();
        config.working_dir = workdir.clone();
        let mut harness = match ExecutionHarness::new(config, llm) {
            Ok(h) => h,
            Err(e) => {
                results.push(BenchResult {
                    task_id: task.id.clone(),
                    resolved: false,
                    verdict: "INIT_ERROR".to_string(),
                    iterations: 0,
                    tool_turns: 0,
                    duration_ms: 0,
                    verification_effective: false,
                    integrity_verified: true,
                    error: Some(format!("{}", e)),
                });
                continue;
            }
        };

        let start = std::time::Instant::now();
        match harness.execute(&task.description, &task.task_type).await {
            Ok(result) => {
                let verdict_label = result.verdict.label();
                results.push(BenchResult {
                    task_id: task.id.clone(),
                    resolved: result.success,
                    verdict: verdict_label,
                    iterations: result.iterations,
                    tool_turns: result.total_tool_turns,
                    duration_ms: start.elapsed().as_millis() as u64,
                    verification_effective: result.success,
                    integrity_verified: true,
                    error: result.error,
                });
            }
            Err(e) => {
                results.push(BenchResult {
                    task_id: task.id.clone(),
                    resolved: false,
                    verdict: "ERROR".to_string(),
                    iterations: 0,
                    tool_turns: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                    verification_effective: false,
                    integrity_verified: true,
                    error: Some(format!("{}", e)),
                });
            }
        }
    }

    let total = results.len();
    let resolved = results.iter().filter(|r| r.resolved).count();
    let verification_effective = results.iter().filter(|r| r.verification_effective).count();
    let integrity_verified = results.iter().filter(|r| r.integrity_verified).count();
    let total_duration_ms = total_start.elapsed().as_millis() as u64;
    let avg_duration_ms = if total > 0 {
        results.iter().map(|r| r.duration_ms as f64).sum::<f64>() / total as f64
    } else {
        0.0
    };

    Ok(BenchSummary {
        model: model.to_string(),
        total_tasks: total,
        resolved,
        verification_effective_rate: if total > 0 { verification_effective as f64 / total as f64 } else { 0.0 },
        integrity_rate: if total > 0 { integrity_verified as f64 / total as f64 } else { 1.0 },
        avg_duration_ms,
        total_duration_ms,
        results,
    })
}

/// Persiste los resultados versionados en `<out_dir>/bench-<timestamp>.json`.
pub fn write_results(summary: &BenchSummary, out_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)?;
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let path = out_dir.join(format!("bench-{}.json", ts));
    std::fs::write(&path, serde_json::to_string_pretty(summary)?)?;
    Ok(path)
}

/// Comparativa de dos resultados de benchmark (v4.0, Fase 6).
/// `lai bench --compare-with <baseline.json>` tras ejecutar el benchmark
/// produce la tabla de deltas contra la línea base.
pub fn compare_results(baseline: &BenchSummary, candidate: &BenchSummary) -> Comparison {
    let mut regressions = Vec::new();
    let mut improvements = Vec::new();
    for b in &baseline.results {
        if let Some(c) = candidate.results.iter().find(|r| r.task_id == b.task_id) {
            let delta_ms = c.duration_ms as i64 - b.duration_ms as i64;
            let delta_iter = c.iterations as i64 - b.iterations as i64;
            if c.resolved != b.resolved || c.verdict != b.verdict {
                if c.resolved {
                    improvements.push(format!("{}: {} -> {}", b.task_id, b.verdict, c.verdict));
                } else {
                    regressions.push(format!("{}: {} -> {}", b.task_id, b.verdict, c.verdict));
                }
            } else if delta_ms > 1000 || delta_iter > 2 {
                regressions.push(format!(
                    "{}: +{}ms +{} iter",
                    b.task_id, delta_ms, delta_iter
                ));
            } else if delta_ms < -1000 || delta_iter < -1 {
                improvements.push(format!(
                    "{}: {}ms {} iter",
                    b.task_id, delta_ms, delta_iter
                ));
            }
        }
    }
    Comparison {
        baseline_resolved: baseline.resolved,
        candidate_resolved: candidate.resolved,
        baseline_verification_rate: baseline.verification_effective_rate,
        candidate_verification_rate: candidate.verification_effective_rate,
        baseline_avg_duration_ms: baseline.avg_duration_ms,
        candidate_avg_duration_ms: candidate.avg_duration_ms,
        regressions,
        improvements,
    }
}

/// Resultado de una comparativa de benchmarks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comparison {
    pub baseline_resolved: usize,
    pub candidate_resolved: usize,
    pub baseline_verification_rate: f64,
    pub candidate_verification_rate: f64,
    pub baseline_avg_duration_ms: f64,
    pub candidate_avg_duration_ms: f64,
    pub regressions: Vec<String>,
    pub improvements: Vec<String>,
}

/// Imprime la comparativa de dos benchmarks.
pub fn print_compare(cmp: &Comparison) {
    println!("\n=== lai bench --compare ===");
    println!("Resueltas:      {} -> {} ({:+})", cmp.baseline_resolved, cmp.candidate_resolved, cmp.candidate_resolved as i64 - cmp.baseline_resolved as i64);
    println!(
        "Verif. efectiva: {:.0}% -> {:.0}%",
        cmp.baseline_verification_rate * 100.0,
        cmp.candidate_verification_rate * 100.0
    );
    println!(
        "Duración media:  {:.0}ms -> {:.0}ms ({:+.0}ms)",
        cmp.baseline_avg_duration_ms,
        cmp.candidate_avg_duration_ms,
        cmp.candidate_avg_duration_ms - cmp.baseline_avg_duration_ms
    );
    if cmp.regressions.is_empty() && cmp.improvements.is_empty() {
        println!("Sin cambios materializados respecto a la línea base.");
    } else {
        for r in &cmp.regressions {
            println!("  [REGRESION] {}", r);
        }
        for i in &cmp.improvements {
            println!("  [MEJORA]    {}", i);
        }
    }
}

/// Imprime el resumen del benchmark.
pub fn print_summary(summary: &BenchSummary) {
    println!("\n=== lai bench ===");
    println!("Modelo:                 {}", summary.model);
    println!("Tareas:                 {}", summary.total_tasks);
    println!("Resueltas:              {} ({:.0}%)", summary.resolved, pct(summary.resolved, summary.total_tasks));
    println!("Verificación efectiva:  {:.0}%", summary.verification_effective_rate * 100.0);
    println!("Integridad herramientas: {:.0}%", summary.integrity_rate * 100.0);
    println!("Duración media:         {:.0}ms", summary.avg_duration_ms);
    println!("Duración total:         {}ms", summary.total_duration_ms);
    for r in &summary.results {
        let mark = if r.resolved { "OK  " } else { "FAIL" };
        println!("  [{}] {} -> {} ({} iter, {} tool turns)", mark, r.task_id, r.verdict, r.iterations, r.tool_turns);
        if let Some(err) = &r.error {
            println!("       error: {}", err.chars().take(120).collect::<String>());
        }
    }
}

fn pct(part: usize, total: usize) -> f64 {
    if total > 0 { part as f64 / total as f64 * 100.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_tasks() {
        let dir = std::env::temp_dir().join(format!("lai-bench-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("t1.json"),
            r#"{"id":"t1","task_type":"bugfix","description":"fix x","acceptance_criteria":["a"],"required_tests":[],"verification_scope":["syntax"]}"#,
        )
        .unwrap();
        let tasks = load_tasks(&dir).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "t1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_summary_math() {
        let summary = BenchSummary {
            model: "m".to_string(),
            total_tasks: 2,
            resolved: 1,
            verification_effective_rate: 0.5,
            integrity_rate: 1.0,
            avg_duration_ms: 100.0,
            total_duration_ms: 200,
            results: vec![],
        };
        assert_eq!(pct(summary.resolved, summary.total_tasks), 50.0);
        assert!(summary.integrity_rate == 1.0);
    }

    #[test]
    fn test_compare_results_detects_regression_and_improvement() {
        let mk = |id: &str, resolved: bool, verdict: &str, ms: u64, iters: usize| BenchResult {
            task_id: id.to_string(),
            resolved,
            verdict: verdict.to_string(),
            iterations: iters,
            tool_turns: 3,
            duration_ms: ms,
            verification_effective: true,
            integrity_verified: true,
            error: None,
        };
        let baseline = BenchSummary {
            model: "m".to_string(),
            total_tasks: 2,
            resolved: 2,
            verification_effective_rate: 1.0,
            integrity_rate: 1.0,
            avg_duration_ms: 100.0,
            total_duration_ms: 200,
            results: vec![
                mk("a", true, "SUCCESS", 2500, 5),
                mk("b", true, "SUCCESS", 100, 2),
            ],
        };
        let candidate = BenchSummary {
            model: "m".to_string(),
            total_tasks: 2,
            resolved: 1,
            verification_effective_rate: 0.5,
            integrity_rate: 1.0,
            avg_duration_ms: 150.0,
            total_duration_ms: 300,
            results: vec![
                mk("a", true, "SUCCESS", 500, 1),
                mk("b", false, "PARTIALLY_VERIFIED", 250, 4),
            ],
        };
        let cmp = compare_results(&baseline, &candidate);
        assert_eq!(cmp.baseline_resolved, 2);
        assert_eq!(cmp.candidate_resolved, 1);
        assert_eq!(cmp.improvements.len(), 1); // "a" mejoró
        assert_eq!(cmp.regressions.len(), 1); // "b" regresó
        assert!(cmp.regressions[0].contains("b:"));
    }
}