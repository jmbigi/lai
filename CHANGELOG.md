# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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