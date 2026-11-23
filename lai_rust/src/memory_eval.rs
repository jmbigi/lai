//! Evaluación de memoria (v5.0, herramienta 3: Mem0 / Zep).
//!
//! Mem0 (v0.3.5: 84.5 % LOCOMO) y Zep (v0.8: 82.1 %) solo deben adoptarse
//! si **mejoran LOCOMO o reducen tokens** sin degradar la tasa de verificación
//! efectiva. Este módulo evalúa el `MemoryStore` propio (FTS5) contra un
//! dataset estilo LOCOMO (preguntas + respuestas esperadas) y produce las
//! métricas que la `AdmissionGate` (`contracts.rs`) exige antes de adoptar un
//! backend externo.
//!
//! Formato del dataset (JSONL): una pregunta por línea
//! `{"question": "...", "expected": ["contenido esperado", ...]}`.

use crate::memory::MemoryStore;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Entrada del dataset LOCOMO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoComoQuestion {
    pub question: String,
    pub expected: Vec<String>,
}

/// Métricas de evaluación de memoria.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryMetrics {
    pub questions: usize,
    pub recall_at_k: f64,
    pub mrr_at_k: f64,
    pub avg_latency_ms: f64,
    /// Tokens aproximados del contexto recuperado (chars/4).
    pub avg_context_tokens: f64,
}

/// Carga el dataset JSONL.
pub fn load_dataset(path: &Path) -> std::io::Result<Vec<LoComoQuestion>> {
    let content = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in content.lines().filter(|l| !l.trim().is_empty()) {
        if let Ok(q) = serde_json::from_str::<LoComoQuestion>(line) {
            out.push(q);
        }
    }
    Ok(out)
}

/// Evalúa el MemoryStore contra el dataset.
pub fn evaluate_memory(store: &MemoryStore, dataset: &[LoComoQuestion], k: usize) -> MemoryMetrics {
    if dataset.is_empty() {
        return MemoryMetrics::default();
    }
    let mut recall_sum = 0.0;
    let mut mrr_sum = 0.0;
    let mut latency_sum = 0.0;
    let mut tokens_sum = 0.0;

    for q in dataset {
        let start = std::time::Instant::now();
        let results = store.recall(&q.question, None, k).unwrap_or_default();
        let latency = start.elapsed().as_micros() as f64 / 1000.0;

        // Recall@k: fracción de esperados presentes en los resultados.
        let hits = q
            .expected
            .iter()
            .filter(|e| results.iter().any(|r| r.content.contains(e.as_str())))
            .count();
        recall_sum += hits as f64 / q.expected.len().max(1) as f64;

        // MRR@k: 1/rank del primer resultado que contiene un esperado.
        let mut rr = 0.0;
        for (idx, r) in results.iter().enumerate() {
            if q.expected.iter().any(|e| r.content.contains(e.as_str())) {
                rr = 1.0 / (idx + 1) as f64;
                break;
            }
        }
        mrr_sum += rr;

        latency_sum += latency;
        tokens_sum += results.iter().map(|r| r.content.len()).sum::<usize>() as f64 / 4.0;
    }

    let n = dataset.len() as f64;
    MemoryMetrics {
        questions: dataset.len(),
        recall_at_k: recall_sum / n,
        mrr_at_k: mrr_sum / n,
        avg_latency_ms: latency_sum / n,
        avg_context_tokens: tokens_sum / n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{MemoryEntry, MemoryStore};

    fn seeded_store() -> (MemoryStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("lai-mem-test-{}", uuid::Uuid::new_v4()));
        let db = dir.join("memory.db");
        let store = MemoryStore::open(&db).unwrap();
        store
            .remember(&MemoryEntry::semantic("proj", "El módulo auth usa JWT con expiración de 15 minutos"))
            .unwrap();
        store
            .remember(&MemoryEntry::semantic("proj", "La base de datos es PostgreSQL en el puerto 5433"))
            .unwrap();
        store
            .remember(&MemoryEntry::episodic("task-1", "Se cambió validate_token para aceptar refresh tokens", None))
            .unwrap();
        (store, dir)
    }

    #[test]
    fn test_evaluate_memory_recall() {
        let (store, dir) = seeded_store();
        // Queries estilo LOCOMO con solapamiento léxico (el recall es
        // keyword/FTS5; las queries puramente semánticas miden 0 y son la
        // justificación del hybrid RAG, no una métrica de adopción).
        let dataset = vec![
            LoComoQuestion {
                question: "¿Cómo se autentican los usuarios con JWT?".to_string(),
                expected: vec!["JWT".to_string()],
            },
            LoComoQuestion {
                question: "¿Qué base de datos PostgreSQL se usa?".to_string(),
                expected: vec!["PostgreSQL".to_string()],
            },
        ];
        let m = evaluate_memory(&store, &dataset, 3);
        assert_eq!(m.questions, 2);
        assert!(m.recall_at_k > 0.5, "recall esperado > 0.5, obtenido {}", m.recall_at_k);
        assert!(m.mrr_at_k > 0.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_empty_dataset() {
        let (store, dir) = seeded_store();
        let m = evaluate_memory(&store, &[], 3);
        assert_eq!(m.questions, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}