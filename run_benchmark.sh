#!/usr/bin/env bash
# Real SWE-bench style benchmark for lai
# Runs coding tasks against local models via ollama

set -euo pipefail

MODEL="${1:-qwen3-coder-opt}"
BASE_URL="${2:-http://127.0.0.1:11434/v1}"
QUICK="${3:-false}"

echo "═══════════════════════════════════════════════════════════"
echo "  lai SWE-bench Benchmark"
echo "  Model: $MODEL"
echo "  Base URL: $BASE_URL"
echo "  Quick mode: $QUICK"
echo "═══════════════════════════════════════════════════════════"

# Check ollama is running
OLLAMA_API="${BASE_URL%/v1}/api/tags"
echo "🔍 Verificando Ollama en $OLLAMA_API..."

for i in {1..30}; do
    if curl -s "$OLLAMA_API" > /dev/null; then
        echo "✅ Ollama listo"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "❌ Ollama no responde en $OLLAMA_API tras 30s"
        echo "   Ejecuta: ollama serve"
        exit 1
    fi
    sleep 1
done

if ! curl -s "$OLLAMA_API" | grep -q "\"$MODEL\""; then
    echo "❌ Modelo '$MODEL' no encontrado en ollama"
    echo "   Modelos disponibles:"
    curl -s "$OLLAMA_API" | jq -r '.models[].name'
    exit 1
fi

echo "✅ Ollama OK, modelo '$MODEL' disponible"

# Build lai if needed
cd /home/kubuntu/lai/lai_rust
cargo build --release --quiet 2>/dev/null

# Create a temporary directory for benchmark
BENCH_DIR=$(mktemp -d -t lai-bench-XXXXXX)
echo "📁 Directorio de benchmark: $BENCH_DIR"

# Global counters (use files to persist across subshells)
echo 0 > "$BENCH_DIR/passed.count"
echo 0 > "$BENCH_DIR/failed.count"
echo 0 > "$BENCH_DIR/total_time.count"
> "$BENCH_DIR/results.list"

# Function to run a single task
run_task() {
    local task_name="$1"
    local prompt="$2"
    local task_dir="$BENCH_DIR/$task_name"
    
    mkdir -p "$task_dir"
    cd "$task_dir"
    
    # Initialize minimal Rust project
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
// Benchmark task library

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_add() { assert_eq!(add(2, 3), 5); }
}
EOF
    
    echo "🎯 Task: $task_name"
    echo "   Prompt: ${prompt:0:80}..."
    
    # Run lai auto IN the task directory
    START=$(date +%s%3N)
    OUTPUT=$(/home/kubuntu/lai/lai_rust/target/release/lai \
        --model "$MODEL" \
        --base-url "$BASE_URL" \
        auto "$prompt" \
        --max-iterations 10 \
        2>&1)
    END=$(date +%s%3N)
    DURATION=$((END - START))
    
    # Check if cargo check passes
    if cargo check --quiet 2>/dev/null; then
        VERIFY="PASS"
        echo "$(( $(cat "$BENCH_DIR/passed.count") + 1 ))" > "$BENCH_DIR/passed.count"
    else
        VERIFY="FAIL"
        echo "$(( $(cat "$BENCH_DIR/failed.count") + 1 ))" > "$BENCH_DIR/failed.count"
    fi
    
    # Update total time
    echo "$(( $(cat "$BENCH_DIR/total_time.count") + DURATION ))" > "$BENCH_DIR/total_time.count"
    
    echo "   ⏱️  ${DURATION}ms | 🔍 $VERIFY"
    echo "$task_name: $VERIFY (${DURATION}ms)" >> "$BENCH_DIR/results.list"
    
    # Save individual result
    cat > "$BENCH_DIR/${task_name}_result.json" << EOF
{
    "task": "$task_name",
    "duration_ms": $DURATION,
    "verify": "$VERIFY",
    "output_length": ${#OUTPUT}
}
EOF
    
    cd - > /dev/null
}

# Define tasks based on mode
if [ "$QUICK" = "true" ]; then
    TASKS=(
        "add_two_numbers|Escribe una función en Rust que sume dos números y retorne el resultado, con tests."
        "factorial|Implementa factorial recursivo con manejo de overflow."
        "fix_off_by_one|Corrige el error off-by-one en este bucle: for i in 0..=arr.len() { ... }"
    )
else
    TASKS=(
        "add_two_numbers|Escribe una función en Rust que sume dos números y retorne el resultado, con tests."
        "factorial|Implementa factorial recursivo con manejo de overflow."
        "fix_off_by_one|Corrige el error off-by-one en este bucle: for i in 0..=arr.len() { ... }"
        "csv_parser|Implementa un parser CSV que maneje campos citados, comillas escapadas y delimitadores variables."
        "async_http_client|Crea un cliente HTTP async con retry logic, timeout y connection pooling usando reqwest/tokio."
        "refactor_large_function|Refactoriza una función de 200 líneas en unidades pequeñas y testeables."
        "design_plugin_system|Diseña un sistema de plugins con carga dinámica, inyección de dependencias y hot reload."
        "async_state_machine|Implementa una state machine async para consenso distribuido con leader election."
        "memory_safe_ffi|Crea bindings seguros en Rust para una librería C con ownership complejo y callbacks."
    )
fi

echo "🚀 Ejecutando ${#TASKS[@]} tareas..."
echo ""

for task_def in "${TASKS[@]}"; do
    IFS='|' read -r name prompt <<< "$task_def"
    run_task "$name" "$prompt"
done

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  RESULTADOS: $MODEL"
echo "════════════════════════════════════════════════════════════"

PASSED=$(cat "$BENCH_DIR/passed.count")
FAILED=$(cat "$BENCH_DIR/failed.count")
TOTAL_TIME=$(cat "$BENCH_DIR/total_time.count")
TOTAL=${#TASKS[@]}

echo "✅ Pasaron: $PASSED"
echo "❌ Fallaron: $FAILED"
echo "📊 Total: $TOTAL"
echo "⏱️  Tiempo total: ${TOTAL_TIME}ms"
echo ""

if [ -f "$BENCH_DIR/results.list" ]; then
    cat "$BENCH_DIR/results.list"
fi

# Aggregate results from individual files
AGGREGATE_RESULTS=()
for result_file in "$BENCH_DIR"/*_result.json; do
    if [ -f "$result_file" ]; then
        AGGREGATE_RESULTS+=("$(cat "$result_file")")
    fi
done

# Save summary
cat > "$BENCH_DIR/summary.json" << EOF
{
    "model": "$MODEL",
    "timestamp": "$(date -Iseconds)",
    "total_tasks": ${#TASKS[@]},
    "passed": $PASSED,
    "failed": $FAILED,
    "total_time_ms": $TOTAL_TIME,
    "results": $(printf '%s\n' "${AGGREGATE_RESULTS[@]}" | jq -s .)
}
EOF

echo ""
echo "💾 Resultados guardados en: $BENCH_DIR/summary.json"
echo "📂 Logs completos en: $BENCH_DIR/"