# Cambios 2026-11-28 — lote A–D del plan de mejoras

Registro según **Regla 10**. Plan: `DOCS/PLAN_MEJORAS_2026-11-28.md`.
Continúa `DOCS/CAMBIOS_2026-11-28_SANDBOX.md`.

## 0. Punto de partida

`HEAD = 62587b1`, 180 tests Rust, 241 Python. Sandbox opt-in/fail-closed ya
presente; guarda de manifiestos solo en `write_file`, `edit` y `apply_patch`.

## 1. Diagnóstico del sandbox (lote A)

**ANTES:** `lai doctor` y `lai status` no decían nada del sandbox; el usuario
descubría que `--sandbox` no funcionaba solo al fallar una ejecución.

**DESPUÉS**

- `sandbox::SandboxReport` con `collect()`, `is_blocking()`, `short_line()` y
  `detail_lines()`.
- `lai doctor` imprime bwrap, preflight, modo y red; si
  `LAI_REQUIRE_SANDBOX=1` y no hay sandbox, lo marca como **error**, no aviso.
- `lai status` añade una línea `sandbox: …`.

**Verificación:** `lai status` → `sandbox: disponible (bubblewrap, opt-in, red
bloqueada)`; `lai doctor` muestra los cuatro campos.

## 2. Red dentro del sandbox (lote B)

**ANTES:** `--unshare-all` bloqueaba siempre la red; una tarea que necesitara
descargar dependencias fallaba sin alternativa.

**DESPUÉS:** `build_command_full(..., allow_net, ...)` añade `--share-net`
después de `--unshare-all` cuando se permite. Se activa con
`LAI_SANDBOX_ALLOW_NET=1` o `"sandbox_network": true`. El resto de namespaces
(PID, mount, IPC, UTS) sigue aislado. Por defecto **sigue bloqueada**.

**Verificación E2E:** con `LAI_SANDBOX_ALLOW_NET=1` y `--sandbox`, el comando
`getent hosts example.com` responde `E2E_NET_UP`; sin la variable, `E2E_NO_NET`.

## 3. Guarda de manifiestos en `bash` (lote C)

**ANTES:** el perfil `bash` podía ejecutar `echo x > Cargo.toml` o
`rm package.json` sin ninguna guarda. Era el límite declarado en
`CAMBIOS_2026-11-28.md`.

**DESPUÉS**

- `shell_manifest_violation()` tokeniza el comando (respetando comillas) y
  rechaza:
  - redirecciones `>`/`>>` a un manifiesto (`echo x > Cargo.toml`, incluido
    heredoc `cat > package.json <<EOF`);
  - `rm`, `tee` y `truncate` sobre un manifiesto;
  - `mv`/`cp`/`install` cuyo destino es un manifiesto existente;
  - `dd of=Cargo.lock`.
- `allow_manifest_overwrite: true` desactiva la guarda.
- Es **mejor esfuerzo**: no interpreta el shell. La ofuscación
  (`cat $VAR > $MANIFEST`, `python -c`, `bash -c`) puede saltarla; el sandbox es
  el control que no depende de parsear el comando.

**Verificación:** 6 tests nuevos (redirección, heredoc, `rm`/`tee`/`dd`, lectura
permitida, opt-out, tokenizador con comillas).

## 4. Allowlist de comandos opt-in en `auto` (lote D)

**ANTES:** `auto` aplicaba el `PolicyEngine` solo a escrituras; los comandos se
ejecutaban sin allowlist porque la lista por defecto bloquearía `cd`, `sed`,
`make`, etc.

**DESPUÉS**

- `LAI_COMMAND_ALLOWLIST=1` hace que `run_command`, `bash` y `run_python` pasen
  por `call_tool` (y por tanto por el `PolicyEngine`).
- `LAI_ALLOW_COMMANDS=make,sed,…` amplía la allowlist.
- `LAI_PROTECTED_PATHS=…` amplía las rutas protegidas.
- `PolicyConfig::from_env()` construye la configuración.
- Por defecto **sigue desactivada**: no cambia el comportamiento de benchmarks
  ni de flujos existentes.

**Verificación E2E con servidor falso:** con `LAI_COMMAND_ALLOWLIST=1`,
`echo` pasa y `curl http://example.com` es rechazado con
`PolicyEngine blocked tool 'run_command': Not allowed: curl`.

## 5. Verificación completa

| Comprobación | Resultado |
|---|---|
| `cargo test` | ✅ **194 tests** (152 lib + 42 integración), 0 fallos |
| `cargo fmt --check` | ✅ |
| `cargo clippy --all-targets -- -D warnings` | ✅ |
| `pytest tests swebench` | ✅ 241 passed, 9 skipped |
| `ruff check .` | ✅ |
| `lai status` con sandbox | ✅ línea nueva |
| `lai doctor` con sandbox | ✅ cuatro campos |
| Red permitida explícitamente | ✅ `E2E_NET_UP` |
| Red bloqueada por defecto | ✅ `E2E_NO_NET` |
| Allowlist ON: `curl` | ✅ rechazado |
| Guarda `bash` sobre manifiestos | ✅ 6 tests |

Tests Rust nuevos en esta tanda: **14** (2 red, 2 report, 6 guarda shell,
4 policy).

## 6. Lo que NO se ha probado (Regla 6)

- **Modelos reales con `--sandbox`.** Solo comandos directos y un servidor [OI]
  falso. No se midió cuántos parches de SWE-bench fallan por red bloqueada.
- **Ofuscación de la guarda shell.** Declarada como límite; no se intentó
  evadirla de forma exhaustiva.
- **Allowlist con un agente real.** La lista por defecto es mínima; no se midió
  cuántas tareas requieren ampliarla.
- **macOS/Windows.** El sandbox solo existe en Linux/Unix.
