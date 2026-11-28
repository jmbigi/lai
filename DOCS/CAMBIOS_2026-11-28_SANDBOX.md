# Cambios 2026-11-28 — sandbox bubblewrap, contratos y guarda de `apply_patch`

Registro según **Regla 10**. Continúa `DOCS/CAMBIOS_2026-11-28_CI_HIGIENE.md`.
Decisión de origen: `DOCS/ADR-0001-rama-fase-0-1-harness.md`.

## 0. Punto de partida

| Comprobación | Resultado inicial |
|---|---|
| `HEAD` | `0fd07b6`, árbol limpio |
| `cargo test` | 158 tests, 0 fallos |
| `pytest tests swebench` | 241 passed, 9 skipped |
| Sandbox en Rust | ❌ no existía |
| Guarda de manifiestos | solo `write_file` y `edit` |

El CLI Rust ejecutaba `run_command` con `sh -c` y `run_python` con `python3`
sin aislamiento. `apply_patch` podía reemplazar un manifiesto entero.

---

## 1. ADR de la rama `feat/fase-0-1-harness`

**Archivo:** `DOCS/ADR-0001-rama-fase-0-1-harness.md`.

**ANTES:** la rama tenía 13 commits y 10.210 líneas sin integrar; `master`
estaba 47 commits adelante. Ambas implementaron por separado módulos con el
mismo nombre. No había decisión escrita.

**DESPUÉS:** ADR con las tres opciones evaluadas. Se elige **rescate selectivo
+ archivo**: se portan `sandbox.rs` y `contracts.rs`; el resto (harness, CPG,
DAG, spec, formal, RAG híbrido, MCP, memoria, trayectorias…) no se porta porque
`master` ya tiene equivalentes o porque la propia rama exige medición de
admisión antes de adoptarlos. La rama se etiqueta y archiva.

---

## 2. `lai_rust/src/sandbox.rs`

**ANTES:** no había módulo. `run_shell_tool` lanzaba `sh -c` directamente;
`run_python` lanzaba `python3` con el script temporal.

**DESPUÉS**

- `preflight()` ejecuta una **sonda real** de namespaces (`bwrap --unshare-pid
  --unshare-net true`), no solo `--version`; detecta la restricción de user
  namespaces de Ubuntu 24.04+. El resultado se cachea con `OnceLock`.
- `build_command(command, workspace, required)` construye el comando bwrap:
  raíces del sistema en solo lectura, workspace en lectura/escritura, `/tmp`,
  caché de Cargo/Rustup, `--unshare-all`, `--die-with-parent`, `--new-session`,
  `--cap-drop ALL`.
- **Fail-closed:** con `required = true`, un sandbox no disponible devuelve
  error; nunca se cae a ejecución sin aislamiento.
- Modos:
  - opt-in: `"sandbox": true` en la herramienta;
  - obligatorio: `LAI_REQUIRE_SANDBOX=1` o `lai --sandbox` (flag global).
  Con el modo obligatorio, `"sandbox": false` se rechaza explícitamente.
- En sistemas no-Unix devuelve un error claro.

**Verificación**

| Caso | Resultado |
|---|---|
| `echo sandbox-ok` con `sandbox: true` | ✅ |
| `getent hosts example.com` con sandbox | ✅ `NO_NET` (sin red) |
| `sandbox: false` con `required` | ✅ error, no ejecuta |
| `--sandbox` sin bwrap (PATH vacío) | ✅ error accionable |

---

## 3. `lai_rust/src/contracts.rs`

**ANTES:** el mapeo de veredictos externos solo existía en la rama, ligado a un
`Verdict` con payload que `master` no tiene.

**DESPUÉS:** portado y adaptado a `VerificationVerdict`:

- `ExternalVerdict::parse` reconoce safe/proved/unsafe/counterexample/timeout…
- `to_verdict(required)` aplica la regla acotada: requerido + unsafe/timeout/
  unknown → `Blocked`; opcional + unsafe → `Failed`; opcional + timeout/unknown
  → `SuccessWithDeclaredGaps`.
- `AdmissionGate` exige una medición sin regresión antes de adoptar una
  herramienta externa.

**Límite declarado:** el módulo compila y tiene tests, pero **no está conectado
a un verificador externo real** (Dafny, TLA+, α,β-CROWN…). Se porta la
semántica para que el futuro adaptador no invente reglas.

---

## 4. Guarda de manifiestos en `apply_patch`

**ANTES:** `apply_patch` no pasaba por ninguna guarda; un diff podía reemplazar
o borrar `Cargo.toml`/`package.json`.

**DESPUÉS**

- `manifest_patch_violation()` analiza el diff: cuenta líneas `-`/`+` por hunk y
  rechaza cuando se eliminan **≥75 %** de las líneas actuales de un manifiesto,
  o cuando el parche lo borra (`+++ /dev/null`).
- Un cambio acotado (añadir una dependencia, cambiar una línea) sigue permitido.
- `allow_manifest_overwrite: true` desactiva la guarda.
- El esquema de la herramienta documenta la bandera.

**Verificación:** 4 tests — reemplazo total rechazado, cambio acotado
permitido, borrado rechazado, opt-in permitido.

> **Límite declarado:** el perfil `bash` sigue pudiendo saltarse la guarda con
> `cat > Cargo.toml` o un heredoc. Inspeccionar comandos arbitrarios exigiría
> interpretar el shell; se declara en `AGENTS.md`.

---

## 5. CLI

- `lai --sandbox <cmd>`: flag global que define `LAI_REQUIRE_SANDBOX=1` antes de
  crear el `ToolRegistry`.
- Esquemas de `run_command`, `bash`, `run_python` y `apply_patch` actualizados.
- `--help` documenta el flag.

---

## 6. Verificación completa

| Comprobación | Resultado |
|---|---|
| `cargo test` | ✅ **180 tests** (138 lib + 42 integración), 0 fallos |
| `cargo fmt --check` | ✅ |
| `cargo clippy --all-targets -- -D warnings` | ✅ |
| `pytest tests swebench` | ✅ 241 passed, 9 skipped |
| `ruff check .` | ✅ |
| `bash -n scripts/ci_local.sh` | ✅ |
| `make gateway-check` | ✅ |
| Sandbox con red bloqueada | ✅ `NO_NET` |
| `--sandbox` sin bwrap | ✅ falla cerrado |

Tests Rust nuevos: **22** (6 sandbox + 8 contratos + 4 `apply_patch` + 4 shell).

---

## 7. Lo que NO se ha probado (Regla 6)

- **Sandbox con modelos reales.** Se probó con comandos directos y tests; no se
  repitió SWE-bench con `--sandbox`. No hay medición de cuántos parches fallan
  por la falta de red (descargas de dependencias) dentro del sandbox.
- **macOS/Windows.** El sandbox solo existe en Linux/Unix; en otros sistemas
  devuelve error. No se probó la compilación cruzada.
- **`contracts.rs` con verificadores reales.** Sin adaptador conectado.
- **Rama archivada.** El tag y la eliminación de la rama se ejecutan en el
  commit de cierre; no se probó un `git clone` desde el tag.
- **Allowlist de comandos en `auto`.** Sigue sin aplicarse; el sandbox mitiga
  pero no sustituye la decisión de producto.
