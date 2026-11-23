#!/bin/bash
# Integration test runner for LAI
# 
# Usage: ./scripts/test-integration.sh [OPTIONS]
# 
# Options:
#   --start-services    Start docker-compose services before tests
#   --stop-services     Stop docker-compose services after tests
#   --benchmarks-only   Run only benchmark tests
#   --infra-only        Run only infrastructure tests
#   --help              Show this help

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
LAI_RUST_DIR="$PROJECT_ROOT/lai_rust"

START_SERVICES=false
STOP_SERVICES=false
BENCHMARKS_ONLY=false
INFRA_ONLY=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --start-services)
            START_SERVICES=true
            shift
            ;;
        --stop-services)
            STOP_SERVICES=true
            shift
            ;;
        --benchmarks-only)
            BENCHMARKS_ONLY=true
            shift
            ;;
        --infra-only)
            INFRA_ONLY=true
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --start-services    Start docker-compose services before tests"
            echo "  --stop-services     Stop docker-compose services after tests"
            echo "  --benchmarks-only   Run only benchmark tests"
            echo "  --infra-only        Run only infrastructure tests"
            echo "  --help              Show this help"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

cd "$LAI_RUST_DIR"

# Function to wait for services
wait_for_services() {
    echo "Waiting for services to be ready..."
    
    local max_retries=60
    local retry=0
    
    while [[ $retry -lt $max_retries ]]; do
        if curl -sf http://localhost:8080/health >/dev/null 2>&1 && \
           curl -sf http://localhost:8081/health >/dev/null 2>&1; then
            echo "✅ Core services are ready"
            return 0
        fi
        
        retry=$((retry + 1))
        echo "Waiting for services... ($retry/$max_retries)"
        sleep 5
    done
    
    echo "❌ Services not ready after $max_retries retries"
    return 1
}

# Start services if requested
if [[ "$START_SERVICES" == "true" ]]; then
    echo "🚀 Starting docker-compose services..."
    cd "$PROJECT_ROOT"
    docker-compose up -d
    
    if ! wait_for_services; then
        echo "❌ Failed to start services"
        docker-compose logs --tail=50
        exit 1
    fi
fi

# Run tests
TEST_ARGS="--test integration"

if [[ "$BENCHMARKS_ONLY" == "true" ]]; then
    TEST_ARGS="$TEST_ARGS benchmarks"
elif [[ "$INFRA_ONLY" == "true" ]]; then
    TEST_ARGS="$TEST_ARGS infrastructure"
fi

echo "🧪 Running integration tests: cargo test $TEST_ARGS -- --ignored"
cargo test $TEST_ARGS -- --ignored

TEST_EXIT_CODE=$?

# Stop services if requested
if [[ "$STOP_SERVICES" == "true" ]]; then
    echo "🛑 Stopping docker-compose services..."
    cd "$PROJECT_ROOT"
    docker-compose down
fi

exit $TEST_EXIT_CODE