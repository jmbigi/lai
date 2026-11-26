#!/usr/bin/env bash
# Competition between local models on the same tasks.
#
# Usage: ./run_competition.sh
#   MODELS_OVERRIDE="qwen2.5-coder:3b qwen2.5-coder:7b" ./run_competition.sh
#   BASE_URL_OVERRIDE=http://127.0.0.1:11434/v1 ./run_competition.sh
#   LAI_BIN=/path/to/lai ./run_competition.sh
#
# Verification is honest: a task counts as PASS only when the agent actually
# modified src/lib.rs AND `cargo test` passes on the result.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LAI_BIN="${LAI_BIN:-$SCRIPT_DIR/lai_rust/target/release/lai}"
[ -x "$LAI_BIN" ] || LAI_BIN="${LAI_BIN}.exe"
if [ ! -x "$LAI_BIN" ]; then
    echo "ERROR: no se encuentra el binario lai en $LAI_BIN"
    echo "       compila con: cd lai_rust && cargo build --release"
    echo "       o define LAI_BIN=/ruta/al/binario"
    exit 1
fi

if [ -n "${MODELS_OVERRIDE:-}" ]; then
    read -r -a MODELS <<< "$MODELS_OVERRIDE"
else
    MODELS=("qwen2.5-coder:3b" "qwen2.5-coder:7b")
fi
BASE_URL="${BASE_URL_OVERRIDE:-http://127.0.0.1:11434/v1}"
TASK_TIMEOUT="${TASK_TIMEOUT:-900}"
BENCH_DIR=$(mktemp -d -t lai-comp-XXXXXX)

echo "======================================================"
echo "  lai Model Competition"
echo "  Models: ${MODELS[*]}"
echo "  Binario: $LAI_BIN"
echo "======================================================"

# Verify all models exist (without jq: grep over the raw JSON is enough).
OLLAMA_API="${BASE_URL%/v1}/api/tags"
if ! curl -s --max-time 5 "$OLLAMA_API" > /dev/null; then
    echo "ERROR: Ollama no responde en $OLLAMA_API"
    exit 1
fi
TAGS=$(curl -s "$OLLAMA_API")
RUNNABLE=()
for model in "${MODELS[@]}"; do
    if echo "$TAGS" | grep -q "\"$model\""; then
        RUNNABLE+=("$model")
    else
        echo "AVISO: modelo '$model' no encontrado, se omite."
    fi
done
if [ "${#RUNNABLE[@]}" -eq 0 ]; then
    echo "ERROR: ninguno de los modelos indicados esta disponible."
    exit 1
fi
MODELS=("${RUNNABLE[@]}")

# Same 3 quick tasks for all models
TASKS=(
    "add_two_numbers|Escribe una funcion en Rust que sume dos numeros y retorne el resultado, con tests."
    "factorial|Implementa factorial recursivo con manejo de overflow."
    "fix_off_by_one|Corrige el error off-by-one en este bucle: for i in 0..=arr.len() { ... }"
)

for model in "${MODELS[@]}"; do
    echo ""
    echo "------------------------------------------------------"
    echo "  Testing: $model"
    echo "------------------------------------------------------"

    # Model names may contain ":" (e.g. qwen3:8b); cargo rejects paths
    # with a colon, so sanitise the directory name.
    MODEL_DIR="$BENCH_DIR/${model//:/_}"
    mkdir -p "$MODEL_DIR"
    > "$MODEL_DIR/results.list"

    for task_def in "${TASKS[@]}"; do
        IFS='|' read -r name prompt <<< "$task_def"

        task_dir="$MODEL_DIR/$name"
        mkdir -p "$task_dir"
        cd "$task_dir" || continue

        # Init project
        cat > Cargo.toml << 'EOF'
[package]
name = "bench_task"
version = "0.1.0"
edition = "2021"
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
thiserror = "1.0"
EOF
        mkdir -p src
        cat > src/lib.rs << 'EOF'
pub fn add(a: i32, b: i32) -> i32 { a + b }
#[cfg(test)] mod tests { use super::*; #[test] fn test_add() { assert_eq!(add(2, 3), 5); } }
EOF

        echo ">>> $model :: $name"

        BEFORE_HASH=$(sha256sum src/lib.rs | cut -d' ' -f1)

        START=$(date +%s%3N)
        timeout "$TASK_TIMEOUT" "$LAI_BIN" \
            --model "$model" \
            --base-url "$BASE_URL" \
            auto "$prompt" \
            --max-iterations 10 \
            > agent.log 2>&1
        RC=$?
        END=$(date +%s%3N)
        DURATION=$((END - START))

        AFTER_HASH=$(sha256sum src/lib.rs | cut -d' ' -f1)

        # Honest verification: file must change and tests must pass.
        if [ "$RC" -eq 124 ]; then
            VERIFY="TIMEOUT"
            REASON="exceeded ${TASK_TIMEOUT}s"
        elif [ "$BEFORE_HASH" = "$AFTER_HASH" ]; then
            VERIFY="FAIL"
            REASON="no file changes (agent rc=$RC)"
        elif cargo test --quiet > test.log 2>&1; then
            VERIFY="PASS"
            REASON="tests pass"
        else
            VERIFY="FAIL"
            REASON="tests failed"
        fi

        echo "   ${DURATION}ms | $VERIFY ($REASON)"
        echo "$name: $VERIFY (${DURATION}ms) [$REASON] agent_rc=$RC" >> "$MODEL_DIR/results.list"

        cat > "$MODEL_DIR/${name}_result.json" << EOF
{"task": "$name", "model": "$model", "duration_ms": $DURATION, "agent_rc": $RC, "verify": "$VERIFY", "reason": "$REASON"}
EOF

        cd - > /dev/null || exit 1
    done

    # Print model summary
    if [ -f "$MODEL_DIR/results.list" ]; then
        cat "$MODEL_DIR/results.list"
    fi
done

echo ""
echo "======================================================"
echo "  COMPETITION SUMMARY"
echo "======================================================"

for model in "${MODELS[@]}"; do
    MODEL_DIR="$BENCH_DIR/${model//:/_}"
    if [ -f "$MODEL_DIR/results.list" ]; then
        PASSED=$(grep -c ': PASS ' "$MODEL_DIR/results.list" || true)
        FAILED=$(grep -c ': FAIL ' "$MODEL_DIR/results.list" || true)
        TIMEOUTS=$(grep -c ': TIMEOUT ' "$MODEL_DIR/results.list" || true)
        echo "  $model: PASS=${PASSED:-0} FAIL=${FAILED:-0} TIMEOUT=${TIMEOUTS:-0}"
    fi
done

echo ""
echo "Resultados detallados en: $BENCH_DIR/"
