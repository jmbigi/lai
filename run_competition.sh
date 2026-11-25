#!/usr/bin/env bash
# Competition between local models on same tasks

set -euo pipefail

MODELS=("qwen3:8b" "deepseek-r1:8b" "qwen3-coder-opt:latest")
BASE_URL="http://127.0.0.1:11434/v1"
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
    
    MODEL_DIR="$BENCH_DIR/$model"
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
        
        START=$(date +%s%3N)
        OUTPUT=$(/home/kubuntu/lai/lai_rust/target/release/lai \
            --model "$model" \
            --base-url "$BASE_URL" \
            auto "$prompt" \
            --max-iterations 10 \
            2>&1)
        END=$(date +%s%3N)
        DURATION=$((END - START))
        
        if cargo check --quiet 2>/dev/null; then
            VERIFY="PASS"
        else
            VERIFY="FAIL"
        fi
        
        echo "   ⏱️  ${DURATION}ms | 🔍 $VERIFY"
        echo "$name: $VERIFY (${DURATION}ms)" >> "$MODEL_DIR/results.list"
        
        cat > "$MODEL_DIR/${name}_result.json" << EOF
{"task": "$name", "model": "$model", "duration_ms": $DURATION, "verify": "$VERIFY"}
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
    MODEL_DIR="$BENCH_DIR/$model"
    if [ -f "$MODEL_DIR/results.list" ]; then
        PASSED=$(grep -c "PASS" "$MODEL_DIR/results.list" || echo 0)
        FAILED=$(grep -c "FAIL" "$MODEL_DIR/results.list" || echo 0)
        echo "  $model: ✅ $PASSED | ❌ $FAILED"
    fi
done

echo ""
echo "📁 Detailed results in: $BENCH_DIR/"