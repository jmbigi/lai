#!/usr/bin/env bash
# Start llama.cpp server with persistent logging and health-check.
# Portable: works from any checkout directory on Linux, macOS or Git Bash.
#
# Usage:
#   ./scripts/start-llama.sh [model.gguf] [--restart]
#
# Environment:
#   LAI_MODEL_PATH, LAI_LLAMA_HOST, LAI_LLAMA_PORT, LAI_CONTEXT, LAI_NGL,
#   LAI_MODEL_ALIAS, LAI_LOG_DIR, LLAMA_SERVER, LAI_LLAMA_EXTRA_ARGS,
#   LAI_LLAMA_PARALLEL (slots for prompt-cache reuse, default 1),
#   LAI_CACHE_REUSE (min chunk to reuse after a divergence, default 256),
#   LAI_MODELS_DIR (extra model directory, default: $HOME/models)
#
#   LAI_NGL=auto omits -ngl so llama.cpp fits layers and context to free VRAM
#   (recommended on 8 GB GPUs; -ngl 99 fails there).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

MODEL_ARG=""
RESTART=0
for arg in "$@"; do
    case "$arg" in
        --restart) RESTART=1 ;;
        -h|--help)
            sed -n '2,12p' "$0"
            exit 0
            ;;
        *) MODEL_ARG="$arg" ;;
    esac
done

MODEL_PATH="${MODEL_ARG:-${LAI_MODEL_PATH:-}}"
HOST="${LAI_LLAMA_HOST:-127.0.0.1}"
PORT="${LAI_LLAMA_PORT:-8080}"
CTX="${LAI_CONTEXT:-32768}"
NGL="${LAI_NGL:-auto}"
ALIAS="${LAI_MODEL_ALIAS:-qwen3-8b}"
PARALLEL="${LAI_LLAMA_PARALLEL:-1}"
CACHE_REUSE="${LAI_CACHE_REUSE:-256}"
LOG_DIR="${LAI_LOG_DIR:-$REPO_ROOT/logs}"
PID_FILE="$LOG_DIR/llama.pid"
LOG_FILE="$LOG_DIR/llama.log"

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
        echo "Stopping existing llama.cpp (PID $old_pid)..."
        kill "$old_pid" 2>/dev/null || true
        for _ in $(seq 1 30); do
            kill -0 "$old_pid" 2>/dev/null || break
            sleep 1
        done
    fi
    rm -f "$PID_FILE"
}

resolve_model() {
    if [[ -n "$MODEL_PATH" ]]; then
        if [[ ! -f "$MODEL_PATH" ]]; then
            echo "ERROR: model not found: $MODEL_PATH" >&2
            return 1
        fi
        printf '%s\n' "$MODEL_PATH"
        return 0
    fi

    # Directorios de modelos: el del repo y, si se indica, uno del usuario.
    # Se ignoran los .gguf diminutos (descargas incompletas, 0 bytes) y, si hay
    # varios, se elige el mayor: es el que el usuario quiso usar.
    local dir candidate
    for dir in "$REPO_ROOT/models" "${LAI_MODELS_DIR:-$HOME/models}"; do
        [[ -d "$dir" ]] || continue
        candidate="$(find "$dir" -maxdepth 1 -type f -name '*.gguf' -size +100M 2>/dev/null \
            | xargs -r ls -S 2>/dev/null | head -n 1 || true)"
        if [[ -n "$candidate" ]]; then
            if find "$dir" -maxdepth 1 -type f -name '*.gguf' -size -100M 2>/dev/null | grep -q .; then
                echo "Ignoring incomplete .gguf files (<100 MB) in $dir." >&2
            fi
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    if command -v ollama >/dev/null 2>&1; then
        local name modelfile from
        for name in "${OLLAMA_MODEL:-qwen3:8b-nothink}" qwen2.5-coder:7b qwen3:8b-nothink qwen2.5-coder:3b; do
            modelfile="$(ollama show --modelfile "$name" 2>/dev/null || true)"
            [[ -n "$modelfile" ]] || continue
            from="$(printf '%s\n' "$modelfile" | awk '/^[[:space:]]*FROM[[:space:]]+/{sub(/^[[:space:]]*FROM[[:space:]]+/, ""); print; exit}')"
            from="${from%\"}"
            from="${from#\"}"
            if [[ -n "$from" && -f "$from" ]]; then
                echo "Using Ollama model blob for '$name'." >&2
                printf '%s\n' "$from"
                return 0
            fi
        done
    fi

    echo "ERROR: no GGUF model found." >&2
    echo "  Put one in $REPO_ROOT/models, set LAI_MODEL_PATH, pass a path," >&2
    echo "  or run: ollama pull qwen2.5-coder:7b" >&2
    return 1
}

