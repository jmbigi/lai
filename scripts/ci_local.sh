#!/usr/bin/env bash
# =============================================================================
# CI Local para lai - 100% open source, sin cuentas, sin cloud, sin API keys
#
# Ejecuta todo el pipeline de calidad localmente. No requiere conexión a
# internet ni cuentas en GitHub/GitLab/Bitbucket.
#
# Uso:
#   ./scripts/ci_local.sh          # Ejecuta todo el pipeline
#   ./scripts/ci_local.sh --quick  # Solo checks obligatorios (compile, lint, test)
#   ./scripts/ci_local.sh --ci     # Modo CI: falla en el primer error
# =============================================================================

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

QUICK=false
CI_MODE=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --quick) QUICK=true; shift ;;
        --ci) CI_MODE=true; shift ;;
        -h|--help)
            echo "Uso: $0 [--quick] [--ci]"
            echo "  --quick  Solo checks obligatorios (compile, lint, test)"
            echo "  --ci     Modo CI: aborta en el primer error"
            exit 0
            ;;
        *) echo "Opción desconocida: $1" >&2; exit 1 ;;
    esac
done

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0
OPTIONAL_PASS_COUNT=0
OPTIONAL_FAIL_COUNT=0

log()    { echo -e "${BLUE}[ci]${NC} $*"; }
pass()   { echo -e "${GREEN}[OK]${NC} $*"; PASS_COUNT=$((PASS_COUNT + 1)); }
fail()   { echo -e "${RED}[FAIL]${NC} $*"; FAIL_COUNT=$((FAIL_COUNT + 1)); }
skip()   { echo -e "${YELLOW}[SKIP]${NC} $*"; SKIP_COUNT=$((SKIP_COUNT + 1)); }

has_cmd() { command -v "$1" >/dev/null 2>&1; }

run_step() {
    local name="$1"; shift
    log "Ejecutando: $name"
    if "$@" 2>&1; then
        pass "$name"
        return 0
    else
        local code=$?
        fail "$name (código de salida: $code)"
        return 1
    fi
}

run_optional() {
    local name="$1"; shift
    if ! has_cmd "$1"; then
        skip "$name (no instalado)"
        return 0
    fi
    log "Ejecutando: $name (opcional)"
    if "$@" 2>&1; then
        echo -e "${GREEN}[OK]${NC} $name"
        OPTIONAL_PASS_COUNT=$((OPTIONAL_PASS_COUNT + 1))
        return 0
    else
        local code=$?
        echo -e "${RED}[FAIL]${NC} $name (código de salida: $code)"
        OPTIONAL_FAIL_COUNT=$((OPTIONAL_FAIL_COUNT + 1))
        return 0
    fi
}

# 1. Compile check (OBLIGATORIO)
run_step "Compile: python3 -m py_compile lai.py" \
    python3 -m py_compile lai.py || { if $CI_MODE; then exit 1; fi; }

# 2. Lint (ruff) - OBLIGATORIO
if has_cmd ruff; then
    run_step "Lint: ruff check lai.py tests/" \
        ruff check lai.py tests/ || { if $CI_MODE; then exit 1; fi; }
else
    fail "ruff no instalado (requerido)"
    if $CI_MODE; then exit 1; fi
fi

# 3. Tests (pytest) - OBLIGATORIO
run_step "Tests: pytest tests/" \
    python3 -m pytest tests/ -q || { if $CI_MODE; then exit 1; fi; }

if $QUICK; then
    echo ""
    echo "============================================================"
    echo "  CI QUICK COMPLETADO"
    echo "  Pasaron: $PASS_COUNT  Fallaron: $FAIL_COUNT  Omitidos: $SKIP_COUNT"
    echo "============================================================"
    exit $([ $FAIL_COUNT -eq 0 ] && echo 0 || echo 1)
fi

# 4. Format (ruff format) - OPCIONAL
run_optional "Format: ruff format --check" ruff format --check lai.py tests/

# 5. Format (black) - OPCIONAL
run_optional "Format: black --check" python3 -m black --check lai.py tests/

# 6. Type checking (mypy) - OPCIONAL
run_optional "Type: mypy lai.py tests/" python3 -m mypy lai.py tests/

# 7. Coverage - OPCIONAL
run_optional "Coverage: pytest --cov=lai --cov-fail-under=26" \
    python3 -m pytest tests/ --cov=lai --cov-report=term-missing --cov-fail-under=26 -q

# 8. Security (bandit) - OPCIONAL
run_optional "Security: bandit lai.py" python3 -m bandit -q -c .bandit lai.py

# 9. Complexity (radon) - OPCIONAL
run_optional "Complexity: radon cc" radon cc -a -n B lai.py

# 10. Dead code (vulture) - OPCIONAL
run_optional "Dead code: vulture" vulture lai.py --min-confidence 90

# 11. Stress test rápido (opcional)
if curl -s --max-time 2 http://127.0.0.1:8081/health >/dev/null 2>&1; then
    run_optional "Stress: concurrency sweep" \
        python3 scripts/stress_test.py --suite concurrency --levels 1,4,8 --total 12 --trials 1 --concurrency 8
else
    skip "Stress test (gateway no responde en :8081)"
fi

echo ""
echo "============================================================"
echo "  CI LOCAL COMPLETADO"
echo "============================================================"
echo "  Obligatorios - Pasaron: $PASS_COUNT  Fallaron: $FAIL_COUNT"
echo "  Opcionales   - Pasaron: $OPTIONAL_PASS_COUNT  Fallaron: $OPTIONAL_FAIL_COUNT  Omitidos: $SKIP_COUNT"
echo "============================================================"

if [[ $FAIL_COUNT -eq 0 ]]; then
    echo -e "${GREEN}TODOS LOS CHECKS OBLIGATORIOS PASARON ✓${NC}"
    exit 0
else
    echo -e "${RED}ALGUNOS CHECKS OBLIGATORIOS FALLARON ✗${NC}"
    exit 1
fi
