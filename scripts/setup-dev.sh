#!/usr/bin/env bash
# Install and prepare the lai development environment on Linux or macOS.
# Run as your normal user; the script uses sudo only for system packages.
#
# Usage:
#   ./scripts/setup-dev.sh [--run-tests] [--skip-build]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RUN_TESTS=0
SKIP_BUILD=0

for arg in "$@"; do
    case "$arg" in
        --run-tests) RUN_TESTS=1 ;;
        --skip-build) SKIP_BUILD=1 ;;
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

need() { command -v "$1" >/dev/null 2>&1; }

if [[ ${EUID:-$(id -u)} -eq 0 ]]; then
    SUDO=""
    echo "Warning: running as root. The .venv and build outputs will be root-owned."
    echo "         Prefer running this script as your normal user."
else
    SUDO="sudo"
fi

install_base_packages() {
    if need apt-get; then
        $SUDO apt-get update -y
        $SUDO apt-get install -y git curl cmake build-essential pkg-config libssl-dev python3 python3-venv python3-pip
    elif need dnf; then
        $SUDO dnf install -y git curl cmake gcc gcc-c++ make openssl-devel python3 python3-pip
    elif need pacman; then
        $SUDO pacman -Sy --noconfirm git curl cmake base-devel openssl python python-pip
    elif need brew; then
        brew install git curl cmake python
    else
        echo "No supported package manager found. Install git, curl, cmake, a C toolchain and Python 3 manually." >&2
        return 1
    fi
}

install_go() {
    if need brew; then
        brew install go
    elif need apt-get; then
        $SUDO apt-get install -y golang-go
    elif need dnf; then
        $SUDO dnf install -y golang
    elif need pacman; then
        $SUDO pacman -S --noconfirm go
    else
        echo "Install Go manually: https://go.dev/dl/" >&2
        return 1
    fi
}

echo "lai development setup"
echo "Repository: $REPO_ROOT"
echo ""

install_base_packages

if ! need go; then
    echo "Installing Go..."
    install_go
fi

if ! need cargo; then
    echo "Installing Rust (rustup)..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
fi

if ! need llama-server; then
    echo "WARNING: llama-server not found." >&2
    echo "         Install llama.cpp and put llama-server on PATH, or set LLAMA_SERVER." >&2
    echo "         The gateway and the rest of the toolchain can still be built." >&2
fi

if ! need ollama; then
    echo "NOTE: Ollama is optional. Install it from https://ollama.com if you want an alternative backend."
fi

PYTHON_BIN="${PYTHON:-python3}"
if ! need "$PYTHON_BIN"; then
    echo "ERROR: $PYTHON_BIN not found." >&2
    exit 1
fi

if [[ -x "$REPO_ROOT/.venv/bin/python" ]]; then
    VENV_PY="$REPO_ROOT/.venv/bin/python"
elif [[ -x "$REPO_ROOT/.venv/Scripts/python.exe" ]]; then
    VENV_PY="$REPO_ROOT/.venv/Scripts/python.exe"
else
    echo "Creating .venv..."
    "$PYTHON_BIN" -m venv "$REPO_ROOT/.venv"
    if [[ -x "$REPO_ROOT/.venv/bin/python" ]]; then
        VENV_PY="$REPO_ROOT/.venv/bin/python"
    else
        VENV_PY="$REPO_ROOT/.venv/Scripts/python.exe"
    fi
fi

echo "Installing Python dependencies..."
"$VENV_PY" -m pip install --upgrade pip setuptools wheel
"$VENV_PY" -m pip install -r "$REPO_ROOT/requirements-dev.txt"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    echo "Building gateway..."
    case "$(uname -s)" in
        MINGW*|MSYS*|CYGWIN*) EXE="gateway.exe" ;;
        *) EXE="gateway" ;;
    esac
    (cd "$REPO_ROOT/gateway-go" && go build -o "$EXE" .)

    echo "Building Rust CLI..."
    (cd "$REPO_ROOT/lai_rust" && cargo build)
fi

if [[ "$RUN_TESTS" -eq 1 ]]; then
    echo "Running Rust tests..."
    (cd "$REPO_ROOT/lai_rust" && cargo test --no-fail-fast)

    echo "Running Python tests..."
    (cd "$REPO_ROOT" && "$VENV_PY" -m pytest -q)
fi

echo ""
echo "Setup complete."
echo "  cargo:  $(cargo --version 2>/dev/null || echo missing)"
echo "  go:     $(go version 2>/dev/null || echo missing)"
echo "  python: $("$VENV_PY" --version 2>/dev/null || echo missing)"
echo ""
echo "Start the local stack:  ./scripts/start-services.sh"
echo "Check everything:       ./scripts/dev-check.sh"
