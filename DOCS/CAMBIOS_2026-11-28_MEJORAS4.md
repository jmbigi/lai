# Cambios 2026-11-28 — lote G: caché de prompt, truncación y guarda de credenciales

Registro según **Regla 10**. Continúa `DOCS/CAMBIOS_2026-11-28_MEJORAS3.md`.
Fuentes consultadas (no son mediciones de lai; Regla 6/11):

- llama.cpp — GBNF y gramáticas perezosas para tool calling; `cache_prompt`,
  `--cache-reuse`, `n_keep` y métricas `timings` (documentación y PRs #9639,
  #9866, discusiones #8860/#8947/#15530).
- Análisis de gramáticas GBNF (Atomic Agent, codeloopback): la gramática
  garantiza forma, no semántica; hay que comprobar `finish_reason` y no
  ejecutar una llamada truncada por `max_tokens`.
- mini-SWE-agent / Anthropic / Warp (ya citados en MEJORAS3).

## 1. `cache_prompt` en cada petición

**ANTES:** cada turno del agente reprocesaba el prompt de sistema y los 15
esquemas de herramientas (miles de tokens) porque no se pedía reutilizar el KV
cache.

**DESPUÉS:** `NativeChatRequest` y `ChatRequest` envían `"cache_prompt": true`.
Es la opción documentada de llama.cpp para reutilizar el prefijo común; los
servidores que no la soportan la ignoran. Se puede desactivar con
`LAI_CACHE_PROMPT=0` por si un backend rechaza campos desconocidos.

**Verificación:** test de serialización (se envía `true`; se omite si es
`None`); parseo del flag de entorno.

## 2. `finish_reason == "length"` no se ejecuta

**ANTES:** si el modelo agotaba `max_tokens` a mitad de una tool call, el JSON
llegaba incompleto y se intentaba ejecutar (o se interpretaba como fin de
tarea). Es el fallo que la documentación de gramáticas señala como límite: la
gramática garantiza prefijos válidos, no salidas completas.

**DESPUÉS:** `chat_with_native_tools` devuelve `ChatOutcome { message,
finish_reason }`. Si `finish_reason == "length"`, `auto` **no ejecuta** las tool
calls, avisa y pide al modelo una acción más corta.

**Verificación:** 1 test (`chat_outcome_detects_truncation`); comportamiento
E2E pendiente de modelo real (declarado abajo).

## 3. Guarda de credenciales en `read_file` y `grep`

**ANTES:** el sandbox aísla la ejecución de comandos, pero `read_file` es una
herramienta del proceso lai y leía con los permisos del usuario. Un README con
prompt injection podía pedir `~/.ssh/id_ed25519` y verter la clave al contexto.

**DESPUÉS:** `sensitive_read_violation()` rechaza rutas con componentes
`.ssh`, `.aws`, `.gnupg`, `.azure`, `.kube`, `.docker/config.json`, y archivos
`.netrc`, `.npmrc`, `.pypirc`, `.git-credentials`, `id_rsa`, `id_ed25519`,
`id_ecdsa`, `id_dsa`, `.pem`/`.key` (salvo `.pub`). `read_file` permite el
override explícito `allow_sensitive_read: true`. `grep` rechaza rutas dentro de
esos directorios.

**Verificación:** 4 tests (rutas rechazadas, archivos normales y `.pub`
permitidos, override, `grep`).

## 4. Métricas opcionales de llama.cpp

**ANTES:** no había forma de comprobar si `cache_prompt` estaba funcionando.

**DESPUÉS:** con `LAI_DEBUG_TIMINGS=1`, cada respuesta imprime
`prompt_n`, `cache_n`, `predicted_n` y `predicted_per_second` cuando el servidor
devuelve `timings`. Permite medir una variable a la vez (Regla 11).

## 5. `make sandbox-check`

**ANTES:** las pruebas de aislamiento solo se ejecutaban si alguien recordaba
`cargo test`.

**DESPUÉS:** `make sandbox-check` ejecuta las 19 pruebas de sandbox si `bwrap`
está instalado y avisa si no lo está.

## 6. Herramienta `todo` y prompt de flujo

**ANTES:** el prompt no sugería planificar; en tareas de varios pasos los
modelos pequeños perdían el hilo.

**DESPUÉS**

- Herramienta `todo` (técnica de Warp): `set`/`add`/`done`/`list`/`clear`, máximo
  20 elementos, estado de sesión. Añadida al perfil `full`; el perfil `bash`
  sigue exponiendo solo `bash`.
- El prompt del perfil full explica el flujo: leer → reproducir → cambio mínimo
  → ejecutar el test → reportar el resultado real; y pide no afirmar que un test
  pasó si no se ejecutó.
- 3 tests de la herramienta.

## 7. `start-llama.sh` con flags de agente

**ANTES:** el script arrancaba llama.cpp sin `--jinja` (necesario para que el
servidor use la plantilla de chat del modelo y el tool calling nativo fiable) y
sin `--cache-reuse`.

**DESPUÉS:** detecta el soporte en `llama-server --help` y añade
`--jinja`, `--cache-reuse 256` (`LAI_CACHE_REUSE`) y `-np N`
(`LAI_LLAMA_PARALLEL`). Si el binario no soporta `--jinja`, avisa en lugar de
fallar.

**Verificación:** `bash -n scripts/start-llama.sh`.

## 8. `/tmp` privado en el sandbox

**ANTES:** el sandbox montaba el `/tmp` del host con `--bind`, así que otros
procesos podían leer los temporales del agente (y el agente los del host).

**DESPUÉS:** `--tmpfs /tmp` (privado y vacío). Para `run_python`, el script se
monta como archivo de solo lectura en `/tmp/lai_script.py` con
`build_command_with_binds`. `run_python` se extrajo a `run_python_tool()` para
poder probarlo.

**Verificación:** 3 tests — Python funciona en sandbox y ve su script; un
marcador creado en el `/tmp` del host no es visible dentro del sandbox; y un
workspace situado dentro de `/tmp` sigue funcionando (el `--tmpfs /tmp` va
**antes** del bind del workspace: bwrap aplica los montajes en orden, y al revés
el tmpfs tapaba el proyecto y bwrap fallaba con `Can't chdir`).

## 9. Herramientas `git_*`: fallan en voz alta y pasan por política

**ANTES:** `git_add`/`git_commit`/`git_status`/`git_diff` devolvían solo
`stdout`. Un `git commit` sin nada que commitear devolvía cadena vacía y el
modelo lo interpretaba como éxito (el mismo fallo que tenía `apply_patch`).
Además, en `auto` no pasaban por el `PolicyEngine`.

**DESPUÉS:** `run_git()` comprueba el código de salida y devuelve `Err` con
`stderr`; si no hay salida, lo dice explícitamente. Los cuatro comandos git (y
`git_rollback`) pasan por `call_tool` en `auto` y `check_tool_value` aplica la
frontera del workspace y `check_git`.

**Verificación:** 3 tests (`git status` fuera de un repo falla, commit sin
cambios falla, `git status` en un repo devuelve salida).

## 10. Resumen de consumo en `auto`

**ANTES:** el bucle no informaba de tokens consumidos; era imposible comparar
corridas ni comprobar si `cache_prompt` servía.

**DESPUÉS:** `ChatOutcome` incluye `usage` y `timings`; `auto` acumula
`prompt_tokens`, `completion_tokens` y `cache_n` y, al terminar, imprime
`📊 Consumo: prompt N tokens, completion M tokens[, K reutilizados del KV cache]`.

**Verificación:** E2E con servidor falso (imprime el resumen).

## 11. `contracts.rs` conectado a `lai verify`

**ANTES:** la capa de contratos compilaba y tenía tests, pero ningún comando la
usaba (declarado en ADR-0001).

**DESPUÉS:** `lai verify <path> --external <safe|unsafe|timeout|unknown>
[--required]` mapea el resultado de un verificador externo al veredicto acotado
y sale con código 2 si no es un éxito. Permite que un wrapper de Dafny, TLA+ o
α,β-CROWN alimente a lai sin que la herramienta externa pueda declarar SUCCESS
por sí sola.

**Verificación** (ejecutada):

| Entrada | Veredicto | Código |
|---|---|---|
| `safe --required` | SUCCESS | 0 |
| `unsafe --required` | BLOCKED | 2 |
| `timeout` (opcional) | SUCCESS_WITH_DECLARED_GAPS | 0 |
| `counterexample` (opcional) | FAILED | 2 |

## 12. `lai auto --verify` y arreglo del motor de verificación

**ANTES (defecto real):** `verification_engine::verify_unit_tests` ejecutaba
`cargo test --message-format=json` y buscaba eventos `reason == "test"`. Cargo
**no emite esos eventos** en ese formato (solo mensajes de compilación), así que
el parser siempre obtenía 0 tests y reportaba `"No tests found"` con
`passed = true, score = 100`. Es decir: **los tests unitarios de Rust nunca se
verificaron**. La rama de Python dependía de `pytest-json-report`; si el plugin
no estaba instalado, también reportaba "Tests passed" sin mirar nada.

**DESPUÉS**

- Rust: `cargo test --quiet` y `parse_cargo_test_summary()` lee las líneas
  `test result: ok. N passed; M failed;`; el éxito exige código de salida 0 y
  tasa ≥ umbral.
- Python: `pytest --tb=no -q` y `parse_pytest_summary()` lee el resumen
  (`3 passed, 1 failed`); sin plugin externo. Código 5 (no tests) se distingue.
- Sin tests: `passed = true` pero `score = 0` y mensaje
  `"No tests found (nothing verified)"`, para no romper proyectos sin tests
  pero sin fingir una verificación.
- `lai auto ... --verify`: al terminar ejecuta sintaxis + tests unitarios y
  emite veredicto acotado; **código de salida 2** si no pasa. Si no hay tests,
  avisa explícitamente de que solo se verificó la sintaxis.

**Verificación** (ejecutada con un proyecto Rust temporal y servidor [OI] falso):

| Caso | Resultado | Código |
|---|---|---|
| Proyecto con 1 test que pasa | `1/1 tests passed` | 0 |
| Test roto a propósito | `0/1 tests passed` → Blocked | 2 |
| Proyecto sin tests | aviso "solo sintaxis" | 0 |

Tests nuevos del parser: 4 (`parses_cargo_test_summary`,
`parses_cargo_test_failures`, `parses_pytest_summary`,
`pytest_progress_dots_are_not_counted`).

## 13. Verificación completa

| Comprobación | Resultado |
|---|---|
| `cargo test` | ✅ **230 tests** (188 lib + 42 integración), 0 fallos |
| `cargo fmt --check` | ✅ |
| `cargo clippy --all-targets -- -D warnings` | ✅ |
| `pytest tests swebench` | ✅ 241 passed, 9 skipped |
| `ruff check .` | ✅ |
| `make sandbox-check` | ✅ 19 pruebas de sandbox |
| `make gateway-check` | ✅ |
| `scripts/ci_local.sh --quick --ci` | ✅ 6/6 |

## 14. Lo que NO se ha probado (Regla 6)

- **`cache_prompt` con un modelo real.** No hay servidor llama.cpp en marcha en
  esta sesión; no se midió la mejora de TTFT ni de tokens reprocesados. La
  opción es la documentada por llama.cpp, pero la cifra queda pendiente.
- **Truncación real.** El test cubre la lógica; no se forzó a un modelo a
  truncarse.
- **Guarda de credenciales.** Es una lista de nombres y componentes, no un
  detector de contenido: un secreto en `notes.txt` no se detecta.
- **`auto --verify` con un modelo real.** Se probó con servidor falso; el
  veredicto depende del estado del proyecto, no del modelo.
- **Contratos con una herramienta externa real.** El mapeo y el CLI están
  probados; no se ejecutó Dafny/TLA+/α,β-CROWN en este equipo.
- **Gramáticas GBNF explícitas.** llama.cpp ya genera la gramática desde los
  `tools` nativos; no se envía un `grammar`/`response_format` adicional.
- **`--jinja` / `--cache-reuse`.** No hay servidor llama.cpp en marcha en esta
  sesión: el script los añade si el binario los soporta, pero no se midió su
  efecto.
- **Herramienta `todo` con modelos reales.** Solo tests unitarios; no se midió
  si mejora la tasa de resolución.
