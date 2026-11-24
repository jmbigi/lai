#!/usr/bin/env bash
# Build and start the Go gateway with persistent logging and health-check.
# Portable: works from any checkout directory on Linux, macOS or Git Bash.
#
# Usage:
#   ./scripts/start-gateway.sh [--build] [--restart]
#
# Environment:
#   LAI_GATEWAY_PORT, LAI_GATEWAY_CONFIG, LAI_GATEWAY_BUILD=1, LAI_LOG_DIR

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
GATEWAY_DIR="$REPO_ROOT/gateway-go"

BUILD="${LAI_GATEWAY_BUILD:-0}"
RESTART=0
for arg in "$@"; do
    case "$arg" in
        --build) BUILD=1 ;;
        --restart) RESTART=1 ;;
        -h|--help)
            sed -n '2,11p' "$0"
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

PORT="${LAI_GATEWAY_PORT:-8081}"
HOST="127.0.0.1"
CONFIG="${LAI_GATEWAY_CONFIG:-$GATEWAY_DIR/config.yaml}"
LOG_DIR="${LAI_LOG_DIR:-$REPO_ROOT/logs}"
PID_FILE="$LOG_DIR/gateway.pid"
LOG_FILE="$LOG_DIR/gateway.log"

case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) EXE="gateway.exe" ;;
    *) EXE="gateway" ;;
esac
BINARY="$GATEWAY_DIR/$EXE"

mkdir -p "$LOG_DIR"

health() {
    if command -v curl >/dev/null 2>&1; then
        curl -sf --max-time 2 "http://$HOST:$PORT/health" >/dev/null 2>&1
    elif command -v wget >/dev/null 2>&1; then
        wget -q -T 2 -O /dev/null "http://$HOST:$PORT/health" >/dev/null 2>&1
    else
        return 1
    fi
}

stop_existing() {
    if [[ ! -f "$PID_FILE" ]]; then
        return
    fi
    local old_pid
    old_pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [[ -n "$old_pid" ]] && kill -0 "$old_pid" 2>/dev/null; then
        echo "Stopping existing gateway (PID $old_pid)..."
        kill "$old_pid" 2>/dev/null || true
        for _ in $(seq 1 20); do
            kill -0 "$old_pid" 2>/dev/null || break
            sleep 0.5
        done
    fi
    rm -f "$PID_FILE"
}

if health; then
    if [[ "$RESTART" -eq 0 ]]; then
        echo "Gateway is already healthy on http://$HOST:$PORT"
        exit 0
    fi
fi

stop_existing

port_in_use() {
    if command -v ss >/dev/null 2>&1; then
        ss -ltn 2>/dev/null | awk -v p=":$PORT" '$4 ~ p"$" {found=1} END {exit !found}'
    elif command -v netstat >/dev/null 2>&1; then
        netstat -an 2>/dev/null | grep -E "[:.]${PORT}[[:space:]].*LISTEN" >/dev/null 2>&1
    else
        return 1
    fi
}

if port_in_use; then
    echo "ERROR: port $PORT is already in use. Stop that process or set LAI_GATEWAY_PORT." >&2
    exit 1
fi

if [[ "$BUILD" -eq 1 || ! -f "$BINARY" ]]; then
    if ! command -v go >/dev/null 2>&1; then
        echo "ERROR: go not found. Run scripts/setup-dev.sh or install Go." >&2
        exit 1
    fi
    echo "Building gateway..."
    (cd "$GATEWAY_DIR" && go build -o "$EXE" .)
fi

if [[ ! -f "$CONFIG" ]]; then
    echo "ERROR: gateway config not found: $CONFIG" >&2
    exit 1
fi

echo "Starting gateway on $HOST:$PORT..."
nohup "$BINARY" "$CONFIG" > "$LOG_FILE" 2>&1 &
PID=$!
echo "$PID" > "$PID_FILE"
echo "gateway started (PID $PID). Logs: $LOG_FILE"

for i in $(seq 1 30); do
    if health; then
        echo "gateway healthy after ${i}s"
        exit 0
    fi
    if ! kill -0 "$PID" 2>/dev/null; then
        break
    fi
    sleep 1
done

echo "ERROR: gateway failed to become healthy" >&2
tail -n 40 "$LOG_FILE" >&2 2>/dev/null || true
kill "$PID" 2>/dev/null || true
rm -f "$PID_FILE"
exit 1
