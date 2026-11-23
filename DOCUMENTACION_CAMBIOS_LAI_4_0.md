# Documentación de cambios — Implementación LAI v4.0 (Regla 10)

**Vigente desde:** 2026-11-23
**Rama:** `feat/fase-0-1-harness` (remoto `github-jmbigi:jmbigi/lai.git`)
**Alcance:** implementación de la especificación v4.0 (`DOCS/ARQUITECTURA_TECNICA_LAI_v4.md`) sobre `lai_rust/`.
**Nota de cumplimiento:** este documento se elabora retroactivamente para cumplir la Regla 10 (documentación obligatoria de cambios con estado antes/después por cada cambio). Los cambios se ejecutaron en un worktree aislado (`lai-wt`) porque la árvore principal estaba siendo modificada por otros agentes.

---

## Línea base (ANTES de todos los cambios)

- **Commit base:** `7d1be85` — "feat: T1 PolicyEngine - guardrails deterministas para todas las tool calls".
- **Versión:** `lai 0.1.0` (`lai_rust/Cargo.toml`).
- **Comportamiento observado:** existía `policy.rs` (PolicyEngine básico) y el ToolRegistry con ~50 herramientas; el harness determinista, la integridad de definiciones, el sandbox, la verificación acotada, la memoria, el benchmark y el MCP no existían.
- **Tests relevantes:** 17 passing (medidos en `454f66f`, primer commit de esta serie).
- **Build:** `cargo check` y `cargo build` en verde.

---

## Cambio 1 — `454f66f` · Fase 0/1: ExecutionHarness determinista, RepoMap, ContextEngine y PolicyEngine fail-closed

**Qué se modificó y por qué:** implementar el núcleo del runtime determinista de la v4.0 (ExecutionEngine FSM, RepoMap con índice AST, ContextBuilder con presupuesto de tokens) y reforzar el PolicyEngine (detección de secretos en `write_file`/`apply_patch`) como base del resto de fases.

**Archivos afectados:** `harness.rs` (nuevo), `repomap.rs` (nuevo), `context.rs` (nuevo), `telemetry.rs` (nuevo), `policy.rs` (fix secretos), `tools.rs` (hook `call()`), `commands.rs` (comando `harness`), `main.rs` (módulos + subcomando), `DOCS/ARQUITECTURA_TECNICA_LAI.md` y `_v4.md` (nuevos).

**ANTES:** sin harness ni índice; el ToolRegistry no tenía integridad; el PolicyEngine no detectaba secretos en escrituras.

**DESPUÉS:** FSM INTAKE/INSPECT/PLAN/LOCATE/EDIT/VERIFY/REPAIR/REPORT con checkpoints y límites; RepoMap con `summary()`/`find_files`/`find_symbols`; ContextEngine con truncamiento registrado; telemetría JSONL; PolicyEngine bloquea secretos.

**Verificación:** `cargo check` 0 errores · `cargo test` 17/17.

---

## Cambio 2 — `64c87bb` · Fases 1-6 v4.0: integridad, sandbox, verificación acotada, spec, memoria y bench

**Qué se modificó y por qué:** completar las fases 1-6 del roadmap: `definition_hash` por herramienta (fijado al registrar, verificado antes de cada invocación), preflight de sandbox (bubblewrap + namespaces, fail-closed), veredicto acotado (`SUCCESS`/`SUCCESS_WITH_DECLARED_GAPS`/`PARTIALLY_VERIFIED`/`BLOCKED`) con baseline y regresiones, planificación Spec-Driven con `verification_scope`, memoria SQLite+FTS5 con invalidación por hash, y `lai bench`.

**Archivos afectados:** `integrity.rs`, `sandbox.rs`, `verifier.rs`, `spec.rs`, `memory.rs`, `bench.rs`, `mcp.rs` (nuevos); `policy.rs` (`require_sandbox`), `tools.rs` (verificación de integridad), `repomap.rs` (`update()` incremental), `harness.rs` (veredicto, spec, integridad en `edit_phase`), `Cargo.toml` (`rusqlite` bundled), `benchmarks/tasks/*.json` (3 tareas).

**ANTES:** sin integridad de definiciones, sin sandbox, verificación no acotada (binaria), sin spec persistida, sin memoria, sin bench.

**DESPUÉS:** cada tool call verifica `definition_hash` (fail-closed); `--sandbox` exige preflight verificado; veredicto acotado con niveles 1-10; `SPEC.md` en `.lai/specs/`; memoria semántica/episódica con FTS5; `lai bench` con métricas versionadas.

**Verificación:** `cargo check` 0 errores · `cargo test` 45/45.

---

## Cambio 3 — `d3ce41c` · Observabilidad v4.0 completa, trayectorias, recover y comparativa de bench

**Qué se modificó y por qué:** cerrar el transversal 4.3 (campos obligatorios `tool_definition_hash`, `policy_decision`, `verification_levels_attempted/passed`), registrar trayectorias por tarea (base de la auto-mejora de Fase 6), añadir `lai recover` y comparativa de benchmarks.

**Archivos afectados:** `telemetry.rs` (campos nuevos + `record_verification`), `trajectory.rs` (nuevo), `harness.rs` (registro de niveles y acciones), `context.rs` (`repo_map_snapshot`), `bench.rs` (`compare_results`), `commands.rs` (`cmd_recover`, comparativa), `main.rs`.

**ANTES:** eventos TOOL sin `definition_hash`; sin registro de verificación; checkpoints sin estado del RepoMap; sin trayectorias; sin recuperación; sin comparativas.

