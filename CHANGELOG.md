# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Diagnóstico del sandbox (2026-11-28)**: `lai doctor` y `lai status` informan bwrap, preflight, modo (opt-in/obligatorio) y si la red está permitida. Con `LAI_REQUIRE_SANDBOX=1` y sin sandbox, `doctor` lo marca como error.
- **Red opcional dentro del sandbox**: `LAI_SANDBOX_ALLOW_NET=1` o `"sandbox_network": true` añade `--share-net` manteniendo el aislamiento de PID/mount/IPC. Por defecto sigue bloqueada.
- **Guarda de manifiestos en `run_command`/`bash`**: rechaza redirecciones (`> Cargo.toml`), heredocs, `rm`, `tee`, `truncate`, `dd of=` y `mv`/`cp` sobre manifiestos, salvo `allow_manifest_overwrite: true`. Mejor esfuerzo; la ofuscación puede saltarla y el sandbox es el control fuerte.
- **Allowlist de comandos opt-in en `auto`**: `LAI_COMMAND_ALLOWLIST=1` enruta `run_command`/`bash`/`run_python` por el `PolicyEngine`; `LAI_ALLOW_COMMANDS=make,sed` amplía la lista y `LAI_PROTECTED_PATHS` las rutas protegidas. Por defecto desactivada.
- **DOCS/PLAN_MEJORAS_2026-11-28.md** y **DOCS/CAMBIOS_2026-11-28_MEJORAS2.md** (Regla 10).
- **Sandbox bubblewrap en el CLI Rust (2026-11-28)**: portado de la rama `feat/fase-0-1-harness` según `DOCS/ADR-0001-rama-fase-0-1-harness.md`. `run_command`, `bash` y `run_python` aceptan `"sandbox": true` (workspace rw, `/tmp`, sin red, `--cap-drop ALL`). `lai --sandbox` / `LAI_REQUIRE_SANDBOX=1` lo hacen obligatorio y **fail-closed**: si bwrap no está disponible, la llamada falla en vez de ejecutar sin aislamiento. Preflight con sonda real de namespaces (detecta restricciones AppArmor). 5 tests de sandbox.
- **Capa de contratos para verificadores externos** (`lai_rust/src/contracts.rs`): `ExternalVerdict` (safe/unsafe/timeout/unknown) mapeado al `VerificationVerdict` acotado y `AdmissionGate` para exigir medición antes de adoptar una herramienta. Portado y adaptado; todavía no conectado a un verificador real. 8 tests.
- **`apply_patch` pasa por la guarda de manifiestos**: rechaza parches que eliminen ≥75 % de un `Cargo.toml`, `package.json`, etc. o que lo borren, salvo `allow_manifest_overwrite: true`. Cierra el límite declarado en `CAMBIOS_2026-11-28.md`. 4 tests.
- **DOCS/ADR-0001-rama-fase-0-1-harness.md**: decisión de rescatar sandbox y contratos y archivar el resto de la rama (13 commits, 10.210 líneas).

### Changed
- **CI local cubre Rust (2026-11-28)**: `scripts/ci_local.sh` ejecuta `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` y `cargo test` como pasos obligatorios; en modo `--ci` la ausencia de `cargo` falla. `make check` incluye `rust-check`; nuevos targets `rust-check` y `gateway-check` (`go build` opcional).
- **`AGENTS.md` deja de afirmar un sandbox inexistente**: el CLI Rust ejecuta `run_command`/`run_python` sin aislamiento; la nota ahora lo dice y remite a `SECURITY.md`.
- **`README.md` etiqueta las secciones del CLI Python legado**: «Tool Registry (50+ tools)» y «AutoMode Intelligence» ya no se atribuyen al Rust, que tiene 15 herramientas y un bucle de herramientas sin Plan/ReAct/Reflection.
- **`DOCS/README.md` al día**: índice completo de registros 27–28, arquitectura con Rust como implementación principal y nota sobre `-ngl` según VRAM.

### Fixed
- **Repositorio sin artefactos de build**: se retiraron del índice 345 archivos de `lai_rust/target/` (82 MB) y 6 JSON de `sessions/`, que ya estaban en `.gitignore` pero se habían añadido antes. Los archivos siguen en disco. El índice pasa de ~85 MB a ~3 MB.

