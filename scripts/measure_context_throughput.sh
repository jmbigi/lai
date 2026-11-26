#!/usr/bin/env bash
# Mide throughput de generacion en dos instancias de Ollama con distinto
# OLLAMA_CONTEXT_LENGTH, con el mismo prompt y el mismo modelo (Regla 11:
# una sola variable). No inventa nada: lee eval_count/eval_duration que
# devuelve /api/generate.

set -uo pipefail
BODY="${1:-/c/Users/Geoblast/AppData/Local/Temp/opencode/gen_body.json}"
MODEL="${2:-qwen2.5-coder:3b}"

measure() {
    local port="$1"
    local label="$2"
    local json
    json=$(curl -s -X POST "http://127.0.0.1:$port/api/generate" \
        -H 'Content-Type: application/json' --data-binary "@$BODY")
    local ec ed pc ctx
    ec=$(printf '%s' "$json" | grep -o '"eval_count":[0-9]*' | head -1 | cut -d: -f2)
    ed=$(printf '%s' "$json" | grep -o '"eval_duration":[0-9]*' | head -1 | cut -d: -f2)
    pc=$(printf '%s' "$json" | grep -o '"prompt_eval_count":[0-9]*' | head -1 | cut -d: -f2)
    ctx=$(curl -s "http://127.0.0.1:$port/api/ps" \
        | grep -o '"context_length":[0-9]*' | head -1 | cut -d: -f2)
    if [ -z "$ec" ] || [ -z "$ed" ] || [ "$ed" = "0" ]; then
        echo "$label: SIN DATOS (respuesta inesperada)"
        printf '%s\n' "$json" | head -c 300
        echo
        return 1
    fi
    awk -v ec="$ec" -v ed="$ed" -v pc="$pc" -v ctx="$ctx" -v l="$label" \
        'BEGIN { printf "%s: ctx=%s  tokens=%s  prompt_tokens=%s  %.2f tok/s\n", l, ctx, ec, pc, ec/(ed/1e9) }'
}

measure 11434 "contexto por defecto (:11434)"
measure 11435 "OLLAMA_CONTEXT_LENGTH=16384 (:11435)"
