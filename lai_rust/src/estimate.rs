//! estimate.rs — Estimación de capacidad SWE-bench (Regla 12).
//!
//! Lee un `report.json` producido por `swebench/evaluate.py` y calcula:
//!
//! - tasa extremo a extremo (resueltas / intentadas) y sobre evaluables;
//! - intervalo de Wilson al 95 % para ambas;
//! - cota superior por regla de tres cuando no hay ninguna resuelta;
//! - desglose por repositorio y errores de infraestructura;
//! - tamaño de muestra necesario para un margen de error.
//!
//! La implementación canónica del harness es `swebench/estimate.py`; esta es la
//! misma matemática para quien solo tiene el binario. Los tests de ambos fijan
//! los mismos valores esperados para la corrida real `2/10`.

use serde::Deserialize;
use std::collections::BTreeMap;

/// Por debajo de este n la Regla 12 obliga a etiquetar el resultado como
/// preliminar en lugar de presentarlo como una tasa.
pub const MIN_N_FOR_CLAIM: usize = 30;
/// Total de instancias de SWE-bench Verified (solo para extrapolación declarada).
pub const FULL_DATASET: usize = 500;

#[derive(Debug, Deserialize, Default)]
pub struct ReportRun {
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub finished_at: Option<String>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub timeout_s: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ReportSummary {
    #[serde(default)]
    pub instances: usize,
    #[serde(default)]
    pub scored: usize,
    #[serde(default)]
    pub resolved: usize,
    #[serde(default)]
    pub empty_patch: usize,
    #[serde(default)]
    pub infra_errors: usize,
}

#[derive(Debug, Deserialize, Default)]
pub struct ReportInstance {
    #[serde(default)]
    pub instance_id: String,
    #[serde(default)]
    pub scored: bool,
    #[serde(default)]
    pub resolved: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct SweReport {
    #[serde(default)]
    pub run: ReportRun,
    #[serde(default)]
    pub summary: ReportSummary,
    #[serde(default)]
    pub instances: Vec<ReportInstance>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RepoStats {
    pub attempted: usize,
    pub scored: usize,
    pub resolved: usize,
}

#[derive(Debug, Clone)]
pub struct SweEstimate {
    pub run_id: String,
    pub model: Option<String>,
    pub attempted: usize,
    pub scored: usize,
    pub resolved: usize,
    pub empty_patch: usize,
    pub infra_errors: usize,
    pub end_to_end_rate: f64,
    pub end_to_end_ci95: (f64, f64),
    pub scored_rate: f64,
    pub scored_ci95: (f64, f64),
    pub upper_bound_95: Option<f64>,
    pub preliminary: bool,
    pub per_repo: BTreeMap<String, RepoStats>,
}

impl SweEstimate {
    pub fn from_report(run_id: &str, report: &SweReport) -> Self {
        let attempted = if report.summary.instances > 0 {
            report.summary.instances
        } else {
            report.instances.len()
        };
        let scored = report.summary.scored;
        let resolved = report.summary.resolved;

        let end_to_end_rate = if attempted > 0 {
            resolved as f64 / attempted as f64
        } else {
            0.0
        };
        let scored_rate = if scored > 0 {
            resolved as f64 / scored as f64
        } else {
            0.0
        };

        let mut per_repo: BTreeMap<String, RepoStats> = BTreeMap::new();
        for instance in &report.instances {
            let repo = instance
                .instance_id
                .split("__")
                .next()
                .unwrap_or("?")
                .to_string();
            let entry = per_repo.entry(repo).or_default();
            entry.attempted += 1;
            if instance.scored {
                entry.scored += 1;
            }
            if instance.resolved {
                entry.resolved += 1;
            }
        }

        Self {
            run_id: run_id.to_string(),
            model: report.run.model.clone(),
            attempted,
            scored,
            resolved,
            empty_patch: report.summary.empty_patch,
            infra_errors: report.summary.infra_errors,
            end_to_end_rate,
            end_to_end_ci95: wilson_interval(resolved, attempted),
            scored_rate,
            scored_ci95: wilson_interval(resolved, scored),
            upper_bound_95: if resolved == 0 && attempted > 0 {
                Some(rule_of_three(attempted))
            } else {
                None
            },
            preliminary: attempted < MIN_N_FOR_CLAIM,
            per_repo,
        }
    }

    /// Extrapolación declarada a las 500 instancias. No es una predicción.
    pub fn extrapolation(&self) -> (usize, (usize, usize), &'static str) {
        (
            (self.end_to_end_rate * FULL_DATASET as f64).round() as usize,
            (
                (self.end_to_end_ci95.0 * FULL_DATASET as f64).round() as usize,
                (self.end_to_end_ci95.1 * FULL_DATASET as f64).round() as usize,
            ),
            "Extrapolación, no medición: asume que la muestra es representativa, \
             cosa que con n pequeño no se sostiene.",
        )
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Estimación SWE — corrida '{}'\n  modelo: {}\n\n",
            self.run_id,
            self.model.as_deref().unwrap_or("—")
        ));
        out.push_str(&format!(
            "  intentadas:         {}\n  parches evaluables: {}  (vacíos: {})\n  resueltas:          {}\n  errores infra:      {}\n\n",
            self.attempted, self.scored, self.empty_patch, self.resolved, self.infra_errors
        ));
        out.push_str("Tasas (intervalo de Wilson 95 %):\n");
        out.push_str(&format!(
            "  extremo a extremo: {} [{}, {}]  = {}/{}\n",
            pct(self.end_to_end_rate),
            pct(self.end_to_end_ci95.0),
            pct(self.end_to_end_ci95.1),
            self.resolved,
            self.attempted
        ));
        out.push_str(&format!(
            "  sobre evaluables:  {} [{}, {}]  = {}/{}\n",
            pct(self.scored_rate),
            pct(self.scored_ci95.0),
            pct(self.scored_ci95.1),
            self.resolved,
            self.scored
        ));
        if let Some(bound) = self.upper_bound_95 {
            out.push_str(&format!(
                "  0 resueltas: cota superior 95 % ≈ {} (regla de tres); NO es 0 %\n",
                pct(bound)
            ));
        }
        if !self.per_repo.is_empty() {
            out.push_str("\nPor repositorio:\n");
            for (repo, stats) in &self.per_repo {
                out.push_str(&format!(
                    "  {repo:<16} {}/{} (evaluables {})\n",
                    stats.resolved, stats.attempted, stats.scored
                ));
            }
        }
        out.push('\n');
        if self.preliminary {
            out.push_str(&format!(
                "⚠ PRELIMINAR: n={} < {MIN_N_FOR_CLAIM}. Es una estimación, no una tasa representativa (Regla 12).\n",
                self.attempted
            ));
        } else {
            out.push_str(&format!(
                "✔ n={} ≥ {MIN_N_FOR_CLAIM}: se puede reportar como tasa, con el intervalo indicado.\n",
                self.attempted
            ));
        }
        out
    }
}

/// Formatea una proporción como porcentaje con un decimal.
pub fn pct(value: f64) -> String {
    format!("{:.1} %", value * 100.0)
}

/// Intervalo de Wilson para una proporción (mejor que Wald con n pequeño).
pub fn wilson_interval(successes: usize, total: usize) -> (f64, f64) {
    if total == 0 {
        return (0.0, 0.0);
    }
    let z = 1.959_963_984_540_054_f64;
    let n = total as f64;
    let p = successes as f64 / n;
    let denominator = 1.0 + z * z / n;
    let centre = (p + z * z / (2.0 * n)) / denominator;
    let margin = (z / denominator) * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt();
    ((centre - margin).max(0.0), (centre + margin).min(1.0))
}

/// Cota superior aproximada del 95 % con 0 éxitos (regla de tres).
pub fn rule_of_three(total: usize) -> f64 {
    if total == 0 {
        1.0
    } else {
        3.0 / total as f64
    }
}

/// n necesario para estimar `p` con ±`margin` (aprox. normal).
pub fn sample_size_for_margin(p: f64, margin: f64) -> usize {
    let z = 1.959_963_984_540_054_f64;
    ((z * z * p * (1.0 - p)) / (margin * margin)).ceil() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(resolved: usize, attempted: usize, scored: usize) -> SweReport {
        let mut instances = Vec::new();
        for index in 0..attempted {
            instances.push(ReportInstance {
                instance_id: format!("repo{}__repo-{index}", index % 3),
                scored: index < scored,
                resolved: index < resolved,
            });
        }
        SweReport {
            run: ReportRun {
                run_id: Some("run-1".into()),
                model: Some("m".into()),
                ..Default::default()
            },
            summary: ReportSummary {
                instances: attempted,
                scored,
                resolved,
                empty_patch: attempted - scored,
                infra_errors: 0,
            },
            instances,
        }
    }

    #[test]
    fn wilson_matches_the_python_implementation() {
        // Mismos valores que `swebench/test_estimate.py` para 2/10.
        let (low, high) = wilson_interval(2, 10);
        assert!((low - 0.057).abs() < 0.002, "low={low}");
        assert!((high - 0.510).abs() < 0.002, "high={high}");
    }

    #[test]
    fn wilson_handles_extremes_and_zero_total() {
        assert_eq!(wilson_interval(0, 0), (0.0, 0.0));
        let (low, high) = wilson_interval(10, 10);
        assert_eq!(low, 0.0_f64.max(low));
        assert!(high > 0.99 && high <= 1.0, "high={high}");
    }

    #[test]
    fn end_to_end_and_scored_rates() {
        let est = SweEstimate::from_report("run-1", &report(2, 10, 8));
        assert_eq!(est.attempted, 10);
        assert_eq!(est.scored, 8);
        assert_eq!(est.resolved, 2);
        assert!((est.end_to_end_rate - 0.2).abs() < 1e-9);
        assert!((est.scored_rate - 0.25).abs() < 1e-9);
        assert!(est.preliminary, "n=10 debe ser preliminar");
    }

    #[test]
    fn not_preliminary_at_threshold() {
        let est = SweEstimate::from_report("run-1", &report(6, 30, 30));
        assert!(!est.preliminary);
    }

    #[test]
    fn zero_resolved_reports_an_upper_bound() {
        let est = SweEstimate::from_report("run-1", &report(0, 10, 10));
        assert_eq!(est.end_to_end_rate, 0.0);
        assert_eq!(est.upper_bound_95, Some(0.3));
    }

    #[test]
    fn per_repo_is_aggregated() {
        let est = SweEstimate::from_report("run-1", &report(2, 10, 8));
        let total: usize = est.per_repo.values().map(|s| s.attempted).sum();
        assert_eq!(total, 10);
        assert!(est.per_repo.len() >= 3);
    }

    #[test]
    fn sample_size_matches_python() {
        assert_eq!(sample_size_for_margin(0.2, 0.10), 62);
        assert!(sample_size_for_margin(0.2, 0.05) > sample_size_for_margin(0.2, 0.10));
    }

    #[test]
    fn text_marks_preliminary() {
        let est = SweEstimate::from_report("run-1", &report(2, 10, 8));
        let text = est.render_text();
        assert!(text.contains("PRELIMINAR"), "{text}");
        assert!(text.contains("20.0 %"), "{text}");
    }

    #[test]
    fn extrapolation_is_labelled() {
        let est = SweEstimate::from_report("run-1", &report(2, 10, 8));
        let (expected, interval, warning) = est.extrapolation();
        assert_eq!(expected, 100);
        assert_eq!(interval.0, 28); // 5.67 % de 500
        assert!(warning.contains("Extrapolación"));
    }

    #[test]
    fn parses_a_real_report_shape() {
        let json = r#"{
          "run": {"run_id": "x", "model": "qwen3-8b", "network": "none", "timeout_s": 1800},
          "summary": {"instances": 10, "scored": 8, "resolved": 2, "empty_patch": 2, "infra_errors": 0},
          "instances": [
            {"instance_id": "pytest-dev__pytest-5809", "scored": true, "resolved": true},
            {"instance_id": "astropy__astropy-13236", "scored": true, "resolved": false}
          ]
        }"#;
        let report: SweReport = serde_json::from_str(json).unwrap();
        let est = SweEstimate::from_report("x", &report);
        assert_eq!(est.attempted, 10);
        assert_eq!(est.resolved, 2);
        assert_eq!(est.model.as_deref(), Some("qwen3-8b"));
        assert_eq!(est.per_repo["pytest-dev"].resolved, 1);
    }
}
