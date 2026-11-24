#!/usr/bin/env bash
# Stop the local llama.cpp and/or gateway processes started by lai scripts.
#
# Usage:
#   ./scripts/stop-services.sh [all|llama|gateway]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_DIR="${LAI_LOG_DIR:-$REPO_ROOT/logs}"
SERVICE="${1:-all}"

stop_from_pid_file() {
    local name="$1"
    local pid_file="$2"
    if [[ ! -f "$pid_file" ]]; then
        echo "$name is not running (no PID file)."
        return
    fi
    local pid
    pid="$(cat "$pid_file" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        echo "Stopping $name (PID $pid)..."
        kill "$pid" 2>/dev/null || true
        for _ in $(seq 1 20); do
            kill -0 "$pid" 2>/dev/null || break
            sleep 0.5
        done
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
        fi
        echo "$name stopped."
    else
        echo "$name is not running (stale PID file)."
    fi
    rm -f "$pid_file"
}

case "$SERVICE" in
    all)
        stop_from_pid_file "llama.cpp" "$LOG_DIR/llama.pid"
        stop_from_pid_file "gateway" "$LOG_DIR/gateway.pid"
        ;;
    llama)
        stop_from_pid_file "llama.cpp" "$LOG_DIR/llama.pid"
        ;;
    gateway)
        stop_from_pid_file "gateway" "$LOG_DIR/gateway.pid"
        ;;
    *)
        echo "Usage: $0 [all|llama|gateway]" >&2
        exit 2
        ;;
esac
