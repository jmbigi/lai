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

## Quick Start (5 min)

```bash
# 1. Clone & build
git clone https://github.com/jmbigi/lai.git
cd lai/lai_rust && cargo install --path . --locked

# 2. Install ollama (model manager)
curl -fsSL https://ollama.com/install.sh | sh

# 3. One-command setup wizard
lai init --auto
```

📖 **Guía completa**: [`QUICKSTART.md`](QUICKSTART.md) — capturas de TUI, MCP, benchmarking, troubleshooting.

```bash
# Primer agente autónomo
cd tu-proyecto
lai auto "añade tests para src/auth.rs"

# Terminal UI visual (4 paneles)
lai tui
# Ctrl+1/2/3/4: archivos/chat/diff/tools | Ctrl+H: ayuda

# MCP Server para Cursor/Cline/Claude Code
lai mcp --stdio    # Claude Code
lai mcp --port 8083 # Cursor/Cline

# Benchmark local (SWE-bench style)
cargo test --test swe_bench
```

## Commands

> **Vigencia (2026-11-27).** La tabla de abajo describe el CLI **Python
> legado** (`lai.py`), en mantenimiento. La implementación principal es
> **Rust** (`lai_rust/`), con 20 subcomandos:

| Comando Rust | Descripción |
|---|---|
| `chat` | Chat interactivo o de un solo prompt, con tool calling |
| `code` | Genera un archivo de código con ciclos de validación |
| `auto` | Agente autónomo (`--profile full` o `--profile bash`) |
| `status` | Estado de llama.cpp / gateway / ollama |
| `repo-map` | Mapa de símbolos del repositorio |
| `verify` | Verificación por niveles (sintaxis, lint, tipos, tests…) o mapeo de un verificador externo (`--external safe\|unsafe\|timeout\|unknown`) |
| `diagnose` | Diagnóstico de un error o log |
| `repair` | Diagnóstico + guía de reparación |
| `baseline` | Baselines y detección de regresiones |
| `memory` | Memoria local (semántica, episódica, de trabajo) |
| `checkpoint` | Checkpoints de la sesión actual |
| `metrics` | Métricas del workspace y telemetría de sesión |
| `plan` | Planificador DAG (requiere modelo local) |
| `bench` / `bench-swe` | Benchmarks de capacidad y estilo SWE-bench |
| `init` / `doctor` | Asistente de instalación y diagnóstico |
| `tui` | Interfaz de terminal |
| `mcp` | Servidor MCP (stdio o `127.0.0.1`) |

Ejemplo para equipos con poca memoria (una sola herramienta `bash`, prompt
corto y contexto ajustado):

```bash
LAI_CONTEXT_WINDOW=8192 lai auto "arregla el bug" --profile bash --max-iterations 12
```

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

## AutoMode Intelligence — CLI Python legado

> **Vigencia (2026-11-28).** Esta sección describe el CLI **Python legado**
> (`lai.py`), en mantenimiento. El `auto` de Rust es un bucle de herramientas
> con presupuesto de contexto, escalada por turnos y perfiles `full`/`bash`;
> **no** implementa Plan-and-Execute, ReAct ni Reflection.

The `auto` command implements advanced agent patterns:

- **Plan-and-Execute**: Decomposes goals into verifiable subtasks
- **ReAct (Reason + Act)**: Explicit Thought → Action → Observation loop
- **Reflection**: Separate critic agent evaluates output against rubric
- **Best-version fallback**: Returns highest-scored version if not converged
- **Cost tracking**: Token usage, estimated cloud-equivalent savings

## Tool Registry — Rust (16 tools)

> **Vigencia (2026-11-28).** Esta lista es del CLI **Rust** (`lai_rust/`), la
> implementación principal. El CLI Python legado (`lai_tools.py`) conserva su
> propio registro, más amplio; no asumir paridad entre ambos.

Core: `read_file`, `write_file`, `edit`, `list_dir`, `run_command`, `bash`,
`grep`, `find_files`, `repo_map`, `run_python`, `apply_patch`, `todo`,
`git_add`, `git_commit`, `git_status`, `git_diff`.

