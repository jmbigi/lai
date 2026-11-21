# Contributing to lai

Thank you for considering a contribution! Please read this guide before submitting.

## Code of Conduct

By participating, you agree to abide by our [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/yourname/lai.git`
3. Create a branch: `git checkout -b feature/your-feature`
4. Install dev dependencies: `make dev-install`
5. Make your changes
6. Run quality checks: `make check`
7. Commit and push
8. Open a Pull Request

## Development Setup

```bash
# Install in editable mode with dev deps
make dev-install

# Build the Go gateway
make gateway-build

# Start local services (llama.cpp, gateway, ollama)
# See scripts/start-llama.sh and scripts/start-gateway.sh
```

## Quality Standards

All changes must pass:

- `python3 -m py_compile lai.py` — syntax check
- `ruff check .` — linting
- `pytest tests/ -v` — tests
- `mypy lai.py tests/` — type checking (non-blocking)

Run all with `make check`.

## Code Style

- Python 3.10+, 4-space indent, max 100 chars
- Ruff + Black formatting (see `pyproject.toml`)
- Type hints for new public functions
- Docstrings for modules, classes, and public methods

## Commit Messages

Use conventional commits:

```
feat: add new tool for X
fix: handle edge case in Y
docs: update README for Z
refactor: simplify ToolRegistry
test: add test for grep case-insensitive
```

## Pull Request Checklist

- [ ] `make check` passes
- [ ] Tests added/updated for new functionality
- [ ] Documentation updated (README, AGENTS.md, CHANGELOG.md)
- [ ] No hardcoded secrets or credentials
- [ ] Changes are minimal and focused

## Reporting Bugs

Use the issue template. Include:

- lai version (`python3 lai.py --version`)
- OS and Python version
- Steps to reproduce
- Expected vs actual behavior
- Relevant logs (`logs/llama.log`, `logs/gateway.log`)

## Security

See [SECURITY.md](SECURITY.md) for vulnerability reporting.

## License

By contributing, you agree that your contributions will be licensed under GPL-3.0-or-later.