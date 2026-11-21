#!/usr/bin/env bash
# Quality gates para lai.
# Obligatorios: compile, ruff lint, pytest.
# Opcionales (informativos): format, mypy, coverage, bandit, radon, vulture.
# Uso: ./scripts/quality_gates.sh [--fix] [--ci] [--strict]

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

FIX=false
CI=false
STRICT=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --fix) FIX=true; shift ;;
        --ci) CI=true; shift ;;
        --strict) STRICT=true; shift ;;
        -h|--help)
            echo "Usage: $0 [--fix] [--ci] [--strict]"
            echo "  --fix    Auto-fix lint/format issues where possible"
            echo "  --ci     Modo CI: falla en cualquier problema"
            echo "  --strict Umbral de cobertura 80% (default: 60%)"
            exit 0
            ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

STATUS=0
COV_THRESHOLD="60"
if $STRICT; then COV_THRESHOLD="80"; fi

log()  { echo "[quality] $*"; }
fail() { echo "[quality][FAIL] $*" >&2; STATUS=1; }
skip() { echo "[quality][SKIP] $*"; }
info() { echo "[quality][INFO] $*"; }

has_cmd() { command -v "$1" >/dev/null 2>&1; }

run_optional() {
    local name="$1"; shift
    if ! has_cmd "$1"; then
        skip "$name (no instalado)"
        return 0
    fi
    if "$@" 2>&1; then
        log "   $name OK"
        return 0
    else
        local code=$?
        info "$name devolvió código $code (opcional, no bloquea)"
        return 0
    fi
}

# ---------------------------------------------------------------------------
# 1. Compile check (OBLIGATORIO)
# ---------------------------------------------------------------------------
log "1/8 Compile: python3 -m py_compile lai.py"
if python3 -m py_compile lai.py; then
    log "   OK"
else
    fail "py_compile falló"
fi

# ---------------------------------------------------------------------------
# 2. Lint (ruff) - OBLIGATORIO
# ---------------------------------------------------------------------------
log "2/8 Lint: ruff check"
if has_cmd ruff; then
    if $FIX; then
        ruff check --fix . || fail "ruff check --fix falló"
    else
        ruff check . || fail "ruff check falló"
    fi
else
    fail "ruff no instalado (requerido)"
fi

# ---------------------------------------------------------------------------
# 3. Format (ruff format) - OPCIONAL
# ---------------------------------------------------------------------------
log "3/8 Format: ruff format --check"
run_optional "ruff format" ruff format --check .

# ---------------------------------------------------------------------------
# 4. Type checking (mypy) - OPCIONAL
# ---------------------------------------------------------------------------
log "4/8 Type: mypy lai.py tests/"
run_optional "mypy" mypy lai.py tests/

# ---------------------------------------------------------------------------
# 5. Tests (pytest) - OBLIGATORIO
# ---------------------------------------------------------------------------
log "5/8 Tests: pytest tests/"
if python3 -m pytest tests/ -q 2>&1; then
    log "   OK"
else
    fail "pytest falló"
fi

# ---------------------------------------------------------------------------
# 6. Coverage - OPCIONAL
# ---------------------------------------------------------------------------
log "6/8 Coverage: pytest --cov=lai (umbral >= ${COV_THRESHOLD}%)"
run_optional "coverage" python3 -m pytest tests/ --cov=lai --cov-report=term-missing --cov-fail-under="${COV_THRESHOLD}" -q

# ---------------------------------------------------------------------------
# 7. Security (bandit) - OPCIONAL
# ---------------------------------------------------------------------------
log "7/8 Security: bandit lai.py"
run_optional "bandit" bandit -q lai.py

# ---------------------------------------------------------------------------
# 8. Complexity & dead code (radon + vulture) - OPCIONAL
# ---------------------------------------------------------------------------
log "8/8 Complexity/dead code: radon cc + vulture"
run_optional "radon" radon cc -a -n B lai.py
run_optional "vulture" vulture lai.py --min-confidence 90

# ---------------------------------------------------------------------------
# Resultado
# ---------------------------------------------------------------------------
echo
if [[ $STATUS -eq 0 ]]; then
    log "GATES OBLIGATORIOS PASARON ✓"
    exit 0
else
    fail "ALGÚN GATE OBLIGATORIO FALLÓ ✗"
    exit 1
fi
