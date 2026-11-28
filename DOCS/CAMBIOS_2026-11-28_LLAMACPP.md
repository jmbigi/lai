# Cambios 2026-11-28 — llama.cpp habilitado en local

Registro según **Regla 10**. Continúa `DOCS/CAMBIOS_2026-11-28_MEJORAS4.md`.

## 0. Punto de partida

| Comprobación | Resultado inicial |
|---|---|
| `llama-server` en `PATH` | ❌ no encontrado |
| Binario disponible | `/home/kubuntu/llama.cpp-bin/llama-b12567/llama-server` (build 12567) |
| `./scripts/start-llama.sh` sin variables | ❌ «llama-server not found» |
| `LAI_NGL=99` (valor por defecto) | ❌ OOM al crear el KV cache |
| Modelo | `/home/kubuntu/models/Qwen3-8B-Q4_K_M.gguf` (4.7 GiB) + un `.gguf` de 0 bytes |
| GPU | RTX 3070 Laptop 8 GB, 15 MiB usados |
| Puerto 8080 | libre |

## 1. El script arranca sin variables de entorno

**ANTES:** había que pasar `LLAMA_SERVER` y `LAI_MODEL_PATH` a mano, y el
`.gguf` de 0 bytes podía confundir la búsqueda.

**DESPUÉS**

- `resolve_llama_server()` busca también en `~/llama.cpp-bin/*/llama-server`,
  `~/llama.cpp/build/bin/llama-server` y `~/ollama-bin/lib/ollama/llama-server`.
- `resolve_model()` busca en `$REPO_ROOT/models` y `$LAI_MODELS_DIR`
  (por defecto `$HOME/models`), **ignora `.gguf` de menos de 100 MB** (descargas
  incompletas) y, si hay varios, elige el mayor.

## 2. Cascada de configuración GPU/contexto

**ANTES:** `-ngl 99` fijo con 32768 de contexto. En 8 GB eso es OOM; el usuario
solo veía «llama.cpp failed to become healthy».

**DESPUÉS:** si el operador no fija `LAI_NGL`/`LAI_CONTEXT`, se prueban
`99:32768 → 99:16384 → 50:18432 → auto:16384 → auto:8192 → 0:4096` y se usa la
primera que arranca. Si los fija, se respeta exactamente (sin sustituciones
silenciosas). `LAI_NGL=auto` omite `-ngl` para que llama.cpp ajuste solo.

**Medido** (detalle en `BENCHMARKS.md`):

| Config | Generación | Prompt | VRAM |
|---|---:|---:|---:|
| `99:16384` | **72.0 t/s** | **642 t/s** | 7071 MiB |
| `50:18432` | 72.4 t/s | 539 t/s | 7361 MiB |
| `auto:18432` | 37.1 t/s | 225 t/s | 6803 MiB |
| `auto:32768` | 15.5 t/s | 102 t/s | 6825 MiB |
| `99:32768`, `50:24576`, `45:32768` | — | — | OOM |

Arranque real sin variables: 5 s, elige `ngl=99 ctx=16384`.

## 3. `lai auto` pregunta la ventana real al servidor

**ANTES:** el presupuesto de contexto suponía 32768 tokens aunque el servidor
tuviera 16384; un prompt más largo se truncaba o fallaba.

**DESPUÉS:** si `LAI_CONTEXT_WINDOW` no está definido, `auto` consulta
`{base_url sin /v1}/props` y usa `default_generation_settings.n_ctx`. Respeta la
Regla 4 (solo consulta endpoints locales salvo `--allow-remote`) y cae al valor
por defecto si el servidor no responde o no expone el campo. Lo imprime:
`Context window: 16384 tokens`.

## 4. Verificación con el modelo real

| Caso | Resultado |
|---|---|
| `lai chat "Responde solo con: HOLA_LLAMA"` | ✅ `HOLA_LLAMA` |
| Tool calling nativo (`write_file`) | ✅ crea el archivo en 1 iteración |
| `cache_prompt` | ✅ turno 2: `prompt_n=64`, `cache_n=5211` (98.8 % reutilizado) |
| `lai status` | ✅ `llama.cpp: Running (Port 8080)` |
| `lai --sandbox auto …` | ✅ escribe el archivo dentro del sandbox |
| Arreglo real + `--verify` | ✅ 1 línea cambiada, 2/2 tests, exit 0 |
| `LAI_DEBUG_TIMINGS=1` | ✅ imprime `prompt_n`, `cache_n`, `gen t/s` |

## 4b. La detección de tool calling ya no es un stub

**ANTES:** `tests/test_lai.py::_check_tool_calling_support` devolvía **siempre
`False`** con el argumento de ser «conservador». Efecto: tres tests de
integración (`test_gateway_tool_calling_*`, `test_automode_multi_tool`) quedaban
*skipped* en silencio aunque hubiera un modelo con tool calling levantado, con
el motivo engañoso «model does not support native tool calling».

**DESPUÉS:** hace una petición real con una herramienta trivial y comprueba que
la respuesta trae `tool_calls`. Detalle importante descubierto al probarlo: con
`Qwen3` hay que enviar `chat_template_kwargs: {"enable_thinking": false}`, porque
por defecto razona y puede gastar todo `max_tokens` en `reasoning_content` sin
emitir la llamada (`finish_reason: length`). Con el thinking desactivado la
detección es determinista.

`test_automode_multi_tool` ejecuta el modelo real y tarda ~8 minutos, así que
queda detrás de `LAI_MODEL_TESTS=1`, con ese motivo explícito en el skip (no un
motivo falso). Verificado: con `LAI_MODEL_TESTS=1` **pasa** (1 passed, 475 s).

## 5. Verificación del proyecto

| Comprobación | Resultado |
|---|---|
| `cargo test` | ✅ 230 tests, 0 fallos |
| `cargo fmt --check` / `clippy -D warnings` | ✅ |
| `pytest tests swebench` | ✅ 242 passed, 8 skipped (llama.cpp arriba activa un test más) |
| `pytest` con `LAI_MODEL_TESTS=1` | ✅ `test_automode_multi_tool` pasa (475 s) |
| `ruff check .` | ✅ |
| `bash -n scripts/start-llama.sh` | ✅ |
| `make sandbox-check` | ✅ |
| `lai doctor` / `lai status` | ✅ |

## 6. Lo que NO se ha probado (Regla 6)

- **Piloto SWE-bench con esta configuración.** Sigue pendiente; los resultados
  de `BENCHMARKS.md` del 27-nov usaron `-ngl 50 -c 18432`.
- **Otros modelos.** La cascada está pensada para modelos de ~5 GiB en 8 GB de
  VRAM; con un modelo mayor o una GPU distinta elegirá otra fila o necesitará
  `LAI_NGL`/`LAI_CONTEXT` explícitos.
- **`-np > 1`.** Se arranca con un slot (prompt cache de una conversación).
- **Gateway (8081).** No se arrancó; el agente habla directo con llama.cpp.
