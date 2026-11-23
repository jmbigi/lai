# Documentación de cambios — 10 herramientas v5.0 (Regla 10)

**Inicio:** 2026-11-23 · **Rama:** `feat/fase-0-1-harness` · **Worktree:** `lai-wt`
**Alcance:** integrar los puntos de anclaje de las 10 herramientas priorizadas (LangGraph, α,β-CROWN/Marabou/ERAN, Mem0/Zep, Langfuse, codebadger/Joern CPG, Hybrid RAG, MCP Hangar, Dafny/TLA+, Hypothesis/fast-check, DAGent) bajo los contratos v5.0: fail-closed, verificación acotada, integridad de herramientas y **medición obligatoria contra línea base** (`lai bench`).

**Regla de admisión (del operador):** ninguna herramienta entra sin medición contra línea base; ninguna sustituye un control por una afirmación del modelo; ninguna se declara "verificada" si solo ejecutó lo que pudo, no lo que debía.

**Estado基线 (ANTES):** rama en `a678968` — v4.0 completa: `cargo check` 0 errores, `cargo test` 60/60, release OK.

---

## Cambio 1 — `contracts.rs`: mapeo de veredictos externos + puerta de admisión

**Qué/por qué:** las herramientas externas (α,β-CROWN, Dafny, Hypothesis) no declaran SUCCESS: producen safe/unsafe/timeout/unknown. Esta capa mapea esos resultados al veredicto acotado de `verifier.rs` y aplica la regla de admisión (medición obligatoria antes de adoptar).

**ANTES:** `Verdict` solo se producía desde el pipeline interno; sin mapeo para resultados externos; sin registro de mediciones de adopción.

**DESPUÉS:** `ExternalVerdict` → `Verdict` (safe→SUCCESS si requerido, GAPS si opcional; unsafe→PARTIAL/BLOCKED si requerido; timeout/unknown→GAPS si opcional, BLOCKED si requerido); `AdmissionGate` con `Measurement` (baseline vs candidato, delta, sin degradación de verificación) y `may_adopt()`.

**Verificación:** tests unitarios del mapeo y la puerta.

---

## Cambio 2 — `mcp_security.rs`: MCP Hangar / digest pinning (herramienta 7)

**Qué/por qué:** extender el `definition_hash` a servidores MCP externos: pin SHA-256 por herramienta anunciada; verificación fail-closed antes de exponer (previene tool drift / MCPoison CVE-2025-54136).

**ANTES:** solo herramientas internas con integridad; sin pins MCP.

**DESPUÉS:** `McpPinRegistry` (pin/verify/filter_verified/load_or_init/save en `.lai/mcp-pins.json`); `McpToolDef::definition_hash` canónico; sin pin o hash distinto → denegado.

**Verificación:** 5 tests (pin/verify, unpinned, drift, filter, hash por esquema).

---

## Cambio 3 — `hybrid_rag.rs`: recuperación híbrida FTS5 + grafo + RRF (herramienta 6)

**Qué/por qué:** capa de fusión RRF sobre keyword (FTS5) y grafo de símbolos/importaciones; evaluación Recall@k y MRR@k contra línea base antes de adopción.

**ANTES:** recuperación solo FTS5 o solo grafo.

**DESPUÉS:** `reciprocal_rank_fusion`, `graph_ranking` (vecinos por imports + símbolos compartidos), `retrieve`, `evaluate_retrieval` (Recall@k/MRR@k).

**Verificación:** 5 tests (RRF, grafo, híbrido, métricas).

---

## Cambio 4 — `cpg.rs`: Code Property Graph lite (herramienta 5, codebadger/Joern)

**Qué/por qué:** capa de análisis semántico sin sustituir el RepoMap: call graph, taint tracking intra-procedural (sources→sinks con path) y program slicing.

**ANTES:** índice solo estructural (símbolos + imports).

**DESPUÉS:** `build_cpg` (nodos archivo+símbolo, aristas contains/calls/imports), `slice`, `taint_analysis` (input()/env::var → eval/exec/subprocess/SQL con severidad), `summary`.

**Verificación:** 4 tests (nodos/contains, slice, taint input→subprocess, callers/callees).

---

