.PHONY: help check lint test format type ci ci-quick install dev-install build gateway-build clean smoke rust-check gateway-check sandbox-check swe-estimate

# Default target
help:
	@echo "Available targets:"
	@echo "  check        Run all quality checks (compile + ruff + pytest)"
	@echo "  lint         Run ruff linter"
	@echo "  format       Format code with ruff and black"
	@echo "  type         Run mypy type checker"
	@echo "  test         Run pytest"
	@echo "  install      Install package in editable mode"
	@echo "  dev-install  Install package with dev dependencies"
	@echo "  gateway-build Build Go gateway"
	@echo "  rust-check   Rust fmt + clippy + tests (implementación principal)"
	@echo "  gateway-check Build the Go gateway"
	@echo "  sandbox-check Verify bubblewrap isolation (if installed)"
	@echo "  swe-estimate Estimate SWE capability from the latest report (Regla 12)"
	@echo "  clean        Remove build artifacts and caches"

# Quality checks
check: compile lint test rust-check

# Quality gates completo (compile + lint + format + type + test + coverage + security + complexity)
quality:
	bash scripts/quality_gates.sh

quality-ci:
	bash scripts/quality_gates.sh --ci

# CI local 100% open source (sin cuentas, sin cloud, sin API keys)
ci:
	bash scripts/ci_local.sh --ci

ci-quick:
	bash scripts/ci_local.sh --quick --ci

compile:
	python3 -m py_compile lai.py

lint:
	ruff check .

format:
	ruff check --fix .
	black .

type:
	mypy lai.py tests/

test:
	python3 -m pytest tests/ -v

# Implementación principal (Rust): formato, lint y tests.
rust-check:
	cd lai_rust && cargo fmt --check
	cd lai_rust && cargo clippy --all-targets -- -D warnings
	cd lai_rust && cargo test

# Gateway Go (opcional: no lo necesita el agente para funcionar).
gateway-check:
	cd gateway-go && go build -o /tmp/lai-gateway-check .

# Estimación SWE de la última corrida evaluada (Regla 12).
# Uso: make swe-estimate RUN_ID=qwen3-8b-final
swe-estimate:
	@if [ -n "$(RUN_ID)" ]; then \
		python3 swebench/estimate.py --run-id "$(RUN_ID)"; \
	else \
		python3 swebench/estimate.py --model qwen3-8b; \
	fi

# Verifica que el sandbox aísla de verdad (solo si bubblewrap está instalado).
sandbox-check:
	@if command -v bwrap >/dev/null 2>&1; then \
		cd lai_rust && cargo test --quiet --lib sandbox ; \
		echo "sandbox: pruebas de aislamiento OK"; \
	else \
		echo "sandbox: bwrap no instalado; se omite (instala bubblewrap para probarlo)"; \
	fi

# Installation
install:
	pip install -e .

dev-install:
	pip install -e .[dev]

# Gateway
gateway-build:
	cd gateway-go && go build -o gateway

# Cleanup
clean:
	rm -rf __pycache__ .pytest_cache .ruff_cache .mypy_cache .coverage htmlcov dist build *.egg-info
	find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete 2>/dev/null || true

# Quick smoke test
smoke:
	python3 lai.py status
	python3 lai.py tools
	python3 lai.py review fib.py