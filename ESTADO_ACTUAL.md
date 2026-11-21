# Estado Actual del Proyecto lai — 2026-11-21

## Resumen Ejecutivo

**lai** (Local AI Code Assistant) es un CLI completo para revisar, reparar y mejorar código usando LLMs locales (Qwen3-8B via llama.cpp). Todo código generado por lai usa modelos locales (Regla 4); el código propio de lai puede generarse con IA cloud (Regla 5).

## Arquitectura Actual

```
Usuario → lai.py (CLI) → ToolRegistry (16 tools) → llama.cpp:8080
                      ↓
              gateway-go:8081 (proxy, cache, filter) → llama.cpp:8080
                      ↓
              lai.py serve:8082 (OpenAI-compatible) → OpenCode / clientes
```

### Servicios Activos

| Servicio | Puerto | Bind | Modelo/Config |
|---|---|---|---|
| llama.cpp | 8080 | 127.0.0.1 | Qwen3-8B-Q4_K_M, 32768 ctx, -ngl 99, **deepseek + budget 1024** |
| gateway-go | 8081 | 127.0.0.1 | v3.1, cache LRU 5000, semantic cache, filter preserva tools/system |
| Ollama | 11434 | 127.0.0.1 | qwen3:8b (backup) |
| lai serve | 8082 | 127.0.0.1 | OpenAI-compatible, SSE, reasoning_content |

## Funcionalidades Implementadas

### Core lai.py
- **Regla 4 guard**: `is_local_url()` bloquea endpoints públicos
- **AutoMode nativo**: tool calling nativo llama.cpp (no heurísticas), max_turns=20, cost tracking
- **chat_messages()**: tool calling, top_p/top_k/min_p, chat_template_kwargs, reasoning_content, **streaming con tool_calls deltas**
- **Serve**: `/health`, `/v1/models`, `/v1/chat/completions` (SSE), **streaming tool_calls**, reasoning_content
- **Review**: chunking 120 líneas, focus, formatos text/json/sarif
- **Tools (16)**: read_file(strict), write_file(strict), list_dir, run_command, grep, find_files, repo_map, python_eval, apply_patch, web_search, run_python, run_tests, lint_file, format_code, check_types, analyze_complexity, find_unused_imports, find_unused_code, find_todos, check_security, check_dead_code, analyze_dependencies, validate_syntax, validate_imports, validate_types, validate_project, fix_imports, fix_syntax, fix_formatting, fix_types, auto_fix, analyze_project, get_project_structure, find_duplicates, check_circular_imports, fix_missing_init, fix_circular_imports, fix_relative_imports, normalize_imports, run_quality_check, generate_type_stubs, check_docstrings
- **Bash/Zsh completion**: `lai completion bash|zsh`

### Gateway-go v3.1
- Proxy reverso llama.cpp/Ollama con circuit breaker, load balancing
- Cache LRU + semántico, compresión preserva código
- Filtro **no destructivo**: preserva system, tool messages, tool calls
- Cache key incluye stream, tools, todos sampling params
- Bind 127.0.0.1:8081 (loopback only)

### llama.cpp
- Qwen3-8B-Q4_K_M, 32768 ctx, -ngl 99, flash-attn, cont-batching
- `--reasoning-format deepseek --reasoning-budget 1024` → reasoning en `reasoning_content`, `content` limpio

## Estándares y Calidad

- `make check` = compile + ruff + pytest (**24 tests pasan**, 3 skipped)
- `pyproject.toml` single source: version, deps, ruff, mypy, black, isort, pytest
- Ruff per-file ignores para CLI patterns (T201, ARG001, PTH*, PLR*, etc.)
- Tests: is_local_url, ToolRegistry, get_tools_schema, get_lai_instance, CLI, **Integration (gateway, serve, AutoMode)**

## Documentación

