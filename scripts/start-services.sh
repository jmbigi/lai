#!/usr/bin/env bash
# Start the complete local lai stack: llama.cpp (8080) and gateway (8081).
#
# Usage:
#   ./scripts/start-services.sh [--restart] [--build-gateway] [--skip-llama] [--skip-gateway]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESTART=0
BUILD_GATEWAY=0
SKIP_LLAMA=0
SKIP_GATEWAY=0

for arg in "$@"; do
    case "$arg" in
        --restart) RESTART=1 ;;
        --build-gateway) BUILD_GATEWAY=1 ;;
        --skip-llama) SKIP_LLAMA=1 ;;
        --skip-gateway) SKIP_GATEWAY=1 ;;
        -h|--help)
            sed -n '2,6p' "$0"
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

if [[ "$SKIP_LLAMA" -eq 0 ]]; then
    llama_args=()
    if [[ "$RESTART" -eq 1 ]]; then
        llama_args+=(--restart)
    fi
    if [[ ${#llama_args[@]} -gt 0 ]]; then
        "$SCRIPT_DIR/start-llama.sh" "${llama_args[@]}"
    else
        "$SCRIPT_DIR/start-llama.sh"
    fi
fi

if [[ "$SKIP_GATEWAY" -eq 0 ]]; then
    gateway_args=()
    if [[ "$RESTART" -eq 1 ]]; then
        gateway_args+=(--restart)
    fi
    if [[ "$BUILD_GATEWAY" -eq 1 ]]; then
        gateway_args+=(--build)
    fi
    if [[ ${#gateway_args[@]} -gt 0 ]]; then
        "$SCRIPT_DIR/start-gateway.sh" "${gateway_args[@]}"
    else
        "$SCRIPT_DIR/start-gateway.sh"
    fi
fi

echo ""
echo "Local stack is ready:"
if [[ "$SKIP_LLAMA" -eq 0 ]]; then
    echo "  llama.cpp: http://127.0.0.1:8080"
fi
if [[ "$SKIP_GATEWAY" -eq 0 ]]; then
    echo "  gateway:   http://127.0.0.1:8081"
fi
echo "Stop it with: $SCRIPT_DIR/stop-services.sh"