**DESPUÉS:** traza completa que responde "¿estuvo autorizada y fue verificada?"; trayectorias JSONL con acciones y hashes; checkpoints con `repo_map_snapshot`; `lai recover [--task]`; `lai bench --compare-with` con marcas REGRESION/MEJORA.

**Verificación:** `cargo check` 0 errores · `cargo test` 49/49.

---

## Cambio 4 — `10e2e16` · Aislamiento por tarea con worktrees (transversal 4.1) y `lai trajectories`

**Qué se modificó y por qué:** aislar cada tarea en un worktree git propio (`lai/task-<slug>-<id>`), confinar la frontera del PolicyEngine al worktree, fusionar solo tras éxito verificado; añadir análisis de trayectorias para auto-mejora.

**Archivos afectados:** `harness.rs` (`create_worktree`/`create_worktree_in`/`remove_worktree`/`merge_worktree`), `commands.rs` (`--worktree`, `--worktree-merge`, `cmd_trajectories`), `main.rs`.

**ANTES:** todas las tareas compartían el working tree; sin aislamiento filesystem; sin análisis de trayectorias.

**DESPUÉS:** `lai harness --worktree` aísla; `--worktree-merge` fusiona tras éxito; fallo → worktree eliminado; `lai trajectories` agrega métricas (success_rate, verificación efectiva, tasa de fallo de tools).

**Verificación:** `cargo check` 0 errores · `cargo test` 50/50 (incluye `test_worktree_lifecycle` con repo git real: crear → editar → la rama principal NO ve el cambio → merge → remove).

---

## Cambio 5 — `2aca2a0` · Change Engine transaccional (transversal 4.2) en `write_file` y `apply_patch`

**Qué se modificó y por qué:** aplicar el flujo `validar → diff → checkpoint → aplicar → verificar integridad` a toda modificación de archivos, con journal y rollback verificado por hash.

**Archivos afectados:** `change_engine.rs` (nuevo), `tools.rs` (write_file/apply_patch transaccionales, `set_workspace_root`), `harness.rs` (cablea workspace), `main.rs`.

**ANTES:** `write_file` escribía directo sin journal; `apply_patch` ignoraba el parámetro `path` (bug latente); sin rollback ante modificación concurrente.

**DESPUÉS:** journal por transacción en `.lai/transactions/<id>.json` (original + hashes SHA-256 + estado); escritura atómica (tmp+rename); rollback automático verificado por hash si un proceso externo modifica el archivo; `apply_patch` usa `path` como objetivo real.

**Verificación:** `cargo check` 0 errores · `cargo test` 56/56 (6 tests nuevos: commit, reemplazo, manipulación+rollback, rollback de archivo nuevo, journal, vectores SHA-256). Smoke E2E vía MCP: `write_file` → `[verified]` + journal; `apply_patch` → `[verified]`.

---

## Cambio 6 — `ed8b7a6` · Limpieza de artefactos y `.gitignore`

**Qué se modificó y por qué:** eliminar `ce-e2e-test.txt.orig/.rej` (artefactos de prueba colados en el commit anterior) e ignorar `.lai/`, `.benchmarks/`, `*.rej`, `*.orig`.

**Archivos afectados:** `.gitignore`, artefactos eliminados.

**ANTES/DESPUÉS:** sin artefactos versionados; artefactos de ejecución fuera del control de versiones.

**Verificación:** `git status` limpio.

---

## Cambio 7 — `085a474` · Concurrencia con locks (transversal 4.4) y suite de benchmark ampliada

**Qué se modificó y por qué:** locks por `(repo, índice)` y `(repo, worktree)` para coordinar tareas concurrentes sin indexar el mismo repo a la vez; ampliar la suite de benchmark a 6 tareas.

**Archivos afectados:** `concurrency.rs` (nuevo), `harness.rs` (lock de índice en `inspect_phase`), `benchmarks/tasks/` (+3 tareas: test-driven-fix, code-generation, doc-update).

**ANTES:** sin coordinación entre tareas; 3 tareas de benchmark.

**DESPUÉS:** `mkdir` atómico como primitivo cross-proceso, recuperación de locks stale por liveness de PID (`/proc`), liberación automática (Drop); 6 tareas de benchmark.

**Verificación:** `cargo check` 0 errores · `cargo test` 60/60 (4 tests de locks: acquire/release, busy, stale, independencia índice/worktree).

---

## Cambio 8 — `a12b379` · Handoff final

**Qué se modificó y por qué:** documentar el estado final para continuidad (`DOCS/LAI_4_0_HANDOFF.md`).

**Verificación:** `cargo build --release` OK (2m55s).

---

## Estado final (DESPUÉS de todos los cambios)

- **Tests:** 60/60 en verde (baseline: 17).
- **Build:** `cargo check` 0 errores · `cargo build --release` OK.
- **Pendiente según spec v4.0:** auto-mejora controlada completa (experimental en la spec; la infraestructura de medida ya está: trayectorias + `lai trajectories` + `lai bench --compare-with`).
- **Regla 0:** cumplida — todo el runtime usa modelos locales; sin cloud/API keys.
- **Regla 4:** la salida del harness se genera con el modelo local configurado (`qwen3-8b` en `127.0.0.1:8080`).

## Seguimiento

Para cambios futuros en esta rama o en `master`, crear un documento de cambios con el mismo formato (qué/por qué/archivos/ANTES/DESPUÉS/verificación) **antes** de modificar código, y actualizarlo después de cada commit.