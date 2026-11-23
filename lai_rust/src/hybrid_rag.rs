//! Hybrid RAG (v5.0, herramienta 6): FTS5 + grafos de símbolos + RRF.
//!
//! No sustituye el índice FTS5 ni el grafo de símbolos del RepoMap: añade
//! una capa de **fusión** (Reciprocal Rank Fusion, RRF) que combina la
//! recuperación por keyword (FTS5/BM25 vía `MemoryStore`) con la recuperación
//! por proximidad en el grafo de símbolos/importaciones del RepoMap. RRF
//! supera a cualquiera de las dos modalidades por separado en corpus mixtos
//! (código + documentación).
//!
//! Criterio de aceptación (v5.0): la capa se evalúa con Recall@k y MRR@k en
//! `lai bench` contra la línea base FTS5 + grafo antes de adoptarla.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Fusiona varios rankings con Reciprocal Rank Fusion.
///
/// `rankings` es una lista de rankings (cada uno, lista de ids ordenada por
/// relevancia). `k` es la constante RRF (típicamente 60).
/// Devuelve los ids ordenados por puntuación Fusión descendente.
pub fn reciprocal_rank_fusion(rankings: &[Vec<String>], k: f64) -> Vec<(String, f64)> {
    let mut scores: HashMap<String, f64> = HashMap::new();
    for ranking in rankings {
        for (idx, id) in ranking.iter().enumerate() {
            let rank = (idx + 1) as f64;
            *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k + rank);
        }
    }
    let mut out: Vec<(String, f64)> = scores.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Puntuación de un candidato por su posición en el grafo de símbolos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphScore {
    pub id: String,
    pub score: f64,
    pub reason: String,
}

/// Recuperador híbrido: keyword + grafo con fusión RRF.
#[derive(Debug, Clone, Default)]
pub struct HybridRetriever {
    /// Constante RRF (60 por defecto).
    pub k: f64,
}

impl HybridRetriever {
    pub fn new() -> Self {
        Self { k: 60.0 }
    }

    /// Construye el ranking por grafo: archivos que comparten símbolos o
    /// importaciones con los archivos "semilla" (los del ranking keyword).
    /// `symbol_index` mapea símbolo -> archivos que lo contienen;
    /// `imports_index` mapea archivo -> archivos importados.
    pub fn graph_ranking(
        &self,
        seed_files: &[String],
        symbol_index: &HashMap<String, Vec<String>>,
        imports_index: &HashMap<String, Vec<String>>,
    ) -> Vec<String> {
        let mut neighbors: HashMap<String, f64> = HashMap::new();
        for seed in seed_files {
            // Vecinos por importaciones directas.
            if let Some(imports) = imports_index.get(seed) {
                for imp in imports {
                    *neighbors.entry(imp.clone()).or_insert(0.0) += 1.0;
                }
            }
            // Archivos que comparten símbolos con la semilla (proxy de
            // similitud estructural).
            for (sym, files) in symbol_index {
                if files.iter().any(|f| f == seed) {
                    for f in files {
                        if f != seed {
                            *neighbors.entry(f.clone()).or_insert(0.0) += 0.5;
                        }
                    }
                }
                let _ = sym;
            }
        }
        let mut ranked: Vec<(String, f64)> = neighbors.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.into_iter().map(|(id, _)| id).collect()
    }

    /// Recuperación híbrida completa: fusiona el ranking keyword (ordenado
    /// por relevancia FTS5) con el ranking por grafo mediante RRF.
    pub fn retrieve(
        &self,
        keyword_ranking: &[String],
        seed_files: &[String],
        symbol_index: &HashMap<String, Vec<String>>,
        imports_index: &HashMap<String, Vec<String>>,
        limit: usize,
    ) -> Vec<String> {
        let graph = self.graph_ranking(seed_files, symbol_index, imports_index);
        let fused = reciprocal_rank_fusion(&[keyword_ranking.to_vec(), graph], self.k);
        fused.into_iter().take(limit).map(|(id, _)| id).collect()
    }
}