resolve_llama_server() {
    if [[ -n "${LLAMA_SERVER:-}" ]]; then
        if [[ -x "$LLAMA_SERVER" ]]; then
            printf '%s\n' "$LLAMA_SERVER"
            return 0
        fi
        echo "ERROR: LLAMA_SERVER is not executable: $LLAMA_SERVER" >&2
        return 1
    fi
    if command -v llama-server >/dev/null 2>&1; then
        command -v llama-server
        return 0
    fi
    if command -v llama-server.exe >/dev/null 2>&1; then
        command -v llama-server.exe
        return 0
    fi
    local pattern
    for pattern in \
        "${LOCALAPPDATA:-}/Microsoft/WinGet/Packages/ggml.llamacpp"*/llama-server.exe \
        "/usr/local/bin/llama-server" \
        "/opt/llama.cpp/bin/llama-server" \
        "$HOME/llama.cpp-bin"/*/llama-server \
        "$HOME/llama.cpp/build/bin/llama-server" \
        "$HOME/ollama-bin/lib/ollama/llama-server"; do
        if [[ -x "$pattern" ]]; then
            printf '%s\n' "$pattern"
            return 0
        fi
    done
    echo "ERROR: llama-server not found. Install llama.cpp or set LLAMA_SERVER." >&2
    return 1
}

if health; then
    if [[ "$RESTART" -eq 0 ]]; then
        echo "llama.cpp is already healthy on http://$HOST:$PORT"
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
    echo "ERROR: port $PORT is already in use. Stop that process or set LAI_LLAMA_PORT." >&2
    exit 1
fi

MODEL_PATH="$(resolve_model)"
LLAMA_SERVER_PATH="$(resolve_llama_server)"

EXTRA_ARGS=()
if [[ -n "${LAI_LLAMA_EXTRA_ARGS:-}" ]]; then
    read -r -a EXTRA_ARGS <<< "$LAI_LLAMA_EXTRA_ARGS"
fi

echo "Starting llama.cpp on $HOST:$PORT..."
echo "  model:  $MODEL_PATH"
echo "  server: $LLAMA_SERVER_PATH"
echo "  log:    $LOG_FILE"

# Flags de agente: se añaden solo si el binario los soporta, para no romper
# versiones antiguas de llama.cpp.
SERVER_HELP="$("$LLAMA_SERVER_PATH" --help 2>&1 || true)"
supports() { grep -q -- "$1" <<<"$SERVER_HELP"; }

AGENT_ARGS=()
if supports "--jinja"; then
    # Usa la plantilla de chat del modelo (tool calling nativo fiable).
    AGENT_ARGS+=(--jinja)
else
    echo "  aviso: este llama-server no soporta --jinja; el tool calling puede ser menos fiable" >&2
fi
if supports "--cache-reuse"; then
    # Reutiliza trozos del KV cache aunque el prompt diverja en el medio.
    AGENT_ARGS+=(--cache-reuse "$CACHE_REUSE")
fi
if supports "--parallel"; then
    AGENT_ARGS+=(-np "$PARALLEL")
fi

# Configuración de GPU/contexto.
#
# En GPU de 8 GB, `-ngl 99` con 32k de contexto no entra (OOM) y `auto` elige
# pocas capas (generación ~5x más lenta). Medido en este equipo con
# Qwen3-8B-Q4_K_M y RTX 3070 Laptop 8 GB:
#
#   ngl=50  ctx=18432 -> 72 t/s generación, 539 t/s prompt, 7361 MiB
#   ngl=auto ctx=32768 -> 15 t/s, 6825 MiB
#   ngl=50  ctx=24576 -> OOM
#   ngl=auto ctx=18432 -> 37 t/s, 6803 MiB
#
# Por eso, si el operador no fija LAI_NGL/LAI_CONTEXT, se prueban configuraciones
# de mayor a menor exigencia y se usa la primera que arranca. Si los fija, se
# respeta exactamente (sin sustituciones silenciosas).
CANDIDATES=()
if [[ -n "${LAI_NGL:-}" || -n "${LAI_CONTEXT:-}" ]]; then
    CANDIDATES+=("$NGL:$CTX")
    echo "  config: ngl=$NGL ctx=$CTX (explícita)"
else
    CANDIDATES+=("99:32768" "99:16384" "50:18432" "auto:16384" "auto:8192" "0:4096")
    echo "  config: automática (se prueba de mayor a menor hasta que arranque)"
fi

launch_attempt() {
    local ngl="$1" ctx="$2"
    local gpu_args=()
    if [[ "$ngl" != "auto" ]]; then
        gpu_args+=(-ngl "$ngl")
    fi
    nohup "$LLAMA_SERVER_PATH" \
        -m "$MODEL_PATH" \
        --host "$HOST" \
        --port "$PORT" \
        -c "$ctx" \
        ${gpu_args[@]+"${gpu_args[@]}"} \
        --alias "$ALIAS" \
        --cont-batching \
        --flash-attn auto \
        "${AGENT_ARGS[@]}" \
        "${EXTRA_ARGS[@]}" \
        > "$LOG_FILE" 2>&1 &
    echo $!
}

LAST_ERROR=""
for candidate in "${CANDIDATES[@]}"; do
    CAND_NGL="${candidate%%:*}"
    CAND_CTX="${candidate##*:}"
    echo "  intentando: ngl=$CAND_NGL ctx=$CAND_CTX"
    PID="$(launch_attempt "$CAND_NGL" "$CAND_CTX")"
    echo "$PID" > "$PID_FILE"

    healthy=0
    for _ in $(seq 1 90); do
        if health; then
            healthy=1
            break
        fi
        if ! kill -0 "$PID" 2>/dev/null; then
            break
        fi
        sleep 1
    done

    if [[ "$healthy" -eq 1 ]]; then
        echo "llama.cpp healthy on http://$HOST:$PORT (PID $PID)"
        echo "  gpu:    ngl=$CAND_NGL"
        echo "  ctx:    $CAND_CTX"
        echo "  log:    $LOG_FILE"
        exit 0
    fi

    LAST_ERROR="$(grep -iE 'error|out of memory|failed' "$LOG_FILE" 2>/dev/null | tail -n 3 || true)"
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
    rm -f "$PID_FILE"
    echo "  falló (ngl=$CAND_NGL ctx=$CAND_CTX); se prueba la siguiente" >&2
done

echo "ERROR: llama.cpp no arrancó con ninguna configuración." >&2
if [[ -n "$LAST_ERROR" ]]; then
    echo "$LAST_ERROR" >&2
fi
tail -n 20 "$LOG_FILE" >&2 2>/dev/null || true
exit 1
