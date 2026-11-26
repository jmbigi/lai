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

# Git Bash (MSYS) entrega rutas tipo /c/DESA/... y Docker Desktop no las monta.
# `cygpath -m` las convierte a C:/DESA/... (formato aceptado por -v en Windows).
to_host_path() {
    if command -v cygpath >/dev/null 2>&1; then
        cygpath -m "$1"
    else
        printf '%s' "$1"
    fi
}

SRC_MOUNT="$(to_host_path "$SRC/lai_rust")"
OUT_MOUNT="$(to_host_path "$OUT")"
CACHE_DIR="${CARGO_CACHE:-$HOME/.cache/lai-swebench-cargo}"
mkdir -p "$CACHE_DIR"
CACHE_MOUNT="$(to_host_path "$CACHE_DIR")"

PLATFORM_ARGS=()
if [ -n "$PLATFORM" ]; then
    PLATFORM_ARGS=(--platform "$PLATFORM")
fi

echo "Compilando lai en $BUILDER_IMAGE (glibc 2.35) ..."
echo "Fuente: $SRC_MOUNT"
echo "Salida: $OUT_MOUNT/lai"
if [ -n "$PLATFORM" ]; then
    echo "Plataforma: $PLATFORM"
fi
echo

docker run --rm "${PLATFORM_ARGS[@]}" \
    -v "$SRC_MOUNT":/src:ro \
    -v "$OUT_MOUNT":/out \
    -v "$CACHE_MOUNT":/cargo \
    "$BUILDER_IMAGE" \
    bash -lc '
        set -e
        export DEBIAN_FRONTEND=noninteractive
        export CARGO_HOME=/cargo
        export RUSTUP_HOME=/cargo/rustup
        # El directorio de compilación vive en el volumen montado para que las
        # recompilaciones reaprovechen las dependencias. Antes era /tmp/target y
        # se perdía en cada corrida: ~25 minutos por build desde cero.
        export CARGO_TARGET_DIR=/cargo/target
        export PATH=/cargo/bin:$PATH
        apt-get update -qq
        apt-get install -y -qq --no-install-recommends \
            build-essential pkg-config libsqlite3-dev curl ca-certificates >/dev/null

        if [ ! -x "$CARGO_HOME/bin/cargo" ]; then
            curl -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path \
                --profile minimal --default-toolchain stable >/dev/null
        fi
        # No dependemos de /cargo/env: si el caché venía de una versión anterior
        # puede no existir. PATH ya está fijado arriba.
        if ! command -v cargo >/dev/null 2>&1; then
            echo "ERROR: cargo no quedó instalado en $CARGO_HOME/bin" >&2
            ls -la "$CARGO_HOME" >&2 || true
            exit 1
        fi
        rustup default stable >/dev/null 2>&1 || true
        cargo --version

        # Copiar excluyendo target/ desde el principio. Con `cp -r` se copiaban
        # los ~7 GB de artefactos del host para borrarlos justo después: en un
        # disco virtualizado eso tarda decenas de minutos y parece un cuelgue.
        # target/ además contiene un binario enlazado contra la glibc del host.
        mkdir -p /build
        tar -C /src --exclude=./target --exclude=./.git -cf - . | tar -C /build -xf -
        cd /build
        cargo build --release --locked --quiet
        cp /cargo/target/release/lai /out/lai
        chmod +x /out/lai
        echo "--- verificación ---"
        ldd --version | head -1
        /out/lai --version
    '

echo
echo "✓ Binario listo: $OUT/lai"
ls -lh "$OUT/lai"
