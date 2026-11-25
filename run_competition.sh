#!/usr/bin/env bash
# Competition between local models on same tasks

set -euo pipefail

if [ -n "${MODELS_OVERRIDE:-}" ]; then
    read -r -a MODELS <<< "$MODELS_OVERRIDE"
else
    MODELS=("qwen3:8b" "deepseek-r1:8b" "qwen3-coder-opt:latest")
fi
BASE_URL="${BASE_URL_OVERRIDE:-http://127.0.0.1:11434/v1}"
TASK_TIMEOUT="${TASK_TIMEOUT:-900}"
BENCH_DIR=$(mktemp -d -t lai-comp-XXXXXX)

echo "═══════════════════════════════════════════════════════════"
echo "  lai Model Competition"
echo "  Models: ${MODELS[*]}"
echo "═══════════════════════════════════════════════════════════"

# Verify all models exist
OLLAMA_API="${BASE_URL%/v1}/api/tags"
for model in "${MODELS[@]}"; do
    if ! curl -s "$OLLAMA_API" | grep -q "\"$model\""; then
        echo "⚠️  Modelo '$model' no encontrado, saltando..."
    fi
done

# Same 3 quick tasks for all models
TASKS=(
    "add_two_numbers|Escribe una función en Rust que sume dos números y retorne el resultado, con tests."
    "factorial|Implementa factorial recursivo con manejo de overflow."
    "fix_off_by_one|Corrige el error off-by-one en este bucle: for i in 0..=arr.len() { ... }"
)

for model in "${MODELS[@]}"; do
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  Testing: $model"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    
    # Model names may contain ":" (e.g. qwen3:8b); cargo rejects paths
    # with a colon, so sanitise the directory name.
    MODEL_DIR="$BENCH_DIR/${model//:/_}"
    mkdir -p "$MODEL_DIR"
    echo 0 > "$MODEL_DIR/passed.count"
    echo 0 > "$MODEL_DIR/failed.count"
    > "$MODEL_DIR/results.list"
    
    for task_def in "${TASKS[@]}"; do
        IFS='|' read -r name prompt <<< "$task_def"
        
        task_dir="$MODEL_DIR/$name"
        mkdir -p "$task_dir"
        cd "$task_dir"
        
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
        
        echo "🎯 $model :: $name"

        BEFORE_HASH=$(sha256sum src/lib.rs | cut -d' ' -f1)

        START=$(date +%s%3N)
        set +e
        OUTPUT=$(timeout "$TASK_TIMEOUT" /home/kubuntu/lai/lai_rust/target/release/lai \
            --model "$model" \
            --base-url "$BASE_URL" \
            auto "$prompt" \
            --max-iterations 10 \
            2>&1)
        RC=$?
        set -e
        END=$(date +%s%3N)
        DURATION=$((END - START))

        AFTER_HASH=$(sha256sum src/lib.rs | cut -d' ' -f1)

        # Honest verification: file must change and tests must pass.
        if [ "$RC" -eq 124 ]; then
            VERIFY="TIMEOUT"
            REASON="exceeded ${TASK_TIMEOUT}s"
        elif [ "$BEFORE_HASH" = "$AFTER_HASH" ]; then
            VERIFY="FAIL"
            REASON="no file changes"
        elif cargo test --quiet >/dev/null 2>&1; then
            VERIFY="PASS"
            REASON="tests pass"
        else
            VERIFY="FAIL"
            REASON="tests failed"
        fi

        echo "   ⏱️  ${DURATION}ms | 🔍 $VERIFY ($REASON)"
        echo "$name: $VERIFY (${DURATION}ms) [$REASON]" >> "$MODEL_DIR/results.list"

        cat > "$MODEL_DIR/${name}_result.json" << EOF
{"task": "$name", "model": "$model", "duration_ms": $DURATION, "verify": "$VERIFY", "reason": "$REASON"}
EOF
        
        cd - > /dev/null
    done
    
    # Print model summary
    if [ -f "$MODEL_DIR/results.list" ]; then
        cat "$MODEL_DIR/results.list"
    fi
done

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  COMPETITION SUMMARY"
echo "═══════════════════════════════════════════════════════════"

for model in "${MODELS[@]}"; do
    # Model names may contain ":" (e.g. qwen3:8b); cargo rejects paths
    # with a colon, so sanitise the directory name.
    MODEL_DIR="$BENCH_DIR/${model//:/_}"
    if [ -f "$MODEL_DIR/results.list" ]; then
        PASSED=$(grep -c ': PASS ' "$MODEL_DIR/results.list" || true)
        FAILED=$(grep -c ': FAIL ' "$MODEL_DIR/results.list" || true)
        TIMEOUTS=$(grep -c ': TIMEOUT ' "$MODEL_DIR/results.list" || true)
        echo "  $model: ✅ ${PASSED:-0} | ❌ ${FAILED:-0} | ⏱️ ${TIMEOUTS:-0}"
    fi
done

echo ""
echo "📁 Detailed results in: $BENCH_DIR/"