### Added
- **Guarda de manifiestos en `write_file`/`edit` (2026-11-28)**: reemplazar entero un `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `Gemfile`, etc. se rechaza salvo `allow_manifest_overwrite: true`; el error recomienda `edit` para un cambio puntual. Cierra el hallazgo del benchmark del 2026-11-26 en el que el agente sustituyó `Cargo.toml` por contenido inventado. 5 tests.
- **Los enunciados del benchmark declaran las restricciones**: `run_benchmark.sh`, `run_competition.sh`, `run_stress.sh` y `swebench/run_eval.py` piden no eliminar ni reemplazar APIs públicas existentes ni sustituir `Cargo.toml`. Cierra el segundo hallazgo del benchmark (una firma pública cambiada rompió un test preexistente).
- **DOCS/CAMBIOS_2026-11-28.md**: registro antes/después (Regla 10) de las reparaciones y pendientes cerrados de esta tanda.
- **Corrida SWE-bench con llama.cpp en GPU (2026-11-27)**: 10 instancias, 8 parches, **2 resueltas** (`pytest-dev__pytest-5809`, `matplotlib__matplotlib-13989`), 0 errores de infraestructura. Tasa de extremo a extremo 2/10 (20 %). Tabla completa en `BENCHMARKS.md`; registro antes/después en `DOCS/CAMBIOS_2026-11-27_SWEBENCH.md`.
- **DOCS/CAMBIOS_2026-11-27_SWEBENCH.md**: registro antes/después (Regla 10) de la puesta en marcha de llama.cpp, la revisión de la aplicación y las correcciones que destapó el benchmark.
- **`lai auto`: escalada por presupuesto de turnos**: si pasa la mitad del presupuesto sin ninguna edición, se le indica al modelo cuántos turnos quedan y se le pide aplicar el arreglo. Medido: los parches producidos pasaron de 5/10 a 8/10.
- **`LlmClient::with_temperature` + `LAI_TEMPERATURE`**: `auto` usa 0.2 por defecto en lugar de 0.7 fijo.
- **DOCS/CAMBIOS_2026-11-27_MEJORAS.md**: registro antes/después (Regla 10) de la tanda de eficiencia: perfil `bash`, presupuesto de contexto, `edit`, comandos reales y deadlock de telemetría.
- **Perfil de herramientas `bash`** (`lai auto --profile bash`): expone una sola herramienta y un prompt de ~800 caracteres en lugar de 14 esquemas. Inspirado en mini-SWE-agent (cifra externa, no reproducida por lai): un agente bash-only es competitivo y arranca más rápido. `--profile full` sigue siendo el valor por defecto. Medido contra un servidor [OI] falso: `tools: 1` frente a 14.
- **`context_budget.rs`**: presupuesto de contexto para modelos pequeños. Antes de cada llamada de `auto` se eliden las salidas de herramientas antiguas (conservando sistema, tarea y los 4 turnos recientes), sin borrar mensajes `tool` para no romper el protocolo de function calling. Ventana configurable con `LAI_CONTEXT_WINDOW` (por defecto 32768). 6 tests.
- **Herramienta `edit`** (search/replace): modifica un archivo reemplazando un fragmento exacto, con `replace_all` opcional. Falla en voz alta si el fragmento no aparece, es ambiguo o es idéntico al nuevo, y pasa por el `PolicyEngine`. 5 tests.
- **`lai auto --output <dir>`** escribe `lai-auto-<fecha>.md` con la tarea, cada llamada y su resultado (antes el flag solo imprimía una línea).
- **DOCS/CAMBIOS_2026-11-27.md**: registro antes/después (Regla 10) de la revisión, pruebas y reparación del cliente LLM, las herramientas, la política, el servidor MCP y el lint.
- **Resultados finales del piloto SWE-bench (5 instancias, 2026-11-26)**: 1 patched / 4 timeout; única evaluable `django__django-12209` → no resuelta (0/1). Contexto medido 32768, hardware i7-10510U 15.8 GB sin GPU. 4 timeouts causados por `apply_patch` devolviendo `Ok` al fallar (bug ya corregido en 56d49c2, el binario del piloto no lo incluía). Tabla completa en `BENCHMARKS.md` con contexto real y honestidad explícita.
- **Dependencias muertas eliminadas** (`lai_rust/Cargo.toml`): 27 crates declarados y nunca importados (tree-sitter y sus 7 gramáticas, `blake3`, `indicatif`, `colored`, `dashmap`, `itertools`, `petgraph`, `serde_yaml`, `syn`/`quote`/`proc-macro2`, `nix`, `tokio-tungstenite`, `tungstenite`, `rustls-pemfile`, `webpki-roots`, `mime_guess`, `async-stream`, `tracing-subscriber`, `base64`, `rand`). `cargo check --all-targets` ya no emite avisos `unused dependency`; el árbol de dependencias y el tiempo de compilación se reducen sin tocar código.
- **Guarda anti-destrucción en `write_file`** (`lai_rust/src/tools.rs`): si el archivo existe, tiene 40+ líneas y el contenido nuevo es menos del 25 % del original, la escritura se rechaza con un error que recomienda `apply_patch` o `allow_shrink: true`. Motivada por un caso real: el agente sustituyó las 1911 líneas de `django/db/models/base.py` por las 18 del ejemplo del issue. 5 tests Rust nuevos.
- **Guía de herramientas actualizada** (`commands.rs`): `apply_patch` pasa a ser la vía recomendada para modificar archivos existentes; se avisa de que el código de ejemplo del issue no es la solución.
- **swebench/summarize.py + test_summarize.py**: cruza los metadatos del agente con el informe de evaluación y emite la tabla markdown para `BENCHMARKS.md`. Lo no medido aparece como `—`; sin evaluación avisa en vez de mostrar un cero. Admite `--run-id` y `--md`. 15 tests offline.
- **swebench/run_pilot.sh**: piloto de 5 instancias, una por parser oficial, con `--keep-going` y `--force`; encadena `evaluate.py` y `summarize.py` al terminar (`SKIP_EVAL=1` para saltarlo, `RUN_ID=...` para fijar el informe).
- **Contexto medido en los metadatos**: `run_eval.py` guarda `context_length` leído de `/api/ps` después de cada corrida (nunca un valor supuesto).
- **swebench/docker_env.py + test_docker_env.py**: diagnóstico compartido del motor Docker. Devuelve la causa real del fallo (binario ausente, motor colgado, versión de servidor antigua) y pistas accionables, en lugar de un mensaje genérico tras 30 s de espera. 15 tests offline.
- **swebench/make_oracle_predictions.py**: genera predicciones con el parche oficial del dataset para validar `evaluate.py` sin ejecutar el agente. Si el evaluador es correcto, todas salen `resolved`.
- **swebench/fetch_dataset.py `--via {auto,api,datasets}`**: descarga del dataset por la API pública de HuggingFace con sólo la stdlib (`urllib`, páginas de 100 filas). Ya no es obligatorio `pip install datasets`. Verificado: 500 instancias de `SWE-bench_Verified` con `image`, `eval_script`, `log_parser` y `eval_type`.
- **DOCS/CAMBIOS_2026-11-26_DOCKER.md**: registro antes/después (Regla 10) del desbloqueo de Docker, la descarga del dataset y los arreglos del script de compilación.
- **DOCS/CAMBIOS_2026-11-26_MEJORAS2.md**: registro antes/después (Regla 10) de la tanda de mejoras y pruebas: guarda local, parser de tool calls, formato/lint, rutas configurables, scripts y mediciones.
- **swebench/test_prepare.py, test_run_eval.py, test_harness_offline.py**: 60 tests offline adicionales (75 en total con `test_evaluate.py`) que cubren preparación de imágenes, clasificación de resultados, preflight de Ollama, normalización del dataset, escritura atómica, wrapper del `eval_script` y graduación. No requieren Docker, HuggingFace ni red.
- **DOCS/CAMBIOS_2026-11-26_PENDIENTES.md**: registro antes/después (Regla 10) de la tanda de trabajo sin Docker.
- **swebench/evaluate.py**: etapa 4 del harness — evaluación oficial. Aplica el parche del modelo, ejecuta el `eval_script` oficial de cada instancia, parsea los logs con el parser oficial (pytest, pytest_v2, pytest_options, django, seaborn, sympy, matplotlib) y calcula `FAIL_TO_PASS`/`PASS_TO_PASS` y *resolved* con la semántica oficial. Separa fallos de infraestructura, escribe `report.json`/`report.md` y logs por instancia. Reanudable vía `data/state.json`.
- **swebench/test_evaluate.py**: 15 tests offline (sin Docker) de parsers, graduación, marcadores, inyección del código de salida y saneado de `run_id`.
- **swebench/**: harness incremental para SWE-bench Verified (descarga del dataset, preparación de imágenes Docker reanudable, compilación de un binario compatible con glibc 2.35, ejecución del agente en contenedor y captura de parches en formato oficial). Validado de punta a punta con `astropy__astropy-12907` en la etapa de ejecución.
- **DOCS/INFERENCIA_LOCAL_BAJA_GAMA.md**: guía de inferencia local en CPU para equipos de 8–16 GB sin GPU (ancho de banda, cuantización para código, decodificación especulativa, caché KV, ternarios, MoE, bucle de verificación y metodología de medición). Las cifras citadas son investigación externa, **no reproducida por lai**.
- **Regla 11 (Baja gama: medir antes de recomendar):** exige medir una variable a la vez, calcular el techo de ancho de banda, aplicar la compuerta de contexto completo y marcar las cifras externas como no reproducidas.
- **Regla 0 (Misión Fundacional):** lai existe para impulsar IAs locales, abiertas y soberanas. Open source first, sin dependencia de claves/API keys, sin dependencia de cuentas en la nube, software gratuito y de calidad, privacidad por diseño. Cualquier feature que introduzca dependencia obligatoria de cloud requiere aprobación explícita.
- **Regla 10 (Documentación obligatoria de cambios):** Nueva regla que exige crear documento de cambios propuestos antes de modificar código, documentar estado ANTES y DESPUÉS de cada cambio individual, y mantener trazabilidad para favorecer el trabajo en equipo. Prohíbe cambios silenciosos o en batch sin documentación intermedia.

### Fixed
- **`lai tui` sin TTY daba un error de sistema poco amable y podía dejar la terminal en modo raw**: ahora comprueba que stdin y stdout son terminales y explica cómo usar `lai chat`/`lai auto`/`lai mcp`; además restaura la terminal aunque el bucle de eventos falle. Verificado con PTY: sale con código 0 y emite `LeaveAlternateScreen` + `show_cursor`.
- **`ruff check .` fallaba con `PLC0207`** en `swebench/run_eval.py` (regresión del commit `9f06f7a`): `split(":", maxsplit=1)`.
- **Configuración de Ruff migrada a `[tool.ruff.lint]`**: `select`, `ignore` y `fixable` ya no están en la sección obsoleta; `ruff check .` deja de emitir el aviso de configuración.
- **`lai bench` presentaba resultados mock como éxito**: `bench all` y `bench run` usaban `MockBenchmarkAdapter` y mostraban «✅ Success: true … 100 %» con latencia 0 ms. Ahora avisan de que no miden nada y cada línea lleva `⚠ MOCK` (Regla 6).
- **`edit` podía destruir un archivo**: un `old_string` igual a casi todo el archivo dejaba 5 líneas de 71 (medido en `pytest-5809`). Ahora aplica la misma guarda anti-destrucción que `write_file`.
- **`grep`/`find_files` con una ruta inexistente devolvían «No matches»**, así que el modelo probaba rutas inventadas hasta agotar las iteraciones. Ahora devuelven un error explícito.
- **El bucle de `auto` giraba hasta el final repitiendo la misma llamada**: en `pytest-10081` fueron 9 repeticiones del mismo `read_file`. Ahora se detiene con un mensaje claro.
- **`auto` empujaba al modelo a seguir trabajando después de editar**, repitiendo `write_file` dos veces. Ahora una respuesta en prosa tras una edición se acepta como final.
- **`swebench/run_eval.py` — la traducción del base-url rompía Linux**: se traducía siempre a `host.docker.internal`, que no resuelve con `--network host` en Linux nativo. Ahora depende de la plataforma y del modo de red, y en bridge se añade `--add-host host.docker.internal:host-gateway`.
- **`swebench/run_eval.py` — `extract_patch` corrompía diffs** que terminaban con una línea de contexto vacía (`.strip()` borraba el espacio): `git apply` respondía `corrupt patch at line 12`. Ahora solo se quitan los saltos de línea del delimitador.
- **`swebench/evaluate.py` — el informe ocultaba los parches vacíos**: la «tasa de resolución» se calculaba solo sobre instancias puntuadas. Ahora muestra también la tasa de extremo a extremo.
- **`swebench/run_eval.py` — el enunciado no indicaba los tests**: el agente leía archivos y nunca comprobaba nada. Ahora incluye los nombres de los `FAIL_TO_PASS` y cómo ejecutarlos (no revela el parche).
- **`telemetry.rs` — deadlock en todos los `record_*`**: el guard del `if let` mantenía el mutex y el segundo `lock()` colgaba el proceso. Medido: `lai verify` no terminaba nunca (código 124 a los 20 s). Ahora un helper `with_telemetry` toma el guard una sola vez. Test de regresión incluido; `lai verify --level 1` termina en 156 ms.
- **`verification_engine.rs` — todo proyecto Python reprobaba la sintaxis**: se ejecutaba `python3 -m py_compile .`, que no acepta directorios (`[Errno 21] Is a directory`). Ahora se usa `compileall -q .`. Además, si `ruff`/`mypy`/`eslint` no están instalados, el nivel pasa como omitido en lugar de fallar.
- **`commands.rs` — siete subcomandos eran stubs**: `repo-map`, `verify`, `diagnose`, `metrics`, `baseline`, `memory` y `checkpoint` imprimían «to be implemented» aunque sus motores existían. Ahora ejecutan el motor real (verificado a mano). `repair` diagnostica el `--error` y `plan` explica que necesita modelo local.
- **`baseline.rs` — `lai baseline create` fallaba con «key must be a string»**: `BaselineMetric` derivaba `Serialize`, así que `Custom(String)` se convertía en `{"Custom": "x"}`, que no es una clave JSON válida. Ahora se serializa como cadena (`custom_files`, `custom_lines`…). `show` y `compare` aceptan además el ID corto que imprime `list`.
- **`repomap.rs` — símbolos mutilados y directorios de build recorridos**: la extracción se unificó en `repomap::extract_symbols` (usada también por la herramienta `repo_map`) y el recorrido respeta `is_skipped_dir()`.
- **`lai_rust/llm.rs` — `tool_choice` se enviaba como `null`**: `#[serde(untagged)]` serializaba `None`/`Auto`/`Required` como `null` en vez de `"none"`/`"auto"`/`"required"`. Ahora hay `Serialize`/`Deserialize` manuales, con soporte de `{"type":"function",…}`.
- **`lai_rust/llm.rs` — `content: null` rompía la respuesta**: cuando el turno solo llevaba `tool_calls`, el servidor devuelve `"content": null` y fallaba la deserialización completa. Se lee como cadena vacía. `ToolCall.id`/`call_type` pasan a opcionales y `function.arguments` acepta objeto o string.
- **`lai_rust/llm.rs` — redirecciones saltaban la Regla 4**: `reqwest` seguía redirecciones a cualquier host. Ahora la política de redirecciones comprueba `is_local_url()` en cada salto y solo permite destinos remotos con `--allow-remote`.
- **`lai_rust/tools.rs` — `run_command`/`run_python` sin timeout real**: `Command::output()` no tiene límite y el `timeout` del esquema se ignoraba. `run_with_timeout()` mata el grupo de procesos completo (`process_group(0)` + `SIGKILL`), necesario porque `sh -c` deja hijos vivos con los pipes abiertos. Medido: `sleep 5` con `timeout: 1` termina en 1 s.
- **`lai_rust/tools.rs` — `write_file` vaciaba el archivo si faltaba `content`**: el valor por defecto era `""` y la llamada devolvía éxito. Ahora es un error explícito.
- **`lai_rust/tools.rs` — `apply_patch` caía a `"."` con un `path` que no era directorio**, aplicando el parche en el lugar equivocado. Ahora falla con un mensaje claro; el fallback a `patch` también se usa cuando `git` no está instalado.
- **`lai_rust/tools.rs` — `repo_map` mutilaba los símbolos**: cortaba en el byte 4 (`async fn foo` → `fn c fn foo`), no veía `pub fn`/`pub struct`/`enum`/`trait` y recorría `target/` y `node_modules/`. Nueva `symbols_in_source()` y el mismo filtro de directorios que `grep`.
- **`lai_rust/tools.rs` — carrera al borrar el temporal de `run_python`**: se guardaba solo la ruta y el archivo se borraba antes de escribir. Ahora se usa `into_temp_path()`.
- **`lai_rust/policy.rs` — `../fuera.txt` pasaba la frontera del workspace**: para rutas inexistentes no se colapsaba `..` y el prefijo coincidía. Nueva `resolve_for_policy()` (canonicaliza la ruta, o el padre, o colapsa léxicamente).
- **`lai_rust/policy.rs` — rutas protegidas comparadas por subcadena**: `notsrc/main.rs` y `Cargo.toml.bak` se bloqueaban. Ahora se comparan componentes.
- **`lai_rust/policy.rs` — `src/` estaba protegido en cualquier proyecto**: la guarda de las fuentes de lai se aplicaba también al repositorio del usuario, donde `src/` es justo lo que hay que editar. Ahora solo se activa si el workspace es el propio lai (`is_lai_workspace()`), y se añaden `.env`, `.ssh`, `.netrc`, `.npmrc`, `.pypirc` e `id_rsa`/`id_ed25519` como protegidos siempre.
- **`lai_rust/commands.rs` — `auto` no consultaba el `PolicyEngine`**: llamaba la función de la herramienta directamente. Ahora `write_file` y `apply_patch` pasan por `ToolRegistry::call_tool()`. Comprobado: `.git/config` y `../escape.txt` se rechazan, `ok.txt` se escribe.
- **`lai_rust/commands.rs` — el normalizador borraba `allow_shrink` y `timeout`**: la guarda anti-destrucción pedía reintentar con `allow_shrink: true`, el normalizador lo eliminaba y el reintento fallaba igual. También faltaba `python3` en la detección de comandos de shell.
- **`lai_rust/mcp_server.rs` — `resources/*` roto en Linux nativo**: se anunciaban URIs `file:///workspace/…` y se leían de `/workspace`, ruta que solo existe en los contenedores de SWE-bench. Además, `resources/read` comprobaba `starts_with("/workspace")` sin canonicalizar, así que `file:///workspace/../../etc/passwd` se leía. Ahora se resuelve contra el workspace real y se canonicaliza.
- **`lai_rust/services.rs` — binarios del `PATH` dados por ausentes**: `Path::new("llama-server").exists()` es falso aunque esté instalado. Ahora `resolve_executable()` usa `which`.
- **Lint: `make check` no podía pasar**: `ruff check .` daba 466 errores en archivos generados y demos (`lai_rust/calc.py` contiene C++, `test_project/`, `lai_fixed.py`, `fastapi_server.py`, `lai_self_improve.py`), en los scripts de `swebench/` y en un cuerpo muerto de `tests/test_lai.py`. Se excluyeron los archivos que no son producto, se añadieron `per-file-ignores` documentados para `swebench/**` y se corrigieron los errores reales. `ruff check .` → All checks passed.
- **swebench (progreso invisible)**: `run_eval.py`, `evaluate.py` y `prepare.py` activan `line_buffering` en `stdout`. Con la salida redirigida a un archivo, Python la bufferiza por bloques y un piloto de 22 minutos no mostraba ni una línea de progreso; solo aparecían los avisos de `stderr`, que no se bufferiza.
- **`apply_patch` devolvía éxito cuando el parche fallaba**: terminaba con `Ok("Patch failed...")`, así que el bucle imprimía `✓ Done` y el modelo creía haber aplicado el cambio. Medido en el piloto: `qwen2.5-coder:7b` repitió el mismo parche cuatro veces hasta agotar los 1500 s. Ahora devuelve `Err` con el stderr real y una instrucción concreta.
- **Sin detección de bucles en `auto`**: un modelo pequeño puede repetir indefinidamente la misma llamada. Ahora se cuenta la firma `herramienta + argumentos` y a partir de la cuarta repetición no se ejecuta: se le dice al modelo que cambie de estrategia. 2 tests.
- **`apply_patch` ensuciaba el árbol al fallar (Windows)**: `patch -p1` reescribe el archivo con CRLF y deja `.orig`/`.rej` aunque el hunk no encaje. Ahora se hace `git apply --check` antes de aplicar y `patch` solo se usa (con `--dry-run` previo) si el directorio no es un repositorio. Un parche fallido no escribe nada; verificado con test.
- **swebench/evaluate.py — parches que rompen la importación**: si el parser no encontraba resultados y `SUITE_RAN` no coincidía, la instancia quedaba «no puntuable» y desaparecía del denominador, inflando la tasa de resolución. Ahora, si el wrapper capturó el código de salida de los tests, la instancia se puntúa y los `FAIL_TO_PASS` fallan como corresponde. Detectado con un parche real que rompió `django.db.models`.
- **swebench/summarize.py — slug incompatible con run_eval.py**: `qwen2.5-coder:7b` se convertía en `qwen2_5_coder_7b` en vez de `qwen2.5-coder_7b`, así que no encontraba los metadatos. Ahora usa la misma regex y, si el nombre no existe, escanea todos los `*.meta.json` buscando filas de ese modelo.
- **swebench/build_lai_for_swebench.sh**: el contenedor de compilación fallaba siempre por dos motivos acumulados. (a) Hacía `. /cargo/env`, que no existe cuando el caché de rustup viene de otra versión; ahora exporta `CARGO_HOME`/`RUSTUP_HOME`/`PATH` e instala con `--no-modify-path`, comprobando `command -v cargo`. (b) Git Bash entrega rutas `/c/DESA/...` que Docker Desktop no monta, así que `cp -r /src/.` fallaba; ahora se convierten con `cygpath -m` antes del `docker run -v`. (c) `cp -r /src/. /build/` copiaba los 7.2 GB de `lai_rust/target` para borrarlos inmediatamente después, lo que dejaba el build «colgado» más de 19 minutos; ahora se usa `tar --exclude=./target --exclude=./.git`.
- **swebench/evaluate.py**: acepta `FAIL_TO_PASS`/`PASS_TO_PASS` vacíos, que el dataset oficial trae legítimamente (django y sphinx). Antes abortaba con «datos incompletos». Verificado: 5/5 instancias con parche oficial resueltas.
- **swebench/run_eval.py**: el binario se monta en `/tmp/lai_bin/lai` y se copia con `chmod +x` dentro del contenedor (en Windows el bit de ejecución no viaja con el bind mount; montar el directorio hacía fallar `cp` con «Not a directory»); rutas de `--mount` con `.as_posix()`; rc 90/91 y el rechazo de la Regla 4 clasificados como error de infraestructura en vez de `no_patch`; el aviso de base-url y el de contexto ya no afirman valores por defecto que no se han medido.
- **swebench/build_lai_for_swebench.sh**: `CARGO_TARGET_DIR=/cargo/target` para que el volumen de caché también guarde los artefactos de compilación (antes cada build recompilaba las dependencias, ~25 min).
- **Regla 4**: `is_local_url()` acepta `host.docker.internal` y `gateway.docker.internal`, necesarios para que `lai` alcance un Ollama local desde dentro de un contenedor. Los alias resuelven a la propia máquina, así que no relajan la misión local. 6 tests.
- **Escrituras de estado concurrentes**: `prepare.py`, `evaluate.py`, `run_eval.py` y `fetch_dataset.py` usaban un `.tmp` de nombre fijo, así que dos procesos a la vez morían con `PermissionError`. Ahora el temporal incluye PID y sufijo aleatorio y se limpia en `finally`.
- **swebench/run_eval.py**: el `--base-url` se pasaba al agente sin traducir. Medido con la imagen de django: dentro del contenedor `http://127.0.0.1:11434` no llega a Ollama ni con `--network host` ni con bridge, mientras que `http://host.docker.internal:11434` funciona en ambos. Se añadió `container_base_url()` (traducción automática) y `container_base_url_warning()` (aviso en el preflight). Cuatro tests nuevos.
- **lai_rust/llm.rs**: la Regla 4 no estaba implementada en el código. `LlmClient` enviaba peticiones a cualquier endpoint y los flags `--allow-remote` de `chat`, `code` y `auto` se ignoraban (parámetros con prefijo `_`). Ahora `is_local_url()` valida loopback, rangos privados, link-local y sufijos `.local`/`.localhost`, `check_endpoint()` aborta antes de la petición y el override requiere `--allow-remote`. Cinco tests nuevos.
- **lai_rust/auto**: el parser de tool calls en bloques ```` ```json ```` fallaba con los modelos pequeños y era la causa real del 0 PASS del benchmark en CPU. `serde_json` rechazaba el JSON porque el `content` traía saltos de línea literales; además solo se aceptaba la clave `"tool"` y `arguments` solo como objeto. Ahora se reparan los caracteres de control dentro de cadenas, se aceptan `tool`/`name`/`tool_name`/`function`, `arguments` como objeto o string JSON, fences sin etiqueta o en mayúsculas, y se ignoran bloques de código que no son JSON. Siete tests nuevos.
- **lai_rust/services.rs**: rutas absolutas de la máquina del autor (`/home/kubuntu/...`). Ahora los binarios se resuelven por entorno: `LAI_LLAMA_SERVER`, `LAI_GATEWAY_BIN`, `LAI_GATEWAY_DIR`, `LAI_OLLAMA_BIN`.
- **lai_rust/main.rs, repair_engine.rs**: avisos de compilación (`base_url`, `quick`, `compare_file`, `bench_file` sin usar; locales muertos en `repair_engine`). `bench-swe` ahora usa e imprime esos parámetros.
- **run_competition.sh, run_stress.sh**: rutas del autor, `jq` obligatorio, `set -e` que abortaba al primer fallo del agente y mojibake. Ahora resuelven el binario desde el repo, degradan sin `jq`, capturan `agent_rc` y omiten de verdad los modelos ausentes.
- **lai_rust/fix_main.py**: eliminado. Era un script que reescribía `main.rs` y no formaba parte del build.
- **scripts/check_docs.py**: el script solo informaba y siempre salía con 0, así que `make check` no detectaba regresiones. Ahora clasifica severidades (`ok`/`info`/`warning`), conserva intactos los documentos históricos, deja de tratar `16384` como error (es la recomendación de SWE-bench) y devuelve código 1 solo ante fallos reales como la opción de formato de razonamiento obsoleta.
- **AGENTS.md**: el árbol de `lai_rust/src/` listaba módulos inexistentes (`automode.rs`, `agents.rs`, `session.rs`, `thinking.rs`, `config.rs`) y la tabla de migración prometía paridad total. Ahora refleja `lib.rs`, los 19 subcomandos y las 13 herramientas reales, y advierte de que el CLI Rust no incluye `serve`/`review`/`fix`/`search`.
- **ESTADO_ACTUAL.md**: describía el CLI en Python como principal y fijaba la versión en 0.0.2. Se añadió un aviso de vigencia y una sección con el estado verificado de `lai_rust/` (0.1.0), incluidas las comprobaciones no ejecutadas (`cargo fmt`/`clippy` no instalados).
- **ROADMAP.md**: presentaba como pendientes un stack `docker-compose` (langfuse, joern, mem0, neo4j, Dafny, TLA+, Marabou…) que no forma parte del lai actual. Se marcó como registro histórico y se redirigió a los pendientes reales.
- **run_benchmark.sh**: contenía rutas absolutas de la máquina del autor (`/home/kubuntu/lai/...`), usaba `jq` de forma obligatoria y `set -e` abortaba el benchmark cuando el agente fallaba. Ahora resuelve el binario desde el propio repo (`LAI_BIN`), degrada sin `jq`, registra `agent_rc` por tarea y no aborta ante un fallo del agente.
- **swebench/run_eval.py**: la clasificación de resultados y la extracción del diff estaban embebidas en `run_instance()` y no se podían probar sin Docker; ahora son funciones puras (`classify_result`, `extract_patch`). Los errores de enlazado dinámico se detectan antes que el código de salida genérico, y `slug()` sanea también puntos iniciales.
- **swebench/evaluate.py**: la validación de campos obligatorios se extrajo a `validate_instances()` para poder probarla sin Docker.
- **swebench/run_eval.py**: `set -e` alrededor de `lai` descartaba el diff cuando el agente fallaba; ahora se captura el código de salida y el diff se emite siempre. Los fallos de infraestructura (imagen ausente, CLI no ejecutable, glibc incompatible, timeout) ya no se escriben como predicciones vacías. Preflight solo de las instancias pendientes; nombres de contenedor únicos; limpieza ante timeout; `shlex.quote` en el comando interno.
- **swebench/prepare.py**: un timeout de `docker pull` dejaba la instancia en `pulling`; ahora se registra como `error`. Usa la imagen del dataset, limpia errores antiguos y admite `--platform` y `--min-free-gb`.
- **swebench/fetch_dataset.py**: el dataset por defecto era `princeton-nlp/SWE-bench_Verified`, que no incluye `image`/`eval_script`/`log_parser`/`eval_type`. Ahora usa `SWE-bench/SWE-bench_Verified`, escribe de forma atómica y avisa si los datos existentes no sirven para evaluar.
- **lai_rust/tools.rs**: `find_files` hacía `unwrap()` sobre el glob y entraba en *panic* con un patrón inválido; ahora devuelve un error. `grep` era una comparación literal que recorría `.git`/`target`/`node_modules`; ahora soporta regex, `case_insensitive` y `glob`, omite ruido y binarios, y ordena la salida. `read_file` leía archivos completos; ahora acepta rangos. `list_dir` ordena. `apply_patch` respeta `path` y usa `git apply` con *fallback* a `patch -p1`.
- **lai_rust/auto**: el contexto del proyecto se inyectaba sin límite; ahora se acota a 8 000 caracteres (cabeza+cola) para no desplazar la tarea.
- **lai_rust/auto**: el resultado de cada herramienta se trunca a 6 000 caracteres (cabeza y cola con marcador). Leer un archivo grande desbordaba el contexto del modelo y le hacía olvidar la tarea.
- **lai_rust/auto**: el *nudge* reinyecta ahora la tarea completa y ordena actuar de forma directiva; antes el modelo respondía "no hace falta herramienta" y terminaba sin cambios.
- **lai_rust/Cargo.toml**: `reqwest` con `default-features = false` (solo rustls). Elimina la dependencia de OpenSSL, que rompía la compilación en contenedores limpios.
- **lai_rust/auto**: tool calling arreglado para modelos no optimizados. `lai auto` usa ahora function calling nativo ([OI] `tools`/`tool_choice`) con fallback al protocolo de bloques JSON, y añade un *nudge* cuando el modelo responde en prosa sin llamar herramientas. `qwen3:8b` pasa de 0/3 a 3/3 tareas en el benchmark rápido.
- **lai_rust/auto**: los argumentos con `\n` literales (JSON doble-escapado) se desescapan antes de escribir el archivo; antes el fichero quedaba en una sola línea.
- **lai_rust/context.rs**: El análisis profundo de proyecto ahora completa realmente la estructura de código (módulos, funciones, tipos, imports y entry points); antes `modules` y `patterns` quedaban siempre vacíos.
- **lai_rust/context.rs**: El walker ahora poda `target/`, `node_modules/`, `.git/`, cachés y directorios ocultos, y omite archivos binarios, no-UTF8 o mayores a 512 KB en lugar de fallar o degradar el rendimiento.
- **lai_rust/context.rs**: Detección de entry points ignora código de ejemplo dentro de strings raw/triple-quoted.
- **lai_rust/context.rs**: Los objetivos se extraen solo de secciones de Goals/Objectives/Mission, se deduplican, se limitan a 20 y se limpian de Markdown.

### Improved
- **Benchmarks (CPU sin GPU)**: dos corridas propias en un equipo sin GPU dedicada (`qwen2.5-coder:3b`, i7-10510U, 15.8 GB RAM). La primera (870.5 s) se midió con el parser de tool calls defectuoso; la segunda, con el parser corregido, dio 0 PASS / 1 PARTIAL / 2 FAIL en 1855.5 s y reveló que el modelo reescribe código que funcionaba y sobrescribe `Cargo.toml`. Ambas se registran como mediciones puntuales, no como conclusiones (Regla 11). Detalle en `BENCHMARKS.md`.
- **Medición de contexto**: `scripts/measure_context_throughput.sh` compara dos instancias de Ollama con distinto `OLLAMA_CONTEXT_LENGTH` sobre el mismo prompt. Resultado medido: 7.11 tok/s con 32768 y 7.07 tok/s con 16384, diferencia indistinguible del ruido. Se corrigió la afirmación previa de que el contexto cargado era 4096: `/api/ps` muestra 32768.
- **Benchmarks**: `run_benchmark.sh`, `run_competition.sh` y `run_stress.sh` verifican de forma honesta (hash de archivo antes/después + tests obligatorios), reportan `PARTIAL`/`TIMEOUT`, soportan límite de tareas, timeout por tarea y ejecución en paralelo. Resultados en `BENCHMARKS.md`.
- **lai_rust/context.rs**: Detección de stack en monorepos (manifiestos anidados como `lai_rust/Cargo.toml` o `gateway-go/go.mod`) y clasificación por lenguaje dominante en lugar de solo el manifiesto raíz.
- **lai_rust/context.rs**: El contexto formateado incluye excerpts de README, AGENTS.md y manifiestos, con un límite de 16 000 caracteres para proteger la ventana del modelo.
- **lai_rust/context.rs**: Análisis de la raíz del repo lai reducido de ~828 ms a ~15 ms y con 9 tests unitarios nuevos.

## [v0.0.2] - 2026-11-20

### Fixed
- **lai.py**: Fixed duplicate `formatter_class` in ArgumentParser
- **lai.py**: Fixed duplicate `chat` subparser definition
- **lai.py**: Fixed `max-tokens` argument conflict with `-m` flag (model)
- **lai.py**: Fixed duplicate `epilog` and `formatter_class` in ArgumentParser
- **lai.py**: Fixed `run_script` method missing `self` parameter
- **lai.py**: Fixed `run_command` method missing `self` parameter
- **lai.py**: Fixed `grep` method signature mismatch
- **lai.py**: Fixed `run_script` method missing `self` parameter
- **lai.py**: Fixed `chat` method missing `self` parameter in async wrapper
- **lai.py**: Fixed thinking mode not properly extracting reasoning
- **lai.py**: Fixed auto mode not using tools properly
- **lai.py**: Fixed code generation not using auto mode properly
- **lai.py**: Fixed missing `--max-tokens` argument conflict with `-m` flag
- **gateway-go**: Fixed circuit breaker state transitions
- **gateway-go**: Fixed weighted load balancing with circuit breaker awareness
- **gateway-go**: Fixed retry with exponential backoff in tryFallback
- **gateway-go**: Fixed semantic cache cosine similarity threshold
- **gateway-go**: Fixed circuit breaker integration in handleChat
- **gateway-go**: Fixed selectUpstream with weighted scoring
- **gateway-go**: Fixed gateway compilation errors (err redeclaration, unused variables)
- **tui_app.py**: Fixed curses wrapper for TTY detection
- **tui_app.py**: Fixed curses initialization and cleanup
- **tui_app.py**: Fixed input handling for arrow keys
- **racing.py**: Fixed player position (now at bottom of screen)
- **racing.py**: Fixed enemy collision detection
- **racing.py**: Fixed game loop timing
- **tui_app.py**: Fixed curses wrapper for TTY detection
- **tui_app.py**: Fixed input handling for arrow keys
- **racing.py**: Fixed player position at bottom of screen
- **project_generator_tui.py**: Fixed duplicate menu_key fields
- **project_generator_tui.py**: Fixed missing main function
- **project_generator_tui.py**: Fixed menu navigation logic

### Improved
- **lai.py**: Enhanced chat command with streaming support
- **lai.py**: Enhanced auto mode with proper tool integration
- **lai.py**: Improved code generation with auto/yolo modes
- **lai.py**: Enhanced thinking mode with proper extraction
- **lai.py**: Improved auto-start services on startup
- **lai.py**: Enhanced status command with detailed service info
- **lai.py**: Improved error handling and logging
- **gateway-go**: Enhanced circuit breaker with proper state transitions
- **gateway-go**: Improved weighted load balancing with circuit awareness
- **gateway-go**: Enhanced semantic cache with cosine similarity
- **gateway-go**: Improved retry logic with exponential backoff
- **gateway-go**: Added circuit breaker metrics
- **gateway-go**: Enhanced context checkpoints for long contexts
- **harness.py**: Improved auto mode with proper tool integration
- **harness.py**: Enhanced thinking extraction with multiple patterns
- **harness.py**: Improved context compression preserving code blocks
- **harness.py**: Enhanced tool registry with OpenAI schemas
- **racing.py**: Fixed player position at bottom of screen
- **racing.py**: Fixed enemy collision detection
- **racing.py**: Fixed game loop timing
- **tui_app.py**: Fixed curses wrapper for TTY detection
- **tui_app.py**: Fixed input handling for arrow keys
- **racing.py**: Fixed player position at bottom of screen
- **project_generator_tui.py**: Fixed duplicate menu_key fields
- **project_generator_tui.py**: Fixed missing main function
- **project_generator_tui.py**: Fixed menu navigation logic

### Documentation
- Updated CHANGELOG.md with v0.0.2 changes
- Updated VERSION to v0.0.2
- Updated README.md with latest features
- Updated DOCUMENTACION_GATEWAY_LOCAL.md with latest improvements
- Updated MEJORAS_IMPLEMENTADAS.md with detailed changes
- Updated DOCUMENTACION_FINAL.md with current state
- Updated DOCUMENTACION_REGLAS.md with corrected reference
- Updated MEJORAS_IMPLEMENTADAS.md with latest improvements

---

## [v0.0.1] - 2026-11-20

### Added
- **llama.cpp server** (port 8080) with Qwen3-8B-Q4_K_M model
  - FP8 KV Cache quantization (-50% VRAM)
  - Context checkpoints for long context support
  - Speculative decoding with ngram (2-3x speedup)
  - Flash Attention + Continuous Batching
  - 71 tok/s on RTX 3070 8GB
- **Gateway Go v3.1** (port 8081)
  - Circuit Breaker pattern (Closed/Open/HalfOpen)
  - Weighted Load Balancing
  - Speculative decoding with ngram
  - Flash Attention + Continuous Batching
  - Context checkpoints for long contexts
  - LRU Cache + Semantic Cache
  - Prometheus metrics
  - Circuit Breaker integration in request flow
- **Harness Python** (harness.py)
  - ThinkingManager: extracts <thinking> blocks
  - ContextManager: smart context compression
  - ToolRegistry: 6 built-in tools (read_file, write_file, list_dir, run_command, grep, python_eval)
  - AutoMode: autonomous task execution
  - Session management
- **lai CLI** (lai.py)
  - Commands: chat, generate, code, think, auto, run, serve, status, models, tools
  - Options: --stream, --thinking, --auto, --yolo
  - Auto-start services (llama.cpp, gateway, ollama)
- **chess.py** - Chess game generated by Qwen3-8B
  - Full chess engine: moves, check/mate, en passant, promotion, castling
  - Unicode board rendering, algebraic notation
- **racing.py** - Terminal racing game
  - Player movement, enemies, collision, levels
  - Terminal rendering with ANSI
- **tui_app.py** - TUI application with curses
  - File explorer, text editor, system monitor
- **project_generator_tui.py** - Project generator TUI
  - Multi-step wizard for project generation
  - Supports multiple project types
- **Gateway Go v3.1** (gateway-go/)
  - Circuit Breaker pattern (Closed/Open/HalfOpen)
  - Weighted Load Balancing
  - Speculative decoding with ngram
  - Flash Attention + Continuous Batching
  - Context checkpoints for long contexts
  - LRU Cache + Semantic Cache
  - Prometheus metrics
  - Circuit Breaker integration in request flow
- **Gateway Python** (gateway.py v2.1)
  - LRU Cache + Semantic Cache
  - Context compression with code preservation
  - Noise filtering (Kimi/Crush/OpenCode)
  - Prometheus metrics
- **Harness Python** (harness.py)
  - ToolRegistry with 6 built-in tools
  - AutoMode for autonomous execution
  - ThinkingManager for reasoning extraction
  - Session management

### Infrastructure
- llama.cpp server (port 8080) - Qwen3-8B-Q4_K_M
  - FP8 KV Cache, Context Checkpoints, Speculative Decoding
  - Flash Attention, Continuous Batching
  - 71 tok/s on RTX 3070 8GB
- Gateway Go v3.1 (port 8081)
  - Circuit Breaker, Load Balancing, Semantic Cache
  - Prometheus metrics
- Ollama (port 11434) - Fallback
- Gateway Python v2.1 (fallback)

### Documentation
- README.md (root) - Complete project documentation
- chess/README.md - Chess game documentation
- DOCUMENTACION_GATEWAY_LOCAL.md - Gateway documentation
- MEJORAS_IMPLEMENTADAS.md - Improvements log
- DOCUMENTACION_FINAL.md - Final documentation
- OPTIMIZACION_PARAMETROS.md - Optimization parameters
- DOCUMENTACION_REGLAS.md - Project rules
- MEJORAS_IMPLEMENTADAS.md - Improvements log
- VERSION file (v0.0.1)
- CHANGELOG.md

### Rules Compliance
- ✅ Rule 1: thinker rules
- ✅ Rule 2: better-ai rules
- ✅ Rule 3: better-project rules
- ✅ Rule 4: All code generated by local LLMs (Qwen3-8B via llama.cpp)
- ✅ Rule 5: Cloud only for config/scripts
- ✅ Rule 6: Honesty 100%
- ✅ Rule 7: Accuracy 99%
- ✅ Rule 8: Chess generated with local LLMs and harness

---

## [v0.0.1] - 2026-11-20

### Added
- **llama.cpp server** (port 8080) with Qwen3-8B-Q4_K_M model
  - FP8 KV Cache quantization (-50% VRAM)
  - Context checkpoints for long context support
  - Speculative decoding with ngram (2-3x speedup)
  - Flash Attention + Continuous Batching
  - 71 tok/s on RTX 3070 8GB
- **Gateway local** (port 8081) - Proxy with filtering and compression
- **Ollama** (port 11434) - qwen3:8b model
- **Crush** - Client configured for local gateway
- **Harness Python** - Session management, thinking, tools
- **chess.py** - Chess game with Unicode pieces
- **racing.py** - Racing game with terminal UI
- **tui_app.py** - TUI app with curses
- **project_generator_tui.py** - Project generator TUI
- **Gateway Go v3.1** - Circuit breaker, load balancing, semantic cache
- **Gateway Python** - LRU cache, semantic compression
- **Harness Python** - Session, thinking, tools, auto-mode
- **lai CLI** - Complete CLI with chat, generate, code, think, auto, run, serve, status, models, tools
- **chess.py** - Chess game generated by Qwen3-8B
- **racing.py** - Racing game generated by Qwen3-8B
- **tui_app.py** - TUI app generated by Qwen3-8B
- **project_generator_tui.py** - Project generator TUI
- **Gateway Go v3.1** - Circuit breaker, load balancing, semantic cache
- **Gateway Python v2.1** - Filtering, compression, semantic cache
- **Harness Python** - Sessions, thinking, tools, auto-mode
- **OpenAPI 3.1 Spec** - Complete spec for gateway

### Infrastructure
- llama.cpp server (port 8080) - Qwen3-8B-Q4_K_M
  - FP8 KV Cache, Context Checkpoints, Speculative Decoding
  - Flash Attention, Continuous Batching
  - 71 tok/s on RTX 3070 8GB
- Gateway Go v3.1 (port 8081)
  - Circuit Breaker, Load Balancing, Semantic Cache
  - Prometheus metrics
- Ollama (port 11434) - Fallback
- Gateway Python v2.1 (fallback)

### Documentation
- README.md (root) - Complete project documentation
- chess/README.md - Chess game documentation
- DOCUMENTACION_GATEWAY_LOCAL.md - Gateway documentation
- MEJORAS_IMPLEMENTADAS.md - Improvements log
- DOCUMENTACION_FINAL.md - Final documentation
- OPTIMIZACION_PARAMETROS.md - Optimization parameters
- DOCUMENTACION_REGLAS.md - Project rules
- MEJORAS_IMPLEMENTADAS.md - Improvements log
- VERSION file (v0.0.1)
- CHANGELOG.md

### Rules Compliance
- ✅ Rule 1: thinker rules
- ✅ Rule 2: better-ai rules
- ✅ Rule 3: better-project rules
- ✅ Rule 4: All code generated by local LLMs (Qwen3-8B via llama.cpp)
- ✅ Rule 5: Cloud only for config/scripts
- ✅ Rule 6: Honesty 100%
- ✅ Rule 7: Accuracy 99%
- ✅ Rule 8: Chess generated with local LLMs and harness