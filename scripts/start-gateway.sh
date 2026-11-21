#!/usr/bin/env bash
# Start gateway-go with persistent logging and health-check

set -euo pipefail

GATEWAY_DIR="/home/kubuntu/curso-ia/modelo-local/gateway-go"
CONFIG="$GATEWAY_DIR/config.yaml"
PORT=8081
HOST="127.0.0.1"
LOG_DIR="/home/kubuntu/curso-ia/modelo-local/logs"
PID_FILE="$LOG_DIR/gateway.pid"
LOG_FILE="$LOG_DIR/gateway.log"

mkdir -p "$LOG_DIR"

if [[ -f "$PID_FILE" ]]; then
    OLD_PID=$(cat "$PID_FILE" 2>/dev/null || true)
    if [[ -n "$OLD_PID" ]] && kill -0 "$OLD_PID" 2>/dev/null; then
        echo "Stopping existing gateway (PID $OLD_PID)..."
        kill "$OLD_PID"
        for i in {1..10}; do kill -0 "$OLD_PID" 2>/dev/null || break; sleep 0.5; done
    fi
fi

cd "$GATEWAY_DIR"

echo "Building gateway..."
go build -o gateway

echo "Starting gateway on $HOST:$PORT..."
nohup ./gateway "$CONFIG" > "$LOG_FILE" 2>&1 &

PID=$!
echo "$PID" > "$PID_FILE"
echo "gateway started (PID $PID). Logs: $LOG_FILE"

for i in {1..30}; do
    if curl -sf --max-time 2 "http://$HOST:$PORT/health" >/dev/null 2>&1; then
        echo "gateway healthy after ${i}s"
        exit 0
    fi
    sleep 1
done

echo "ERROR: gateway failed to become healthy" >&2
exit 1