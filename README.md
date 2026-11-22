# lai - Local AI Code Assistant

A comprehensive CLI tool for reviewing, repairing, and improving any code project using local LLMs (Qwen3-8B via llama.cpp).

## Mission (Regla 0)

**lai existe para impulsar el uso de IAs locales, abiertas y soberanas.**

- **Open source first:** Modelos, herramientas y licencias abiertas.
- **Sin dependencia de claves/API keys:** El usuario no necesita cuentas en la nube, suscripciones ni tokens externos.
- **Sin dependencia de cuentas en la nube:** Inferencia, almacenamiento y procesamiento ocurren localmente por defecto.
- **Software gratuito y de calidad:** Modelos abiertos (Qwen, Llama, Gemma) servidos localmente con llama.cpp/Ollama.
- **Privacidad por diseño:** Código y datos del usuario nunca salen de su máquina salvo decisión explícita.

> **Regla 0:** *Cualquier feature, arquitectura o dependencia que introduzca dependencia obligatoria de cloud, API keys, cuentas externas o software propietario será rechazada salvo aprobación explícita y documentada del operador.*

## Key Rules

1. **Regla 4**: Output of `lai` commands (`code`, `project`, `auto`, `fix`, `review`, `serve`) **must** use a local model.
2. **Regla 5**: lai's own code (`lai.py`, `gateway-go`, scripts, tests, docs) may be generated with cloud or local.
3. **Local-only guard**: `is_local_url()` validates endpoints before any generation.
4. **Tool calling**: AutoMode uses llama.cpp native function calling (not keyword heuristics).
5. **Loopback only**: `serve` binds to `127.0.0.1:8082` by default; gateway binds to `127.0.0.1:8081`.

## Architecture

```
┌─────────────┐     ┌──────────────────┐     ┌────────────────────┐
│   lai CLI   │────▶│  Gateway Go 8081 │────▶│  llama.cpp 8080    │
│  (Rust/Py)  │     │  Circuit Breaker │     │  FP8 KV Cache      │
│             │     │  Load Balancing  │     │  Speculative Dec.  │
└─────────────┘     │  Semantic Cache  │     │  Context Checkpt.  │
                    │  Compression     │     │  Reasoning Budget  │
                    └──────────────────┘     └────────────────────┘
                              │
                              ▼
                      ┌────────────────────┐
                      │   Ollama 11434     │
                      │   (Fallback)       │
                      └────────────────────┘
```

## Implementations

| Implementation | Location | Status |
|---------------|----------|--------|
| **Python (legacy)** | `lai.py`, `lai_tools.py`, `lai_automode.py` | Stable |
| **Rust (current)** | `lai_rust/` | **Primary** |

## Quick Start

```bash
# Build Rust version (recommended)
cd lai_rust && cargo build --release
./target/release/lai --help

# Or use Python version
python3 lai.py --help

# Check services
lai status

# Start services if needed
lai status  # will prompt to start stopped services

# Chat with AI
lai chat "Hola, ¿cómo estás?"

# Generate code
lai code "Crear una calculadora en Python" calculadora.py

# Autonomous agent
lai auto "Refactorizar el proyecto para usar async/await" --max-iterations 10

# Code review
lai review ./mi_proyecto --focus security

# Run tests
lai test ./tests -v
```

## Commands

| Command | Description |
|---------|-------------|
| `chat` | Interactive or single-prompt chat |
| `generate` | Generate text |
| `code` | Generate code file with iterative refinement |
| `think` | Deep thinking mode |
| `auto` | Autonomous agent (Plan + ReAct + Reflection) |
| `run` | Execute Python script |
| `serve` | Start OpenAI-compatible API server |
| `status` | Check local services (llama.cpp, gateway, ollama) |
| `models` | List available models |
| `tools` | List available tools |
| `search` | Search code/files by pattern |
| `find` | Find files by glob pattern |
| `fix` | Auto-fix lint, format, imports, types |
| `review` | Code review (security, performance, style) |
| `diff` | Show diff between files |
| `test` | Run pytest |
| `init` | Initialize new project |
| `config` | Manage configuration |
| `install` | Install dependencies |
| `update` | Update dependencies |
| `exec` | Execute command in project context |
| `doctor` | Diagnose environment |
| `clean` | Clean build artifacts |
| `completion` | Generate shell completions |
| `project` | Generate multi-file project from spec |
| `repair` | Analyze and auto-repair project |

## AutoMode Intelligence

The `auto` command implements advanced agent patterns:

- **Plan-and-Execute**: Decomposes goals into verifiable subtasks
- **ReAct (Reason + Act)**: Explicit Thought → Action → Observation loop
- **Reflection**: Separate critic agent evaluates output against rubric
- **Best-version fallback**: Returns highest-scored version if not converged
- **Cost tracking**: Token usage, estimated cloud-equivalent savings

## Tool Registry (50+ tools)

Core: `read_file`, `write_file`, `list_dir`, `run_command`, `grep`, `find_files`, `repo_map`, `python_eval`, `apply_patch`, `web_search`, `run_python`, `run_tests`

Quality: `lint_file`, `format_code`, `check_types`, `analyze_complexity`, `find_unused_imports`, `find_unused_code`, `find_todos`, `check_security`, `check_dead_code`, `analyze_dependencies`

Validation: `validate_syntax`, `validate_imports`, `validate_types`, `validate_project`

Auto-repair: `fix_imports`, `fix_syntax`, `fix_formatting`, `fix_types`, `auto_fix`, `fix_missing_init`, `fix_circular_imports`, `fix_relative_imports`, `normalize_imports`

Project: `analyze_project`, `get_project_structure`, `find_duplicates`, `check_circular_imports`

Git: `git_init`, `git_add`, `git_commit`, `git_rollback`, `git_status`, `git_diff`

OCR: `ocr_image`, `ocr_image_best`, `ocr_screenshot`

## Configuration

Config file: `~/.lai/config.json` (global) or `.lai/config.json` (project)

```bash
lai config set model qwen3-8b
lai config set base_url http://localhost:8080/v1
lai config get model
lai config list
```

Environment variables:
- `LAI_LLAMA_MODEL` - Path to GGUF model for llama.cpp
- `LAI_CONTEXT` - Context size (default: 32768)
- `LAI_NGL` - GPU layers (default: 99)

## Development

```bash
# Rust version
cd lai_rust
cargo build           # Debug build
cargo build --release # Release build
cargo test            # Run tests
cargo clippy          # Lint
cargo fmt             # Format

# Python version
make check            # compile + ruff + pytest
make lint             # ruff only
make format           # ruff --fix + black
make type             # mypy
make test             # pytest
make gateway-build    # Build Go gateway
```

## Services

lai manages three local services:

| Service | Port | Purpose |
|---------|------|---------|
| llama.cpp | 8080 | Primary inference (Qwen3-8B) |
| Gateway Go | 8081 | Reverse proxy with cache, circuit breaker |
| Ollama | 11434 | Alternative backend |

## Documentation

| File | Description |
|------|-------------|
| `AGENTS.md` | Agent instructions for this codebase |
| `DOCUMENTACION_REGLAS.md` | Single source of truth for rules |
| `DOCS/REVISION_2026.md` | Technical review and research |
| `CHANGELOG.md` | Version history |
| `gateway-go/config.yaml` | Gateway configuration |
| `gateway-go/openapi.yaml` | OpenAPI 3.1 spec |

## License

GPL-3.0-or-later - Free software for a free society.