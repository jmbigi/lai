# lai 4.0 — Handoff de implementación

Fecha: 2026-11-23 · Rama: `feat/fase-0-1-harness` (mergeada sobre `master` en el remoto `github-jmbigi:jmbigi/lai.git`)

## Qué se construyó

Implementación completa de la especificación v4.0 (`DOCS/ARQUITECTURA_TECNICA_LAI_v4.md`):

| Componente | Módulo | Estado |
|---|---|---|
| ExecutionEngine (FSM INTAKE/INSPECT/PLAN/LOCATE/EDIT/VERIFY/REPAIR/REPORT) | `harness.rs` | ✅ |
| PolicyEngine fail-closed (allowlist, frontera, rutas protegidas, secretos, require_sandbox) | `policy.rs` | ✅ |
| Integridad de herramientas (definition_hash SHA-256, verificación antes de cada invocación) | `integrity.rs` + `tools.rs` | ✅ |
| Sandbox preflight (bubblewrap + namespaces; AppArmor → fail-closed) | `sandbox.rs` | ✅ |
| RepoMap incremental (AST syn + heurístico, update por hash) | `repomap.rs` | ✅ |
| ContextBuilder (presupuesto de tokens) | `context.rs` | ✅ |
| Spec-Driven Planning (generate_spec, 2 reintentos, SPEC.md) | `spec.rs` | ✅ |
| VerificationEngine (niveles 1-10, veredicto acotado, baseline, regresiones) | `verifier.rs` | ✅ |
| MemoryStore (SQLite + FTS5, invalidación por definition_hash, destilación) | `memory.rs` | ✅ |
| MCP server stdio JSON-RPC 2.0 | `mcp.rs` | ✅ |
| BenchmarkEngine + comparativa + suite (6 tareas) | `bench.rs`, `benchmarks/tasks/` | ✅ |
| Observabilidad v4.0 (tool_definition_hash, policy_decision, verification_levels) | `telemetry.rs` | ✅ |
| Trayectorias (base auto-mejora) + `lai trajectories` | `trajectory.rs` | ✅ |
| Aislamiento por tarea (worktrees, transversal 4.1) | `harness.rs` + `--worktree/--worktree-merge` | ✅ |
| Change Engine transaccional (transversal 4.2) | `change_engine.rs` + `tools.rs` | ✅ |
| Concurrencia: locks (repo,índice) y (repo,worktree) (transversal 4.4) | `concurrency.rs` | ✅ |
| Recuperación: checkpoints con RepoMap + `lai recover` | `harness.rs`, `commands.rs` | ✅ |

## Verificación

- `cargo check`: 0 errores.
- `cargo test`: **60/60 en verde**.
- `cargo build --release`: OK.
- Smoke tests: `lai mcp` (tools/list + tools/call write_file/apply_patch con journal y `[verified]`), `lai recover` (checkpoint sintético), `lai bench --help`, `lai trajectories`, `lai harness --help`.

## Commits en la rama

```
085a474 feat: concurrencia con locks (transversal 4.4) y suite de benchmark ampliada
ed8b7a6 chore: limpiar artefactos de prueba y versionar .lai/.benchmarks en .gitignore
2aca2a0 feat: Change Engine transaccional (transversal 4.2) en write_file y apply_patch
10e2e16 feat: aislamiento por tarea con worktrees (transversal 4.1) y lai trajectories
d3ce41c feat: observabilidad v4.0 completa, trayectorias, recover y comparativa de bench
64c87bb feat: Fases 1-6 v4.0 - integridad, sandbox, verificación acotada, spec, memoria y bench
454f66f feat: Fase 0/1 - ExecutionHarness determinista, RepoMap, ContextEngine y PolicyEngine fail-closed
7d1be85 feat: T1 PolicyEngine - guardrails deterministas para todas las tool calls
```

## Pendiente (largo plazo, experimental en la spec)

- Auto-mejora controlada: análisis offline de trayectorias + adopción de cambios sobre lai condicionada a benchmark. La infraestructura de medida ya está (trayectorias, `TrajectoryStats`, `lai bench --compare-with`).

## Notas operativas

- El worktree de trabajo fue `lai-wt` con `CARGO_TARGET_DIR` privado (la árvore principal del repo estaba siendo modificada por otros agentes).
- Regla 0 intacta: todo corre local (llama.cpp/Ollama); sin API keys ni cloud.
- Los artefactos de ejecución (`.lai/`, `.benchmarks/`, `*.rej`, `*.orig`) están en `.gitignore`.