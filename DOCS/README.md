# Documentation Index

This directory contains technical documentation for lai.

## Files

| File | Description |
|------|-------------|
| [REVISION_2026.md](REVISION_2026.md) | Comprehensive technical review, research findings, and roadmap |
| [CHANGELOG.md](CHANGELOG.md) | Detailed version history |
| [quality_comparison.md](quality_comparison.md) | Quality metrics and comparison methodology |
| [INFERENCIA_LOCAL_BAJA_GAMA.md](INFERENCIA_LOCAL_BAJA_GAMA.md) | CPU inference guide for low-end teams (Regla 11) |
| [CAMBIOS_2026-11-26.md](CAMBIOS_2026-11-26.md) | Before/after record of the 2026-11-26 changes (Regla 10) |

## External References

- [DOCUMENTACION_REGLAS.md](../DOCUMENTACION_REGLAS.md) — Single source of truth for project rules
- [README.md](../README.md) — User-facing project overview
- [AGENTS.md](../AGENTS.md) — Agent/developer reference
- [CONTRIBUTING.md](../CONTRIBUTING.md) — Contribution guidelines

## Architecture Diagrams

```
User → lai.py (CLI) → ToolRegistry → llama.cpp (8080)
                      ↓
              gateway-go (8081) → llama.cpp (8080)
                      ↓
              lai.py serve (8082) → OpenCode / OpenAI-compatible clients
```

## Service Ports

| Service | Port | Bind | Protocol |
|---------|------|------|----------|
| llama.cpp | 8080 | 127.0.0.1 | HTTP + SSE |
| gateway-go | 8081 | 127.0.0.1 | HTTP + SSE |
| Ollama | 11434 | 127.0.0.1 | HTTP |
| lai serve | 8082 | 127.0.0.1 | HTTP + SSE |

## Model Configuration

- **Model**: Qwen3-8B-Q4_K_M
- **Context**: 32768 tokens
- **GPU Layers**: 99 (`-ngl 99`)
- **Reasoning**: `--reasoning-format deepseek --reasoning-budget 1024`
- **Sampling (agent)**: temp=0.2, top_p=0.95, top_k=20, min_p=0, thinking=false

## Quality Gates

Run `make check` to execute:

1. `python3 -m py_compile lai.py`
2. `ruff check .`
3. `pytest tests/ -v`

See [CONTRIBUTING.md](../CONTRIBUTING.md) for details.