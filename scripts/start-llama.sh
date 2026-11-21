#!/usr/bin/env bash
# Start llama.cpp server with persistent logging and health-check
# Usage: ./scripts/start-llama.sh [model_path]

set -euo pipefail

MODEL_PATH="${1:-/home/kubuntu/models/Qwen3-8B-Q4_K_M.gguf}"
LLAMA_DIR="/home/kubuntu/llama.cpp-bin/llama-b12567"
PORT=8080
HOST="127.0.0.1"
CTX=32768
NGL=99
LOG_DIR="/home/kubuntu/curso-ia/modelo-local/logs"
PID_FILE="$LOG_DIR/llama.pid"
LOG_FILE="$LOG_DIR/llama.log"

mkdir -p "$LOG_DIR"

# Kill existing if running on our PID file
if [[ -f "$PID_FILE" ]]; then
    OLD_PID=$(cat "$PID_FILE" 2>/dev/null || true)
    if [[ -n "$OLD_PID" ]] && kill -0 "$OLD_PID" 2>/dev/null; then
        echo "Stopping existing llama.cpp (PID $OLD_PID)..."
        kill "$OLD_PID"
        for i in {1..30}; do kill -0 "$OLD_PID" 2>/dev/null || break; sleep 1; done
    fi
fi

cd "$LLAMA_DIR"
export LD_LIBRARY_PATH=".:$LD_LIBRARY_PATH"

echo "Starting llama.cpp on $HOST:$PORT..."
nohup ./llama-server \
    -m "$MODEL_PATH" \
    --host "$HOST" \
    --port "$PORT" \
    -c "$CTX" \
    -ngl "$NGL" \
    --flash-attn on \
    --cont-batching \
    --alias qwen3-8b \
    --reasoning-format deepseek \
    --reasoning-budget 1024 \
    -ctk f8_e4m3 \
    -ctv f8_e4m3 \
    --spec-type ngram-simple \
    --spec-ngram-simple-size-n 12 \
    --spec-ngram-simple-size-m 48 \
    -b 4096 \
    -ub 512 \
    > "$LOG_FILE" 2>&1 &

PID=$!
echo "$PID" > "$PID_FILE"
echo "llama.cpp started (PID $PID). Logs: $LOG_FILE"

# Health check
for i in {1..60}; do
    if curl -sf --max-time 2 "http://$HOST:$PORT/health" >/dev/null 2>&1; then
        echo "llama.cpp healthy after ${i}s"
        exit 0
    fi
    sleep 1
done

echo "ERROR: llama.cpp failed to become healthy" >&2
exit 1