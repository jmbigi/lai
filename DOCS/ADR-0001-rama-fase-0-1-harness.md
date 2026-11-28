# ADR-0001 — Rama `feat/fase-0-1-harness`: rescate selectivo y archivo

- **Fecha:** 2026-11-28
- **Estado:** Aceptado
- **Decisores:** operador del proyecto
- **Contexto técnico:** `master` en `0fd07b6`; rama en `f61d570`

## Contexto

La rama `feat/fase-0-1-harness` se creó el 2026-11-23 en un worktree aislado
para implementar la arquitectura v4.0/v5.0 (harness determinista, sandbox,
contratos, CPG, DAG, RAG híbrido, verificación formal, property testing…).
Mientras tanto, `master` avanzó en otra dirección: harness SWE-bench, perfil
`bash`, presupuesto de contexto, guardas de escritura y correcciones medidas
contra modelos reales.

Al 2026-11-28 la situación es:

| Dato | Valor |
|---|---|
| Commits exclusivos de la rama | 13 |
| Commits de `master` desde el merge-base | 47 |
| Merge-base | `7d1be85` (2026-11-23) |
| Último commit de la rama | `f61d570` (2026-11-23) |
| Diff | 44 archivos, **+10.210 / −34** |
| Worktree asociado | inexistente (marcado `prunable`) |

Ambas ramas implementaron por separado módulos con el mismo nombre
(`change_engine`, `repomap`, `telemetry`, `tools`, `policy`). La rama tiene
sandbox y 48 herramientas; `master` tiene las correcciones posteriores
(Regla 4, guarda de manifiestos, TUI sin TTY, perfil bash). **Ninguna es
superconjunto de la otra.**

## Opciones consideradas

1. **Merge completo.** Rechazada: reemplazaría los módulos de `master` por
   versiones de hace cinco días, reintroduciría dependencias eliminadas
   (`itertools`, `syn`, `axum`, `nix`, `rustls`…) y perdería correcciones
   medidas. El conflicto no es mecánico sino de arquitectura.
2. **Descartar la rama.** Rechazada: contiene dos piezas que `master` no
   tiene y que resuelven riesgos reales — el sandbox fail-closed y la capa de
   contratos para verificadores externos.
3. **Rescate selectivo + archivo.** **Elegida.**

## Decisión

### Se rescata (portado y adaptado a `master`)

| Archivo | Cambio |
|---|---|
| `lai_rust/src/sandbox.rs` | Preflight real de bubblewrap (sonda de namespaces, no solo `--version`), `build_command` con workspace rw, `/tmp`, sin red y `--cap-drop ALL`. Modos opt-in (`sandbox: true`) y requerido (`LAI_REQUIRE_SANDBOX=1` / `lai --sandbox`), **fail-closed**: si el sandbox no está disponible, la llamada falla; nunca se ejecuta sin aislamiento de forma silenciosa. |
| `lai_rust/src/contracts.rs` | Capa de admisión para verificadores externos (`ExternalVerdict` → veredicto acotado, `AdmissionGate`). Adaptada al `VerificationVerdict` de `master`; el `Verdict` original con payload no existe en esta rama. |

### No se rescata

`harness.rs`, `cpg.rs`, `dag.rs`, `spec.rs`, `formal.rs`, `hybrid_rag.rs`,
`graph_orchestrator.rs`, `mcp.rs`, `mcp_security.rs`, `memory.rs`,
`memory_eval.rs`, `nn_verifier.rs`, `property_testing.rs`,
`retrieval_bench.rs`, `trajectory.rs`, `integrity.rs`, `langfuse.rs`,
`concurrency.rs`, `bench.rs`, `verifier.rs` y los cambios de `main.rs`,
`commands.rs`, `tools.rs` y `policy.rs`.

Motivos: `master` ya tiene equivalentes para benchmark, orquestación,
verificación, memoria y MCP; el resto son capas de investigación sin medición
de admisión (la propia rama exige medir antes de adoptar). Portarlas sin esa
medición violaría Regla 6 y Regla 11.

### Archivo

1. Tag anotado `archive/fase-0-1-harness-v5` sobre `f61d570`, publicado en el
   remoto. Conserva la historia completa y el diff.
2. Eliminación de la rama local y remota `feat/fase-0-1-harness`.
3. `git worktree prune` para limpiar el worktree inexistente.
4. Este ADR queda como registro de la decisión.

## Consecuencias

- **Positivas:** desaparece la duplicación de arquitecturas; el sandbox llega a
  `master` con tests; el historial de la rama sigue recuperable desde el tag.
- **Negativas:** se pierde el trabajo de investigación no portado. Si se
  necesita, se puede `git cherry-pick` o `git show` desde el tag.
- **Pendiente:** medir la admisión de un verificador externo real; y aplicar
  `require_sandbox` por defecto cuando la experiencia con bubblewrap en
  Ubuntu 24.04 esté validada. `contracts.rs` ya está conectado al CLI mediante
  `lai verify --external` (ver `DOCS/CAMBIOS_2026-11-28_MEJORAS4.md` §11).

## Verificación

- `cargo test` → 180 tests, 0 fallos (incluye 6 de sandbox y 8 de contratos).
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` → limpios.
- `lai --sandbox auto …` falla cerrado si `bwrap` no está disponible; con
  `bwrap` funciona y bloquea la red.