`run_command`, `bash` y `run_python` aceptan `"sandbox": true` para ejecutar en
bubblewrap (sin red, workspace escribible). Con `lai --sandbox …` o
`LAI_REQUIRE_SANDBOX=1` el sandbox es obligatorio y falla cerrado si no está
disponible.

## Security

- La inferencia es local (Regla 4); `llm.rs` bloquea endpoints no locales salvo
  `--allow-remote`.
- El sandbox de ejecución es **opt-in** por defecto y obligatorio con
  `--sandbox`. En Linux usa bubblewrap; en otros sistemas no está disponible.
- `auto` no aplica la allowlist de comandos del `PolicyEngine`: úsalo solo en
  repositorios confiables o con `--sandbox`.
- La guarda de manifiestos cubre `write_file`, `edit` y `apply_patch`; el perfil
  `bash` puede saltarla con `cat > Cargo.toml`.

## Tool Registry — CLI Python legado (50+ tools)

> **Vigencia (2026-11-28).** Esta lista es del **CLI Python legado**
> (`lai_tools.py`), en mantenimiento. La implementación principal es Rust; su
> registro está arriba. No asumir paridad entre ambas.

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
- `LAI_REQUIRE_SANDBOX` - `1` makes bubblewrap mandatory (fail-closed)
- `LAI_SANDBOX_ALLOW_NET` - `1` allows network inside the sandbox
- `LAI_COMMAND_ALLOWLIST` - `1` applies the command allowlist in `auto`
- `LAI_ALLOW_COMMANDS` - Comma-separated extra allowed commands
- `LAI_PROTECTED_PATHS` - Comma-separated extra protected paths
- `LAI_CACHE_PROMPT` - `0` disables prompt/KV cache reuse (default: enabled)
- `LAI_DEBUG_TIMINGS` - `1` prints llama.cpp prompt/cache/generation timings

## Development

### Windows (PowerShell)

```powershell
# Install/refresh toolchains and Python dependencies.
# Add -Elevate to relaunch with a UAC prompt for machine-wide installs.
powershell -ExecutionPolicy Bypass -File scripts/setup-dev.ps1 -Elevate

# Build and run Rust, Go and Python checks.
powershell -ExecutionPolicy Bypass -File scripts/dev-check.ps1

# Start/stop the local stack.
powershell -ExecutionPolicy Bypass -File scripts/start-services.ps1
powershell -ExecutionPolicy Bypass -File scripts/stop-services.ps1
```

`start-llama.ps1` resolves the model from `-ModelPath`, `LAI_MODEL_PATH`,
`models/*.gguf`, or an Ollama blob (default `qwen3:8b-nothink`). If port 8080
is already occupied, stop the conflicting process or start llama.cpp on
another port and update the upstream URL in `gateway-go/config.yaml`.

### Linux / macOS

```bash
./scripts/setup-dev.sh          # install toolchains + Python deps
./scripts/dev-check.sh          # cargo check/test + go build + pytest
./scripts/start-services.sh     # llama.cpp :8080 + gateway :8081
./scripts/stop-services.sh
```

### Manual commands

```bash
# Rust version
cd lai_rust
cargo build           # Debug build
cargo build --release # Release build
cargo test            # Run tests
cargo clippy          # Lint
cargo fmt             # Format

# Python version (legacy; GNU make required)
make check            # compile + ruff + pytest
make lint             # ruff only
make format           # ruff --fix + black
make type             # mypy
make test             # pytest
make gateway-build    # Build Go gateway
```

### Development scripts

| Script | Purpose |
|--------|---------|
| `scripts/setup-dev.ps1` / `.sh` | Install toolchains, create `.venv`, build gateway and CLI |
| `scripts/dev-check.ps1` / `.sh` | Run Rust, Go and Python checks (`-Full`/`--full` adds fmt + clippy/vet) |
| `scripts/start-llama.ps1` / `.sh` | Resolve a GGUF model and start llama.cpp on `127.0.0.1:8080` |
| `scripts/start-gateway.ps1` / `.sh` | Build/start the Go gateway on `127.0.0.1:8081` |
| `scripts/start-services.ps1` / `.sh` | Start llama.cpp and the gateway together |
| `scripts/stop-services.ps1` / `.sh` | Stop processes recorded in `logs/*.pid` |

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