#!/usr/bin/env bash
# Stress test: N random coding tasks, optionally in parallel.
#
# Usage: ./run_stress.sh [MODEL] [BASE_URL] [PARALLEL_JOBS]
#
# Verification is honest: a task only counts as PASS when the agent actually
# modified src/lib.rs AND `cargo test` passes on the result.

set -uo pipefail

MODEL="${1:-qwen3-coder-opt:latest}"
BASE_URL="${2:-http://127.0.0.1:11434/v1}"
PARALLEL="${3:-2}"
MAX_ITER="${MAX_ITER:-10}"
TASKS_LIMIT="${TASKS_LIMIT:-20}"
LAI_BIN="/home/kubuntu/lai/lai_rust/target/release/lai"
BENCH_DIR=$(mktemp -d -t lai-stress-XXXXXX)

echo "═══════════════════════════════════════════════════════════"
echo "  lai Stress Test"
echo "  Model: $MODEL"
echo "  Parallel jobs: $PARALLEL"
echo "  Workdir: $BENCH_DIR"
echo "═══════════════════════════════════════════════════════════"

OLLAMA_API="${BASE_URL%/v1}/api/tags"
for i in $(seq 1 30); do
    curl -s "$OLLAMA_API" >/dev/null && break
    [ "$i" -eq 30 ] && { echo "❌ Ollama no responde en $OLLAMA_API"; exit 1; }
    sleep 1
done
curl -s "$OLLAMA_API" | grep -q "\"$MODEL\"" || {
    echo "❌ Modelo '$MODEL' no encontrado. Disponibles:"
    curl -s "$OLLAMA_API" | jq -r '.models[].name'
    exit 1
}
echo "✅ Modelo disponible"
echo ""

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

run_one() {
    local idx="$1"
    local prompt="$2"
    local task_dir="$BENCH_DIR/task_$idx"

    mkdir -p "$task_dir/src"
    cd "$task_dir" || return 1

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

    cat > src/lib.rs << 'EOF'
pub fn placeholder() -> i32 { 42 }
#[cfg(test)] mod tests { use super::*; #[test] fn test_placeholder() { assert_eq!(placeholder(), 42); } }
EOF

    local before after start end dur verify reason
    before=$(sha256sum src/lib.rs | cut -d' ' -f1)
    start=$(date +%s%3N)
    "$LAI_BIN" --model "$MODEL" --base-url "$BASE_URL" auto "$prompt" \
        --max-iterations "$MAX_ITER" > "$task_dir/agent.log" 2>&1
    end=$(date +%s%3N)
    dur=$((end - start))
    after=$(sha256sum src/lib.rs | cut -d' ' -f1)

    if [ "$before" = "$after" ]; then
        verify="FAIL"; reason="no file changes"
    elif cargo test --quiet > "$task_dir/test.log" 2>&1; then
        verify="PASS"; reason="tests pass"
    else
        verify="FAIL"; reason="tests failed"
    fi

    printf '%s\t%s\t%s\t%s\n' "$idx" "$verify" "$dur" "$reason" > "$task_dir/result.tsv"
    echo "  [task_$idx] $verify (${dur}ms) [$reason]"
}

export -f run_one
export MODEL BASE_URL MAX_ITER LAI_BIN BENCH_DIR

TASK_COUNT=$(( ${#TASKS[@]} < TASKS_LIMIT ? ${#TASKS[@]} : TASKS_LIMIT ))
echo "🚀 Ejecutando $TASK_COUNT tareas (paralelo: $PARALLEL)..."
echo ""
WALL_START=$(date +%s)

for i in "${!TASKS[@]}"; do
    [ "$i" -ge "$TASKS_LIMIT" ] && break
    idx=$((i + 1))
    printf '%s\t%s\n' "$idx" "${TASKS[$i]}"
done | xargs -P "$PARALLEL" -I{} bash -c '
    IFS=$'"'"'\t'"'"' read -r idx prompt <<< "{}"
    run_one "$idx" "$prompt"
'

PASSED=0; FAILED=0; TOTAL_MS=0
WALL_SECONDS=$(( $(date +%s) - WALL_START ))
echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  STRESS TEST RESULTS: $MODEL"
echo "═══════════════════════════════════════════════════════════"
for f in "$BENCH_DIR"/task_*/result.tsv; do
    [ -f "$f" ] || continue
    IFS=$'\t' read -r idx verify dur reason < "$f"
    TOTAL_MS=$((TOTAL_MS + dur))
    if [ "$verify" = "PASS" ]; then PASSED=$((PASSED + 1)); else FAILED=$((FAILED + 1)); fi
    printf '  task_%-2s %-4s %7sms  %s\n' "$idx" "$verify" "$dur" "$reason"
done

TOTAL=$((PASSED + FAILED))
echo "───────────────────────────────────────────────────────────"
echo "✅ Pasaron: $PASSED"
echo "❌ Fallaron: $FAILED"
echo "📊 Total: $TOTAL"
[ "$TOTAL" -gt 0 ] && echo "📈 Pass rate: $((PASSED * 100 / TOTAL))%"
echo "⏱️  Tiempo sumado por tarea: ${TOTAL_MS}ms"
echo "🕐 Tiempo de pared: ${WALL_SECONDS}s (con $PARALLEL jobs)"

cat > "$BENCH_DIR/summary.json" << EOF
{
  "model": "$MODEL",
  "timestamp": "$(date -Iseconds)",
  "parallel_jobs": $PARALLEL,
  "total_tasks": $TOTAL,
  "passed": $PASSED,
  "failed": $FAILED,
  "sum_task_time_ms": $TOTAL_MS,
  "wall_time_s": $WALL_SECONDS
}
EOF
echo ""
echo "💾 Resultados: $BENCH_DIR/summary.json"
