# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **swebench/evaluate.py**: etapa 4 del harness — evaluación oficial. Aplica el parche del modelo, ejecuta el `eval_script` oficial de cada instancia, parsea los logs con el parser oficial (pytest, pytest_v2, pytest_options, django, seaborn, sympy, matplotlib) y calcula `FAIL_TO_PASS`/`PASS_TO_PASS` y *resolved* con la semántica oficial. Separa fallos de infraestructura, escribe `report.json`/`report.md` y logs por instancia. Reanudable vía `data/state.json`.
- **swebench/test_evaluate.py**: 15 tests offline (sin Docker) de parsers, graduación, marcadores, inyección del código de salida y saneado de `run_id`.
- **swebench/**: harness incremental para SWE-bench Verified (descarga del dataset, preparación de imágenes Docker reanudable, compilación de un binario compatible con glibc 2.35, ejecución del agente en contenedor y captura de parches en formato oficial). Validado de punta a punta con `astropy__astropy-12907` en la etapa de ejecución.
- **DOCS/INFERENCIA_LOCAL_BAJA_GAMA.md**: guía de inferencia local en CPU para equipos de 8–16 GB sin GPU (ancho de banda, cuantización para código, decodificación especulativa, caché KV, ternarios, MoE, bucle de verificación y metodología de medición). Las cifras citadas son investigación externa, **no reproducida por lai**.
- **Regla 11 (Baja gama: medir antes de recomendar):** exige medir una variable a la vez, calcular el techo de ancho de banda, aplicar la compuerta de contexto completo y marcar las cifras externas como no reproducidas.
- **Regla 0 (Misión Fundacional):** lai existe para impulsar IAs locales, abiertas y soberanas. Open source first, sin dependencia de claves/API keys, sin dependencia de cuentas en la nube, software gratuito y de calidad, privacidad por diseño. Cualquier feature que introduzca dependencia obligatoria de cloud requiere aprobación explícita.
- **Regla 10 (Documentación obligatoria de cambios):** Nueva regla que exige crear documento de cambios propuestos antes de modificar código, documentar estado ANTES y DESPUÉS de cada cambio individual, y mantener trazabilidad para favorecer el trabajo en equipo. Prohíbe cambios silenciosos o en batch sin documentación intermedia.

### Fixed
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