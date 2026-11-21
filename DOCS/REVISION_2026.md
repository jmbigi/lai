# Revisión técnica y de investigación — 2026

Este documento resume la revisión de todo el código del repositorio y las
prácticas actuales investigadas para mejorar `lai`. No sustituye a
`DOCUMENTACION_REGLAS.md`, que sigue siendo la fuente única de reglas.

## 1. Alcance y método

- Revisión estática de `lai.py`, `gateway.py`, `harness.py`, `gateway-go/main.go`,
  `chess/chess.py`, `todo_api.py`, `rate_limiter_lai.py`, generadores de ajedrez
  y `scripts/version.sh`.
- Ejecución de `py_compile`, `ruff`, `mypy` y pruebas de humo contra los servicios
  locales (llama.cpp 8080, gateway 8081, Ollama 11434).
- Verificación real de tool calling nativo en llama.cpp con Qwen3-8B.
- Investigación de documentación oficial y proyectos de referencia.

## 2. Investigación: qué hacen los mejores agentes locales

### 2.1 Tool calling nativo, no heurísticas

llama.cpp soporta function calling en `/v1/chat/completions` cuando el servidor
usa una plantilla Jinja compatible (`--jinja`, activo por defecto en builds
recientes). La respuesta incluye `tool_calls` con nombre y argumentos JSON.

- Documentación: <https://github.com/ggml-org/llama.cpp/blob/master/docs/function-calling.md>
- Servidor: <https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md>

**Decisión aplicada:** `AutoMode` ahora usa un bucle de tool calling nativo.
El modelo recibe el esquema de herramientas, ejecuta una o varias, recibe los
resultados como mensajes `role: tool` y continúa hasta responder sin tool calls.
Esto sustituye la heurística anterior basada en palabras como "create" o "list".

### 2.2 Muestreo recomendado para Qwen3

La documentación de Qwen3 distingue modo pensante y no pensante:

- No pensante: `temperature=0.7`, `top_p=0.8`, `top_k=20`, `min_p=0`.
- Pensante: `temperature=0.6`, `top_p=0.95`, `top_k=20`, `min_p=0`.
- Para código conviene bajar la temperatura (0.0–0.2) y desactivar thinking.

Fuente: <https://github.com/QwenLM/Qwen3/blob/main/docs/source/getting_started/quickstart.md>

**Decisión aplicada:** `chat_messages` envía `top_p`, `top_k`, `min_p` y
`chat_template_kwargs.enable_thinking`. El modo agente usa `thinking=False` y
temperatura baja.

### 2.3 Contexto estructural: repo map

Aider demuestra que un mapa compacto de clases/funciones mejora la calidad y
reduce tokens frente a volcar archivos completos.

- Documentación: <https://aider.chat/docs/repomap.html>

**Decisión aplicada:** `ToolRegistry.repo_map()` construye un mapa con `ast`
(solo Python, por ahora) y está disponible como herramienta del agente.
También se añadió `find_files()`.

### 2.4 Validación antes de escribir

Los agentes fiables validan sintaxis y ejecutan linters/pruebas después de cada
edición. En Python, `compile()` es la barrera mínima; `ruff` y `pytest` son la
siguiente capa.

**Decisión aplicada:** `ToolRegistry.write_file()` rechaza archivos `.py` con
error de sintaxis y devuelve el error al modelo para que se auto-corrija.
`run_command`, `run_python` y `grep` tienen timeout y límite de salida.

### 2.5 Seguridad y modelo local

- El endpoint debe ser loopback/privado. Se añadió `is_local_url()`.
- `LAI.chat()` rechaza endpoints públicos (Regla 4).
- `serve` escucha en `127.0.0.1:8082` por defecto, no en `0.0.0.0:8080`.
- `run_command` sigue usando `shell=True`: no es un sandbox. AutoMode solo debe
  usarse en repositorios de confianza.

## 3. Hallazgos críticos en `lai.py`

### Corregidos en esta revisión

| Hallazgo | Estado |
|---|---|
| `get_tools_schema()` usaba `false` (NameError) | Corregido; esquema válido con 16 herramientas |
| `AutoMode` elegía herramientas por palabras clave | Reemplazado por tool calling nativo |
| `review` era un stub que solo imprimía el nombre | Implementado con chunking y números de línea |
| `serve` era un stub sin servidor | Implementado: `/health`, `/v1/models`, `/v1/chat/completions` |
| `test` pasaba `--pattern` a pytest (argumento inválido) | Corregido: glob o `-k` |
| `project` duplicaba llaves del JSON y compilaba archivos no Python | Corregido: sin doble escape, prompt por tipo, sin placeholders inválidos |
| `config edit` ejecutaba literalmente `${EDITOR:-vim}` | Corregido: usa `$EDITOR` |
| `install --dev` pasaba `--dev` a pip | Corregido: `pip install -e .[dev]` |
| `update --all` no imprimía ni actualizaba nada | Corregido: lista obsoletos y actualiza |
| `clean` solo borraba en la raíz | Corregido: patrones recursivos |
| `get_lai_instance()` ignoraba `--model`/`--base-url` | Corregido: recrea si cambian |
| `start_service('ollama')` no usaba `ollama serve`; health path incorrecto | Corregido |
| `status` podía reportar Ollama detenido | Corregido con `/api/tags` y fallback TCP |
| `write_file` escribía Python inválido | Corregido: valida sintaxis |
| `grep` no era case-insensitive ni filtraba por tipo | Corregido |
| `run_python` dejaba temporales si fallaba | Corregido con `finally` |
| `chat` permitía endpoints públicos | Corregido con guard de Regla 4 |

