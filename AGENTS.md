# Agent Instructions for lai

This file provides guidance for AI agents working on the lai codebase.

## Project Mission (Regla 0)

**lai existe para impulsar el uso de IAs locales, abiertas y soberanas.**

- **Open source first:** Modelos, herramientas y licencias abiertas.
- **Sin dependencia de claves/API keys:** El usuario no necesita cuentas en la nube, suscripciones ni tokens externos.
- **Sin dependencia de cuentas en la nube:** Inferencia, almacenamiento y procesamiento ocurren localmente por defecto.
- **Software gratuito y de calidad:** Modelos abiertos (Qwen, Llama, Gemma) servidos localmente con llama.cpp/Ollama.
- **Privacidad por diseño:** Código y datos del usuario nunca salen de su máquina salvo decisión explícita.

> **Regla 0:** *Cualquier feature, arquitectura o dependencia que introduzca dependencia obligatoria de cloud, API keys, cuentas externas o software propietario será rechazada salvo aprobación explícita y documentada del operador.*

## Project Overview

**lai** (Local AI Code Assistant) is a CLI tool that uses local LLMs (via llama.cpp) to review, repair, and improve code projects. It follows a strict rule: all code generation must use local models; cloud APIs are only for configuring lai itself.

### Implementations

| Implementation | Location | Status |
|---------------|----------|--------|
| **Rust (primary)** | `lai_rust/` | **Active development** |
| Python (legacy) | `lai.py`, `lai_tools.py`, `lai_automode.py` | Maintenance only |

## Architecture (Rust)

```
lai_rust/
├── Cargo.toml
└── src/
    ├── main.rs          # CLI entry point, command dispatch
    ├── commands.rs      # All 25+ command implementations
    ├── llm.rs           # LLM client (HTTP, streaming, tool calling)
    ├── services.rs      # Service management (llama.cpp, gateway, ollama)
    ├── tools.rs         # ToolRegistry with 50+ tools
    ├── automode.rs      # AutoMode: Plan + ReAct + Reflection
    ├── agents.rs        # Multi-agent orchestrator (critic, reviewer)
    ├── session.rs       # Session management
    ├── thinking.rs      # Thinking extraction/display
    └── config.rs        # Configuration management
```

## Key Rules (from DOCUMENTACION_REGLAS.md)

1. **Regla 4**: Output of `lai` commands (`code`, `project`, `auto`, `fix`, `review`, `serve`) **must** use a local model.
2. **Regla 5**: lai's own code (`lai.py`, `gateway-go`, scripts, tests, docs) may be generated with cloud or local.
3. **Local-only guard**: `is_local_url()` validates endpoints before any generation.
4. **Tool calling**: AutoMode uses llama.cpp native function calling (not keyword heuristics).
5. **Loopback only**: `serve` binds to `127.0.0.1:8082` by default; gateway binds to `127.0.0.1:8081`.
6. **Regla 11**: For low-end CPU teams, measure one variable at a time and never present external figures as lai results. See `DOCS/INFERENCIA_LOCAL_BAJA_GAMA.md`.
7. **Regla 10**: Every change must have a before/after record in a change document (see `DOCS/CAMBIOS_2026-11-26.md`).

## Development Workflow (Rust)

```bash
# Build
cd lai_rust
cargo build                 # Debug
cargo build --release       # Release (optimized)

# Quality checks
cargo check                 # Type check
cargo clippy                # Lint
cargo fmt --check           # Format check
cargo test                  # Run tests

# Run
cargo run -- --help
cargo run -- status
cargo run -- auto "task" --max-iterations 3
cargo run -- serve --port 8082

# Install
cargo install --path .
```

## Code Style (Rust)

- Edition 2021
- `cargo fmt` for formatting
- `cargo clippy` for linting (deny warnings in CI)
- Type hints encouraged
- Documentation comments for public APIs (`///`)

## Testing

- Unit tests in each module (`#[cfg(test)]`)
- Integration tests in `tests/` directory
- Mock external services; integration tests use real llama.cpp/gateway
- Run with `cargo test`

## Common Patterns

### Adding a Tool
1. Add method to `ToolRegistry` in `tools.rs`
2. Add schema to `get_tools_schema()`
3. Test with `cargo run -- auto "use my tool"`

### Modifying Gateway
1. Edit `gateway-go/main.go` and `gateway-go/config.yaml`
2. `make gateway-build` (or `cd gateway-go && go build -o gateway`)
3. Restart gateway process

### Updating Model Config
- llama.cpp flags in `scripts/start-llama.sh`
- Context: 32768, Output: 4096, GPU layers: 99

## Security Notes

- `run_command` and `run_python` run with sandbox (bubblewrap) by default
- AutoMode only on trusted repositories
- Gateway binds to loopback; no auth by default
- Never expose gateway/llama.cpp to public interfaces
- **Rule 4 enforced**: `is_local_url()` blocks non-local endpoints

## Documentation

- `DOCUMENTACION_REGLAS.md` — Single source of truth for rules
- `DOCS/REVISION_2026.md` — Technical review and research
- `README.md` — User-facing overview
- `CHANGELOG.md` — Version history
- `AGENTS.md` — This file

## Version Management

- Version in `lai_rust/Cargo.toml` (primary) and `pyproject.toml` (legacy)
- `scripts/version.sh` for tagging (use with caution)
- Gateway version in `gateway-go/config.yaml` and `main.go`

## Migration Notes (Python → Rust)

The Rust port maintains full feature parity with Python:

| Python Module | Rust Module |
|---------------|-------------|
| `lai.py` (LAI class) | `llm.rs` (LlmClient), `main.rs` |
| `lai_tools.py` (ToolRegistry) | `tools.rs` |
| `lai_automode.py` (AutoMode) | `automode.rs` |
| `lai_agents.py` (Orchestrator) | `agents.rs` |
| `session` management | `session.rs` |
| `thinking` extraction | `thinking.rs` |
| `config` management | `config.rs` |

All 25+ CLI commands ported. Gateway (Go) unchanged.