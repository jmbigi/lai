#!/usr/bin/env bash
# Run the local development checks for lai.
# Default: cargo check, cargo test, go build and pytest.
# --full adds cargo fmt --check and cargo clippy.
#
# Usage:
#   ./scripts/dev-check.sh [--full] [--skip-rust] [--skip-go] [--skip-python]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FULL=0
SKIP_RUST=0
SKIP_GO=0
SKIP_PYTHON=0

for arg in "$@"; do
    case "$arg" in
        --full) FULL=1 ;;
        --skip-rust) SKIP_RUST=1 ;;
        --skip-go) SKIP_GO=1 ;;
        --skip-python) SKIP_PYTHON=1 ;;
        -h|--help)
            sed -n '2,7p' "$0"
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

failures=()

run_step() {
    local name="$1"
    shift
    echo ""
    echo "==> $name"
    if "$@"; then
        echo "OK: $name"
    else
        echo "FAILED: $name"
        failures+=("$name")
    fi
}

rust_cmd() { (cd "$REPO_ROOT/lai_rust" && "$@"); }
go_cmd() {
    case "$(uname -s)" in
        MINGW*|MSYS*|CYGWIN*) EXE="gateway.exe" ;;
        *) EXE="gateway" ;;
    esac
    (cd "$REPO_ROOT/gateway-go" && go build -o "$EXE" .)
}

if [[ "$SKIP_RUST" -eq 0 ]]; then
    if [[ "$FULL" -eq 1 ]]; then
        run_step "cargo fmt --check" rust_cmd cargo fmt --all -- --check
        run_step "cargo clippy" rust_cmd cargo clippy --all-targets
    fi
    run_step "cargo check --all-targets" rust_cmd cargo check --all-targets
    run_step "cargo test" rust_cmd cargo test --no-fail-fast
fi

if [[ "$SKIP_GO" -eq 0 ]]; then
    run_step "go build gateway" go_cmd
    if [[ "$FULL" -eq 1 ]]; then
        run_step "go vet" bash -c "cd '$REPO_ROOT/gateway-go' && go vet ./..."
    fi
fi

if [[ "$SKIP_PYTHON" -eq 0 ]]; then
    if [[ -x "$REPO_ROOT/.venv/bin/python" ]]; then
        VENV_PY="$REPO_ROOT/.venv/bin/python"
    elif [[ -x "$REPO_ROOT/.venv/Scripts/python.exe" ]]; then
        VENV_PY="$REPO_ROOT/.venv/Scripts/python.exe"
    else
        echo "FAILED: Python venv missing. Run scripts/setup-dev.sh." >&2
        failures+=("python venv")
        VENV_PY=""
    fi
    if [[ -n "$VENV_PY" ]]; then
        run_step "pytest" bash -c "cd '$REPO_ROOT' && '$VENV_PY' -m pytest -q"
    fi
fi

echo ""
if [[ ${#failures[@]} -eq 0 ]]; then
    echo "All checks passed."
    exit 0
fi

echo "Failed checks:"
for failure in "${failures[@]}"; do
    echo "  - $failure"
done
exit 1