### Pendientes prioritarios

1. **Refactorizar `chat()`**: sigue duplicando lógica de streaming y limpieza de
   thinking. Debe delegar en `chat_messages()`.
2. **Historial acotado**: `conversation_history` crece sin límite. Añadir
   `_trim_history()` por caracteres/tokens.
3. **Streaming con tool calls**: el agente usa modo no-streaming; es correcto,
   pero la API `serve` podría emitir chunks reales.
4. **Sandbox**: `run_command`/`python_eval` son peligrosos. Evaluar `bubblewrap`
   o contenedores para AutoMode.
5. **Diff editing**: para modelos pequeños, escribir archivos completos es más
   fiable que parches; mantener `apply_patch` como opción avanzada.
6. **Tests**: añadir pruebas unitarias de herramientas, CLI y guard local.

## 4. Hallazgos en el resto del código

### `gateway.py` (legado)

- **P0:** en cache hit devuelve la petición, no la respuesta; además cachea antes
  del upstream.
- **P1:** streaming SSE roto; `tools`/`tool_choice` eliminados; servidor
  mono-hilo; filtro que borra mensajes legítimos; compresión que elimina
  indentación de código.
- **P1:** escucha en `0.0.0.0` sin autenticación y sin límite de cuerpo.
- **P2:** `MAX_CONTEXT_TOKENS=16384` frente a 32768 del resto.

**Recomendación:** declararlo legado y no usarlo; el camino soportado es
`gateway-go` o llama.cpp directo.

### `harness.py`

- **P0:** `extract_reasoning()` lanza `AttributeError` con respuestas normales
  por un patrón mal escapado.
- **P0:** `eval`/`exec` con sandbox evadible y `shell=True`.
- **P1:** AutoMode es un stub; no añade assistant/tool_calls al historial.
- **P1:** path traversal en sesiones; truncado muta el historial.

**Recomendación:** no usar `harness.py` como harness de producción hasta
corregir estos puntos; `lai.py` ya cubre el caso de uso principal.

### `chess/chess.py`

- **P0:** no tiene `if __name__ == "__main__"`, no arranca.
- **P0:** `islower()`/`isupper()` no funcionan con piezas Unicode; jaque y color
  rotos.
- **P1:** no respeta turno, permite capturar piezas propias, no valida jaque.
- **P1:** `parse_coord` puede lanzar `IndexError`/`ValueError`.

**Recomendación:** tratarlo como ejemplo generado, no como producto terminado.
Si se quiere conservar, reescribir la lógica con índices y validación.

### `todo_api.py`

- **P0:** `/todos/{todo_id}` tapa `/todos/count`.
- **P1:** estado en memoria sin lock, CORS `*`, escucha en `0.0.0.0` sin auth.

### `rate_limiter_lai.py`

- **P1:** el refill con `int()` pierde fracciones y puede no reponer nunca.
- **P2:** acepta tokens negativos; usa `time.time()` en vez de `monotonic()`.
- No está integrado en `lai.py` ni en el gateway.

### `gateway-go`

- Funciona y es el gateway recomendado, pero hay que verificar coherencia entre
  `main.go`, `config.yaml` y `openapi.yaml` (modelos, límites, endpoints).
- Añadir autenticación opcional y límites de cuerpo si se expone fuera de
  loopback.

### Generadores de ajedrez y `scripts/version.sh`

- Los generadores pueden sobrescribir `chess.py` con contenido vacío o con
  reasoning; no validan sintaxis ni hacen backup.
- `version.sh` crea el tag antes de commitear `VERSION` y acepta tags arbitrarios.

## 5. Roadmap recomendado

### P0 — antes de usar en producción

1. Corregir o retirar `gateway.py` y `harness.py`.
2. Añadir tests de `lai.py` (herramientas, guard local, CLI).
3. Validar `AutoMode` con tareas reales y límites de iteraciones.
4. Documentar que `run_command` no es un sandbox.

### P1 — calidad

1. Refactorizar `chat()` sobre `chat_messages()` y acotar historial.
2. Añadir `ruff`/`pytest` automáticos tras `code()` y `project()`.
3. Implementar `lai map` para inspeccionar el repo map.
4. Mejorar `project_cmd` con plantillas por tipo (no solo Python).
5. Añadir autenticación y límites al gateway si se expone en LAN.

### P2 — higiene

1. `.gitignore` para `__pycache__`, `sessions/`, binarios y entornos.
2. `pyproject.toml`, `requirements*.txt`, `Makefile`, `tests/`.
3. Unificar versiones y contexto (32768) en todos los documentos.
4. Marcar documentos históricos con aviso de vigencia.

## 6. Fuentes consultadas

- llama.cpp function calling: <https://github.com/ggml-org/llama.cpp/blob/master/docs/function-calling.md>
- llama.cpp server: <https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md>
- Qwen3 quickstart: <https://github.com/QwenLM/Qwen3/blob/main/docs/source/getting_started/quickstart.md>
- Aider repo map: <https://aider.chat/docs/repomap.html>
- OpenAI function calling: <https://platform.openai.com/docs/guides/function-calling>
- Python `ast`: <https://docs.python.org/3/library/ast.html>
- Ruff: <https://docs.astral.sh/ruff/>
- pytest: <https://docs.pytest.org/>
