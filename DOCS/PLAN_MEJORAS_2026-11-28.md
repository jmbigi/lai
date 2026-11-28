# Plan de mejoras 2026-11-28 (post-sandbox)

Estado de partida: `master` en `62587b1`. 180 tests Rust, 241 Python, CI local con
Rust. Sandbox opt-in/fail-closed y guarda de manifiestos en `write_file`, `edit`
y `apply_patch`.

Criterio de este plan: **solo mejoras seguras, verificables en este equipo y sin
dependencias externas nuevas**. Nada que prometa aislamiento o capacidad que no
se pueda probar (Regla 6). Orden por riesgo creciente.

## Lote A — Diagnóstico del sandbox

| # | Mejora | Por qué | Verificación |
|---|---|---|---|
| A1 | `lai doctor` informa bwrap, preflight real y si el sandbox es obligatorio | Hoy el usuario no sabe si `--sandbox` va a funcionar hasta que falla | Salida con bwrap y con `PATH` sin bwrap |
| A2 | `lai status` muestra una línea de sandbox | Diagnóstico rápido sin `doctor` | Salida con/sin bwrap |
| A3 | `lai doctor` avisa si `LAI_REQUIRE_SANDBOX=1` y el sandbox no está disponible | Evita fallos diferidos en `auto` | Test unitario de la lógica de aviso |

## Lote B — Sandbox utilizable en tareas reales

| # | Mejora | Por qué | Verificación |
|---|---|---|---|
| B1 | `LAI_SANDBOX_ALLOW_NET=1` (o `sandbox_network: true`) permite red dentro del sandbox | Muchas tareas necesitan descargar dependencias; hoy fallarían sin explicación | Comando con red dentro/ fuera del sandbox |
| B2 | `lai doctor` documenta la variable y el modo | Descubribilidad | Salida |
| B3 | Tests de `run_python` bajo sandbox | La ruta de Python no estaba cubierta E2E | `cargo test` |

## Lote C — Guarda de manifiestos en el perfil `bash`

| # | Mejora | Por qué | Verificación |
|---|---|---|---|
| C1 | `run_command`/`bash` rechazan redirecciones y borrados obvios sobre manifiestos (`> Cargo.toml`, `cat > …`, `tee …`, `rm …`, `mv …`) | Era el límite declarado del lote anterior | Tests con cada patrón |
| C2 | Opt-out `allow_manifest_overwrite: true` | No bloquear un reemplazo deliberado | Test |
| C3 | Los falsos negativos quedan declarados (ofuscación con variables, `python -c`, etc.) | Honestidad (Regla 6) | Documentación |

## Lote D — Allowlist de comandos opt-in en `auto`

| # | Mejora | Por qué | Verificación |
|---|---|---|---|
| D1 | `LAI_COMMAND_ALLOWLIST=1` hace que `auto` pase `run_command`/`bash`/`run_python` por el `PolicyEngine` | Hoy `auto` solo aplica política a escrituras | Test con comando permitido y denegado |
| D2 | `LAI_ALLOW_COMMANDS=cmd1,cmd2` amplía la lista | La allowlist por defecto es mínima | Test |
| D3 | Por defecto sigue desactivada (no rompe benchmarks ni flujos) | Cambio de comportamiento solo opt-in | Tests existentes |

## Lote E — Veracidad documental y cierre

| # | Mejora | Por qué | Verificación |
|---|---|---|---|
| E1 | README: 20 subcomandos (no 19), sandbox y red opt-in | Conteo real | `lai --help` |
| E2 | `SECURITY.md`: sección de sandbox con modos y límites | Coherencia con el código | Lectura |
| E3 | `BENCHMARKS.md`: nota de que el sandbox no se midió con modelos | Regla 6 | Lectura |
| E4 | `ESTADO_ACTUAL.md`, `CHANGELOG.md`, `DOCS/README.md`, `CAMBIOS_2026-11-28_MEJORAS2.md` | Regla 10 | Enlaces |
| E5 | `make check` incluye el chequeo de sandbox si bwrap existe | Evita regresiones silenciosas | `make rust-check` |

## Fuera de alcance (declarado)

- Conectar `contracts.rs` a Dafny/TLA+/α,β-CROWN: requiere instalar herramientas
  externas y medir admisión. No es seguro hacerlo a ciegas.
- Repetir SWE-bench con `--sandbox`: requiere GPU y horas; se hará cuando el
  operador lo pida.
- Windows/macOS: el sandbox es Linux/Unix.
- Cifrado/autenticación del gateway: fuera del objetivo local-first.
