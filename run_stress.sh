#!/usr/bin/env bash
# Stress test: 20 random coding tasks sequentially

set -euo pipefail

MODEL="${1:-qwen3-coder-opt:latest}"
BASE_URL="${2:-http://127.0.0.1:11434/v1}"
BENCH_DIR=$(mktemp -d -t lai-stress-XXXXXX)

echo "═══════════════════════════════════════════════════════════"
echo "  lai Stress Test"
echo "  Model: $MODEL"
echo "  20 random tasks"
echo "═══════════════════════════════════════════════════════════"

# 20 random task templates
TASKS=(
    "Implement a function that calculates the nth Fibonacci number using memoization"
    "Write a function that validates an email address using regex"
    "Create a thread-safe singleton pattern in Rust"
    "Implement a simple LRU cache with capacity limit"
    "Write a function that parses JSON and extracts a nested field"
    "Create a retry wrapper with exponential backoff for async functions"
    "Implement a binary search tree with insert, search, and delete"
    "Write a function that converts Celsius to Fahrenheit and vice versa"
    "Create a simple rate limiter using token bucket algorithm"
    "Implement a function that finds the longest common subsequence"
    "Write a CSV serializer for a struct with custom field mapping"
    "Create a simple HTTP mock server for testing"
    "Implement a priority queue using a binary heap"
    "Write a function that validates credit card numbers (Luhn algorithm)"
    "Create a simple configuration loader from environment variables"
    "Implement a function that generates UUID v4"
    "Write a function that computes SHA-256 hash of a file"
    "Create a simple event emitter / observer pattern"
    "Implement a function that checks if a string is a palindrome"
    "Write a function that merges two sorted vectors into one sorted vector"
)

echo 0 > "$BENCH_DIR/passed.count"
echo 0 > "$BENCH_DIR/failed.count"
> "$BENCH_DIR/results.list"

for i in "${!TASKS[@]}"; do
    task_name="task_$((i+1))"
    prompt="${TASKS[i]}"
    task_dir="$BENCH_DIR/$task_name"
    
    mkdir -p "$task_dir"
    cd "$task_dir"
    
    cat > Cargo.toml << 'EOF'
[package]
name = "stress_task"
version = "0.1.0"
edition = "2021"
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
thiserror = "1.0"
rand = "0.8"
EOF
    mkdir -p src
    cat > src/lib.rs << 'EOF'
pub fn placeholder() -> i32 { 42 }
#[cfg(test)] mod tests { use super::*; #[test] fn test_placeholder() { assert_eq!(placeholder(), 42); } }
EOF
    
    echo "🎯 [$((i+1))/20] ${prompt:0:60}..."
    
    START=$(date +%s%3N)
    OUTPUT=$(/home/kubuntu/lai/lai_rust/target/release/lai \
        --model "$MODEL" \
        --base-url "$BASE_URL" \
        auto "$prompt" \
        --max-iterations 10 \
        2>&1)
    END=$(date +%s%3N)
    DURATION=$((END - START))
    
    if cargo check --quiet 2>/dev/null; then
        VERIFY="PASS"
        echo "$(( $(cat "$BENCH_DIR/passed.count") + 1 ))" > "$BENCH_DIR/passed.count"
    else
        VERIFY="FAIL"
        echo "$(( $(cat "$BENCH_DIR/failed.count") + 1 ))" > "$BENCH_DIR/failed.count"
    fi
    
    echo "   ⏱️  ${DURATION}ms | 🔍 $VERIFY"
    echo "$task_name: $VERIFY (${DURATION}ms)" >> "$BENCH_DIR/results.list"
    
    cat > "$BENCH_DIR/${task_name}_result.json" << EOF
{"task": "$task_name", "duration_ms": $DURATION, "verify": "$VERIFY", "prompt": "$prompt"}
EOF
    
    cd - > /dev/null
done

PASSED=$(cat "$BENCH_DIR/passed.count")
FAILED=$(cat "$BENCH_DIR/failed.count")
TOTAL=${#TASKS[@]}

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  STRESS TEST RESULTS: $MODEL"
echo "═══════════════════════════════════════════════════════════"
echo "✅ Pasaron: $PASSED"
echo "❌ Fallaron: $FAILED"
echo "📊 Total: $TOTAL"
echo "📈 Pass rate: $(( PASSED * 100 / TOTAL ))%"

cat "$BENCH_DIR/results.list"

echo ""
echo "📁 Results in: $BENCH_DIR/"