| Archivo | Estado |
|---|---|
| `DOCUMENTACION_REGLAS.md` | Fuente única verdad (8 reglas, jerarquía, cumplimiento) |
| `DOCS/REVISION_2026.md` | Revisión técnica, investigación, hallazgos, roadmap |
| `AGENTS.md` | Guía para agentes IA |
| `README.md` | User-facing (actualizar comandos) |
| `CHANGELOG.md` | Historial versiones |
| `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md` | Estándares |
| `LICENSE` | GPL-3.0-or-later |
| Históricos (`DOCUMENTACION_FINAL.md`, `LECCIONES_APRENDIDAS.md`, etc.) | Banners vigencia ⚠️ |

## Próximas Mejoras (Plan de Acción)

### 1. Bash Completion (~30 min) ✅ **COMPLETADO**
- `lai completion bash|zsh` genera script completado
- Subcomandos, flags, modelos, rutas

### 2. Streaming Tool Calls Real (3-5h) ✅ **COMPLETADO**
- `chat_messages(stream=True)` → SSE con `tool_calls` chunked
- `serve` SSE real: envía `tool_calls` deltas, non-streaming con tool_calls
- Gateway-go ya pasa streaming raw; lai.py parser ajustado

### 3. Tests Integración E2E (4-6h, incremental) ✅ **COMPLETADO**
- Fixtures pytest: llama.cpp + gateway reales
- AutoMode multi-tool (crear archivo → test → lint)
- Serve round-trip: chat + tools + streaming (skipped si serve no running)
- Gateway tool calling (non-stream + stream) ✅

### 4. LSP Integration (opcional, 4-8h) ⏸ PENDIENTE DECISIÓN
- `python-lsp-server` para goto_definition, hover, references
- Extender `repo_map` o nuevo tool `lsp_query`

### 5. Auth Gateway (opcional, 2-4h) ⏸ PENDIENTE DECISIÓN
- Solo si se expone fuera de loopback
- API key + rate limit en gateway-go config

### 6. Chat history trimming + chat() refactor ✅ **COMPLETADO**
- Delegar `chat()` a `chat_messages()` para evitar duplicación
- Límite de historial por tokens (history_max_tokens=12000)
- _strip_thinking_artifacts() para limpiar respuestas

### 7. Sandboxing para run_command/python_eval ✅ **COMPLETADO**
- bubblewrap para run_command y run_python (sandbox=True por defecto)
- --unshare-all, --cap-drop ALL, bind mounts selectivos
- Verificado: bloquea /etc/shadow, red; permite python execution
- Opción sandbox=False para compatibilidad

## Comandos de Desarrollo

```bash
# Calidad
make check          # compile + ruff + pytest
make lint           # ruff
make format         # ruff --fix + black
make type           # mypy
make test           # pytest

# Servicios
./scripts/start-llama.sh      # llama.cpp 8080
./scripts/start-gateway.sh    # gateway-go 8081

# lai
python3 lai.py status
python3 lai.py auto "tarea" --max-iterations 3 --max-turns 10
python3 lai.py serve --port 8082
python3 lai.py review archivo.py --format sarif

# Versionado
./scripts/version.sh version
./scripts/version.sh bump patch --dry-run
./scripts/version.sh bump patch
```

## Versiones

- lai: **0.0.2** (pyproject.toml single source)
- gateway-go: **3.1.0** (main.go log line)
- OpenAPI: **3.0.0** (gateway-go/openapi.yaml)
- llama.cpp: b12567 (build custom)

## Issues Conocidos

1. ~~`chat()` legacy mantiene lógica duplicada; debería delegar en `chat_messages()`~~ ✅ **RESUELTO**
2. ~~Historial conversacional sin límite (crece indefinido)~~ ✅ **RESUELTO** (_trim_history)
3. ~~`serve` SSE single-chunk; no streaming real de tool calls~~ ✅ **RESUELTO**
4. ~~`run_command`/`python_eval` usan `shell=True` (no sandbox)~~ ✅ **RESUELTO** (bubblewrap sandbox)
5. `gateway.py`/`harness.py` legacy — no mantener
6. Versiones divergentes: gateway 3.1.0 vs OpenAPI 3.0.0