## Cambio 5 — `dag.rs`: planificación DAG con validación estructural (herramienta 10, DAGent)

**Qué/por qué:** sustituir el plan lineal por DAG con dependencias explícitas, capas topológicas, grow incremental y ejecución fail-closed (nodos críticos sin completar → error).

**ANTES:** plan lineal (Vec<PlanStep>).

**DESPUÉS:** `DagPlan` (layers/validate/grow/progress), `execute_layers` fail-closed, `parse_plan` validado, `linear_to_dag` fallback, `GrowQueue` (evaluate-then-grow).

**Verificación:** 8 tests (capas, ciclo, dependencia inexistente, grow, fail-closed, parse, lineal).

---

## Cambio 6 — `property_testing.rs`: Hypothesis/fast-check con mutation testing (herramienta 9)

**Qué/por qué:** patrón goalie: propiedades deterministas con semilla fija + validación de la suite inyectando bugs (`<=`→`<`, etc.); una propiedad que no detecta el bug inyectado es débil (Unknown, no SUCCESS).

**ANTES:** sin testing basado en propiedades.

**DESPUÉS:** `run_property` (PRNG determinista, counterexample), `mutation_bugs`, `validate_suite`, mapeo a `ExternalVerdict` (Safe/Unsafe/Unknown).

**Verificación:** 6 tests.

---

## Cambio 7 — `formal.rs`: contratos ejecutables Dafny-lite (herramienta 8)

**Qué/por qué:** patrón generate→check→repair sobre `requires`/`ensures` declarados en SPEC.md; evaluación determinista con python3; contrato requerido que no cierra → BLOCKED (nunca gap).

**ANTES:** sin contratos formales.

**DESPUÉS:** `ContractSpec`, `evaluate_expression` (estado JSON como scope), `check_contract`, `parse_contracts_from_spec`, mapeo a Verdict vía contracts.rs.

**Verificación:** 7 tests (evaluación true/false/no-evaluable, contrato cerrado/violado, parse SPEC, required→BLOCKED).

---

---

## Cambio 8 — Integración CLI y `lai bench` (concurrencia, retrieval, contratos, NN, memoria, CPG, telemetría, grafo)

**Qué/por qué:** exponer las 10 herramientas por línea de comandos y conectar la medición obligatoria a `lai bench`.

**ANTES:** los módulos existían pero sin punto de entrada CLI ni métricas de adopción.

**DESPUÉS:** `lai bench [--concurrency N]` ejecuta tareas en paralelo aisladas en worktrees (DAGent + 4.1/4.4); `lai bench --retrieval` mide Recall@k/MRR@k del hybrid RAG vs línea base keyword (alimenta la AdmissionGate); `lai mcp-pin`/`lai mcp-verify` (MCP Hangar); `lai contracts` (Dafny-lite); `lai nn-verify` (α,β-CROWN/Marabou/ERAN adapter); `lai memory-eval` (LOCOMO Recall/MRR); `lai cpg` (taint/slice); `lai telemetry-export` (batches Langfuse); `lai graph-run` (LangGraph grafo con presupuesto de tokens).

**Verificación:** `cargo check` 0 errores · `cargo test` 121/102+21.

**Estado del bloque 1:** `cargo test` 121 en verde (antes 60 en v4.0).

---

## Resumen final (2026-11-23)

- **Tests:** 121/121 (baseline v4.0: 60).
- **Build:** `cargo check` 0 errores · `cargo build --release` OK.
- **Contratos v5.0 cumplidos:** fail-closed (MCP pins, integridad, sandbox, Change Engine rollback, required-contracts BLOCKED), veredicto acotado (SUCCESS/GAPS/BLOCKED vía contracts.rs), integridad de herramientas (definition_hash en TOOL, journal de cambios, pins MCP), medición obligatoria (AdmissionGate, retrieval bench Recall@k/MRR@k, memory-eval LOCOMO, `--concurrency` en bench).
- **Regla 0:** todo local; sin cloud/API keys. Langfuse es fail-open y solo POSTea a `LANGFUSE_HOST` si se configura (default: batch local).
- **Próximo paso (auto-mejora controlada):** adoptar una herramienta externa solo cuando `lai bench` demuestre mejora contra la línea base sin degradar verificación.