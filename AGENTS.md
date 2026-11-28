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

`lai_rust/src/` (module list taken from `lib.rs`, 2026-11-26):

```
lai_rust/
├── Cargo.toml
└── src/
    ├── main.rs              # clap CLI: 20 subcommands (chat, code, auto, verify, repair, plan, bench, init, doctor, tui, mcp…)
    ├── commands.rs          # implementations of the CLI subcommands
    ├── llm.rs               # HTTP client for llama.cpp / Ollama; Rule 4 local-only guard
    ├── tools.rs             # ToolRegistry (16 tools) + JSON schemas + perfiles (full/bash)
    ├── context_budget.rs    # presupuesto de contexto: elide salidas antiguas
    ├── context.rs           # project understanding (structure, docs, patterns, objectives)
    ├── planner.rs           # DAG planner + SPEC.md generation
    ├── change_engine.rs     # controlled changes, diff/patch, checkpoints, rollback
    ├── diagnostic_engine.rs # failure localisation and root-cause classification
    ├── repair_engine.rs     # automated repair with validation and rollback
    ├── verification_engine.rs # multi-level verification, bounded verdicts
    ├── baseline.rs          # baseline tracking and regression detection
    ├── memory_store.rs      # 4-tier memory with provenance and invalidation
    ├── policy.rs            # policy engine used by tools
    ├── sandbox.rs           # bubblewrap isolation: preflight + fail-closed wrapper
    ├── contracts.rs         # external verifier contracts + admission gate
    ├── quality.rs           # quality checks
    ├── repomap.rs           # repository symbol map
    ├── services.rs          # local service management (llama.cpp, gateway, ollama)
    ├── telemetry.rs         # local observability
    ├── tui.rs               # terminal UI (diff viewer, file tree, chat, tool log)
    ├── init_wizard.rs       # interactive local setup
    ├── mcp_server.rs        # MCP (JSON-RPC 2.0) server exposing lai tools
    ├── benchmark/           # benchmark framework + mandatory contracts
    ├── orchestration/       # alternative orchestration engines (e.g. LangGraph adapter)
    └── verification/        # formal verification adapters (Dafny, TLA+, neural)
```

> The legacy Python CLI (`lai.py`) is **not** at parity with the Rust crate: it
> has more subcommands (serve, review, fix, search…) but is in maintenance mode.
> Do not assume a command exists in both.

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
cargo run -- auto "task" --profile bash          # una sola herramienta, para modelos pequeños
LAI_CONTEXT_WINDOW=8192 cargo run -- auto "task"  # presupuesto de contexto explícito
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

- **Sandbox opcional y fail-closed.** En el CLI Rust, `run_command` y
  `run_python` aceptan `"sandbox": true` para ejecutar dentro de bubblewrap
  (workspace en lectura/escritura, `/tmp` temporal, sin red, `--cap-drop ALL`).
  Con `LAI_REQUIRE_SANDBOX=1` o `lai --sandbox …` el sandbox es obligatorio y no
  se puede desactivar; si bwrap no está disponible, la llamada falla en vez de
  ejecutar sin aislamiento. Por defecto sigue **desactivado** para no romper
  flujos existentes: usar `auto` solo en repositorios confiables y activar
  `--sandbox` cuando el código no sea de confianza. El CLI Python legado usa
  bubblewrap por defecto.
- La guarda de manifiestos cubre `write_file`, `edit` y `apply_patch` (rechaza
  reemplazos que eliminen ≥75 % del manifiesto o lo borren, salvo
  `allow_manifest_overwrite: true`); el perfil `bash` puede saltarla con
  `cat > Cargo.toml`.
- AutoMode only on trusted repositories
- Gateway binds to loopback; no auth by default
- Never expose gateway/llama.cpp to public interfaces
- **Rule 4 enforced in code**: `llm.rs::is_local_url()` + `LlmClient::check_endpoint()`
  abort before any request to a non-local endpoint (override only with
  `--allow-remote`, explicit user decision)
- Service binaries are resolved from the environment, never hardcoded:
  `LAI_LLAMA_SERVER`, `LAI_GATEWAY_BIN`, `LAI_GATEWAY_DIR`, `LAI_OLLAMA_BIN`
- Sandbox environment flags: `LAI_REQUIRE_SANDBOX=1` (mandatory, fail-closed),
  `LAI_SANDBOX_ALLOW_NET=1` (network inside the sandbox),
  `LAI_COMMAND_ALLOWLIST=1` + `LAI_ALLOW_COMMANDS=a,b` (opt-in command allowlist
  in `auto`), `LAI_PROTECTED_PATHS=…` (extra protected paths)
- `read_file`/`grep` refuse credential paths (`~/.ssh`, `~/.aws`, `*.pem`,
  `id_rsa`, …) unless `allow_sensitive_read: true`.
- `auto` does not execute tool calls from a response with
  `finish_reason == "length"` (they may be truncated).
- `cache_prompt: true` is sent on every request (disable with
  `LAI_CACHE_PROMPT=0`); `LAI_DEBUG_TIMINGS=1` prints llama.cpp timings.

## Documentation

- `DOCUMENTACION_REGLAS.md` — Single source of truth for rules
- `DOCS/CAMBIOS_2026-11-26.md` and `DOCS/CAMBIOS_2026-11-26_MEJORAS2.md` — change records (Rule 10)
- `DOCS/INFERENCIA_LOCAL_BAJA_GAMA.md` — CPU inference guide for low-end teams (Rule 11)
- `DOCS/REVISION_2026.md` — Technical review and research
- `README.md` — User-facing overview
- `CHANGELOG.md` — Version history
- `AGENTS.md` — This file

## Version Management

- Version in `lai_rust/Cargo.toml` (primary) and `pyproject.toml` (legacy)
- `scripts/version.sh` for tagging (use with caution)
- Gateway version in `gateway-go/config.yaml` and `main.go`

## Migration Notes (Python → Rust)

The Rust port is the primary implementation. Historical module correspondence
(the Python modules were not ported 1:1; several Rust modules are new):

| Python (legacy) | Rust equivalent |
|-----------------|-----------------|
| `lai.py` (LAI class) | `llm.rs` (client), `commands.rs`, `main.rs` |
| `lai_tools.py` (ToolRegistry) | `tools.rs` |
| `lai_automode.py` (AutoMode) | `planner.rs` + `change_engine.rs` + `verification_engine.rs` (deterministic loop) |
| `lai_agents.py` (Orchestrator) | `orchestration/` (not at parity) |
| session/state handling | `memory_store.rs`, `change_engine.rs` (checkpoints) |
| thinking extraction | `llm.rs` / `telemetry.rs` |
| config management | `init_wizard.rs`, `policy.rs` |

Gateway (Go) unchanged.