/// Métricas de evaluación de recuperación.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RetrievalMetrics {
    pub recall_at_k: f64,
    pub mrr_at_k: f64,
    pub queries: usize,
}

/// Evalúa un recuperador contra un conjunto de consultas con respuesta
/// esperada: `queries` es (consulta, [ids esperados]).
pub fn evaluate_retrieval<F>(k: usize, queries: &[(String, Vec<String>)], retrieve: F) -> RetrievalMetrics
where
    F: Fn(&str) -> Vec<String>,
{
    if queries.is_empty() {
        return RetrievalMetrics::default();
    }
    let mut recall_sum = 0.0;
    let mut mrr_sum = 0.0;
    for (query, expected) in queries {
        let ranked = retrieve(query);
        let topk: Vec<&String> = ranked.iter().take(k).collect();
        // Recall@k: fracción de esperados recuperados en el top-k.
        let hits = expected.iter().filter(|e| topk.iter().any(|r| *r == *e)).count();
        recall_sum += hits as f64 / expected.len().max(1) as f64;
        // MRR@k: 1/rank del primer esperado encontrado.
        let mut rr = 0.0;
        for (idx, r) in topk.iter().enumerate() {
            if expected.contains(r) {
                rr = 1.0 / (idx + 1) as f64;
                break;
            }
        }
        mrr_sum += rr;
    }
    let n = queries.len() as f64;
    RetrievalMetrics {
        recall_at_k: recall_sum / n,
        mrr_at_k: mrr_sum / n,
        queries: queries.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_combines_two_rankings() {
        let a = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let b = vec!["y".to_string(), "w".to_string()];
        let fused = reciprocal_rank_fusion(&[a, b], 60.0);
        // "y" aparece en ambos rankings -> debe liderar.
        assert_eq!(fused[0].0, "y");
        assert!(fused[0].1 > fused[1].1);
    }

    #[test]
    fn test_rrf_empty() {
        assert!(reciprocal_rank_fusion(&[], 60.0).is_empty());
    }

    #[test]
    fn test_graph_ranking_follows_imports() {
        let retriever = HybridRetriever::new();
        let mut imports: HashMap<String, Vec<String>> = HashMap::new();
        imports.insert("app.py".to_string(), vec!["db.py".to_string(), "api.py".to_string()]);
        let ranking = retriever.graph_ranking(&["app.py".to_string()], &HashMap::new(), &imports);
        assert_eq!(ranking.len(), 2);
        assert!(ranking.contains(&"db.py".to_string()));
    }

    #[test]
    fn test_hybrid_prefers_shared_symbol_neighbor() {
        let retriever = HybridRetriever::new();
        let mut symbols: HashMap<String, Vec<String>> = HashMap::new();
        symbols.insert("User".to_string(), vec!["models.py".to_string(), "views.py".to_string()]);
        let ranked = retriever.retrieve(
            &["models.py".to_string()],
            &["models.py".to_string()],
            &symbols,
            &HashMap::new(),
            5,
        );
        // Ambos, semilla y vecino por símbolo, deben aparecer en el top-k.
        assert!(ranked.contains(&"models.py".to_string()));
        assert!(ranked.contains(&"views.py".to_string()));
        assert!(ranked.len() >= 2);
    }

    #[test]
    fn test_evaluate_retrieval_metrics() {
        let queries = vec![
            ("query1".to_string(), vec!["a".to_string()]),
            ("query2".to_string(), vec!["b".to_string(), "c".to_string()]),
        ];
        // Recuperador perfecto para query1, parcial para query2.
        let metrics = evaluate_retrieval(2, &queries, |q| {
            if q == "query1" {
                vec!["a".to_string()]
            } else {
                vec!["b".to_string(), "z".to_string()]
            }
        });
        assert!((metrics.recall_at_k - 0.75).abs() < 1e-9); // 1.0 + 0.5
        assert!((metrics.mrr_at_k - 1.0).abs() < 1e-9); // 1 + 1
    }
}