# Agent Instructions for lai

This file provides guidance for AI agents working on the lai codebase.

## Project Overview

**lai** (Local AI Code Assistant) is a CLI tool that uses local LLMs (via llama.cpp) to review, repair, and improve code projects. It follows a strict rule: all code generation must use local models; cloud APIs are only for configuring lai itself.

## Architecture

- **lai.py** — Main entry point, CLI, agent loop, tool registry, HTTP server
- **gateway-go/** — Go reverse proxy for llama.cpp (port 8081), handles caching, compression, filtering
- **llama.cpp** — Inference server (port 8080), Qwen3-8B-Q4_K_M, 32768 context, `-ngl 99`, `--reasoning-format deepseek --reasoning-budget 1024`
- **Ollama** — Alternative local backend (port 11434)

## Key Rules (from DOCUMENTACION_REGLAS.md)

1. **Regla 4**: Output of `lai` commands (`code`, `project`, `auto`, `fix`, `review`, `serve`) **must** use a local model.
2. **Regla 5**: lai's own code (`lai.py`, `gateway-go`, scripts, tests, docs) may be generated with cloud or local.
3. **Local-only guard**: `is_local_url()` validates endpoints before any generation.
4. **Tool calling**: AutoMode uses llama.cpp native function calling (not keyword heuristics).
5. **Loopback only**: `serve` binds to `127.0.0.1:8082` by default; gateway binds to `127.0.0.1:8081`.

## Development Workflow

```bash
# Run quality checks
make check          # compile + ruff + pytest
make lint           # ruff only
make format         # ruff --fix + black
make type           # mypy
make test           # pytest

# Run lai
python3 lai.py status
python3 lai.py auto "task" --max-iterations 3
python3 lai.py serve --port 8082

# Build gateway
make gateway-build
```

## Code Style

- Python 3.10+, 4-space indent, 100-char lines
- Ruff for linting (see `pyproject.toml`)
- Type hints encouraged but not enforced everywhere
- Docstrings for public functions

## Testing

- Tests in `tests/test_lai.py`
- Run with `make test` or `pytest tests/ -v`
- Mock external services; integration tests use real llama.cpp/gateway

## Common Patterns

### Adding a Tool
1. Add method to `ToolRegistry` in `lai.py`
2. Add entry to `get_tools_schema()`
3. Test with `python3 lai.py auto "use my tool"`

### Modifying Gateway
1. Edit `gateway-go/main.go` and `gateway-go/config.yaml`
2. `make gateway-build`
3. Restart gateway process

### Updating Model Config
- llama.cpp flags in `scripts/start-llama.sh` (to be created)
- Context: 32768, Output: 4096, GPU layers: 99

## Security Notes

- `run_command` and `python_eval` run with `shell=True` — **not a sandbox**
- AutoMode only on trusted repositories
- Gateway binds to loopback; no auth by default
- Never expose gateway/llama.cpp to public interfaces

## Documentation

- `DOCUMENTACION_REGLAS.md` — Single source of truth for rules
- `DOCS/REVISION_2026.md` — Technical review and research
- `README.md` — User-facing overview
- `CHANGELOG.md` — Version history

## Version Management

- Version in `VERSION` file and `pyproject.toml`
- `scripts/version.sh` for tagging (use with caution)
- Gateway version in `gateway-go/config.yaml` and `main.go`

---
*Generated for agent reference. Keep in sync with DOCUMENTACION_REGLAS.md.*