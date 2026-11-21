.PHONY: help check lint test format type clean install dev-install build gateway-build

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
	@echo "  clean        Remove build artifacts and caches"

# Quality checks
check: compile lint test

# Quality gates completo (compile + lint + format + type + test + coverage + security + complexity)
quality:
	bash scripts/quality_gates.sh

quality-ci:
	bash scripts/quality_gates.sh --ci

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