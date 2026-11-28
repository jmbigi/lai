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
| [CAMBIOS_2026-11-26_PENDIENTES.md](CAMBIOS_2026-11-26_PENDIENTES.md) | Before/after record of the Docker-free pending work: offline tests, docs fixes (Regla 10) |
| [CAMBIOS_2026-11-26_MEJORAS2.md](CAMBIOS_2026-11-26_MEJORAS2.md) | Before/after record: Rule 4 guard, tool-call parser fix, fmt/clippy, scripts, measurements (Regla 10) |
| [CAMBIOS_2026-11-26_DOCKER.md](CAMBIOS_2026-11-26_DOCKER.md) | Before/after record: Docker unblocked (Desktop 4.22→4.100), dataset download without dependencies, container networking (Regla 10) |
| [CAMBIOS_2026-11-27.md](CAMBIOS_2026-11-27.md) | Before/after record: LLM client, tools, policy, MCP server and lint review (Regla 10) |
| [CAMBIOS_2026-11-27_MEJORAS.md](CAMBIOS_2026-11-27_MEJORAS.md) | Before/after record: `bash` profile, context budget, `edit`, telemetry deadlock (Regla 10) |
| [CAMBIOS_2026-11-27_SWEBENCH.md](CAMBIOS_2026-11-27_SWEBENCH.md) | Before/after record: llama.cpp GPU run, patch application review and fixes (Regla 10) |
| [CAMBIOS_2026-11-28.md](CAMBIOS_2026-11-28.md) | Before/after record: manifest guard, TUI without TTY, benchmark pending items (Regla 10) |
| [CAMBIOS_2026-11-28_CI_HIGIENE.md](CAMBIOS_2026-11-28_CI_HIGIENE.md) | Before/after record: repository hygiene, Rust in local CI, docs truth pass (Regla 10) |
| [ADR-0001-rama-fase-0-1-harness.md](ADR-0001-rama-fase-0-1-harness.md) | Decision: rescue sandbox + contracts from the old branch, archive the rest |
| [CAMBIOS_2026-11-28_SANDBOX.md](CAMBIOS_2026-11-28_SANDBOX.md) | Before/after record: bubblewrap sandbox, contracts layer, `apply_patch` manifest guard (Regla 10) |
| [PLAN_MEJORAS_2026-11-28.md](PLAN_MEJORAS_2026-11-28.md) | Improvement plan after the sandbox work |
| [CAMBIOS_2026-11-28_MEJORAS2.md](CAMBIOS_2026-11-28_MEJORAS2.md) | Before/after record: sandbox diagnostics, network opt-in, shell manifest guard, command allowlist (Regla 10) |

## External References

- [DOCUMENTACION_REGLAS.md](../DOCUMENTACION_REGLAS.md) — Single source of truth for project rules
- [README.md](../README.md) — User-facing project overview
- [AGENTS.md](../AGENTS.md) — Agent/developer reference
- [CONTRIBUTING.md](../CONTRIBUTING.md) — Contribution guidelines
- [BENCHMARKS.md](../BENCHMARKS.md) — Measured results and declared limitations

## Architecture (current)

The primary implementation is the Rust CLI. The Python CLI is legacy and is
kept in maintenance mode; the Go gateway is optional.

```
User → lai (Rust CLI) ──────────────────────▶ llama.cpp (8080)
                    └──▶ gateway-go (8081) ──▶ llama.cpp (8080)

Legacy: lai.py (CLI) ──▶ ToolRegistry ──▶ llama.cpp (8080)
        lai.py serve (8082) ──▶ OpenCode / [OI]-compatible clients
```

> The Rust CLI does **not** include `serve`, `review`, `fix`, `search` or
> `project`; those remain in the legacy Python CLI. Do not assume parity.

## Service Ports

| Service | Port | Bind | Protocol |
|---------|------|------|----------|
| llama.cpp | 8080 | 127.0.0.1 | HTTP + SSE |
| gateway-go | 8081 | 127.0.0.1 | HTTP + SSE |
| Ollama | 11434 | 127.0.0.1 | HTTP |
| lai serve (legacy) | 8082 | 127.0.0.1 | HTTP + SSE |

## Model Configuration

Defaults in `scripts/start-llama.sh` (override with `LAI_CONTEXT`, `LAI_NGL`):

- **Model**: Qwen3-8B-Q4_K_M
- **Context**: 32768 tokens
- **GPU layers**: 99 (`-ngl 99`) — this assumes the model fits in VRAM.
  On an 8 GB GPU the measured run used `-ngl 50` (see `BENCHMARKS.md`).
- **Reasoning**: `--reasoning-format deepseek --reasoning-budget 1024`
- **Sampling (agent)**: temp=0.2, top_p=0.95, top_k=20, min_p=0, thinking=false

## Quality Gates

Local CI covers both implementations:

```bash
make check        # Python compile + ruff + pytest + Rust fmt/clippy/test
make ci           # full local pipeline (scripts/ci_local.sh --ci)
make quality      # extended quality gates (coverage, security, complexity)
make gateway-check
```

The Rust steps require `cargo`; in `--ci` mode a missing toolchain is a failure,
because Rust is the primary implementation. `go` is optional and only builds the
gateway.
