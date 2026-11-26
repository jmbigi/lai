#!/usr/bin/env bash
# Etapa 2b — Compilar un binario de lai compatible con las imágenes SWE-bench.
#
# Las imágenes de SWE-bench son Ubuntu 22.04 (glibc 2.35). El binario del host
# requiere glibc más nueva, así que lo compilamos dentro de un contenedor
# ubuntu:22.04 y lo guardamos en swebench/bin/lai.
#
# Uso:
#   ./swebench/build_lai_for_swebench.sh
#   PLATFORM=linux/amd64 ./swebench/build_lai_for_swebench.sh
#
# Resultado:
#   swebench/bin/lai   binario listo para copiarse dentro de cualquier imagen

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$(cd "$ROOT/.." && pwd)"
OUT="$ROOT/bin"
BUILDER_IMAGE="${BUILDER_IMAGE:-ubuntu:22.04}"
PLATFORM="${PLATFORM:-}"

mkdir -p "$OUT"

PLATFORM_ARGS=()
if [ -n "$PLATFORM" ]; then
    PLATFORM_ARGS=(--platform "$PLATFORM")
fi

echo "Compilando lai en $BUILDER_IMAGE (glibc 2.35) ..."
echo "Fuente: $SRC/lai_rust"
echo "Salida: $OUT/lai"
if [ -n "$PLATFORM" ]; then
    echo "Plataforma: $PLATFORM"
fi
echo

docker run --rm "${PLATFORM_ARGS[@]}" \
    -v "$SRC/lai_rust":/src:ro \
    -v "$OUT":/out \
    -v "${CARGO_CACHE:-$HOME/.cache/lai-swebench-cargo}":/cargo \
    -e CARGO_HOME=/cargo \
    -e CARGO_TARGET_DIR=/tmp/target \
    "$BUILDER_IMAGE" \
    bash -lc '
        set -e
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        apt-get install -y -qq --no-install-recommends \
            build-essential pkg-config libsqlite3-dev curl ca-certificates >/dev/null

        if [ ! -x /cargo/bin/cargo ]; then
            curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable >/dev/null
        fi
        . /cargo/env
        # Idempotente: el caché puede tener el toolchain sin default configurado.
        rustup default stable >/dev/null 2>&1 || true

        mkdir -p /build && cp -r /src/. /build/
        # Excluir artefactos del host: target/ contiene un binario enlazado
        # contra la glibc nueva del host y se copiaría por error.
        rm -rf /build/target
        cd /build
        cargo build --release --locked --quiet
        cp /tmp/target/release/lai /out/lai
        chmod +x /out/lai
        echo "--- verificación ---"
        ldd --version | head -1
        /out/lai --version
    '

echo
echo "✓ Binario listo: $OUT/lai"
ls -lh "$OUT/lai"
