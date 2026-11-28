#!/usr/bin/env bash
# Ejecuta el agente sobre las instancias de una muestra cuyas imágenes ya están
# preparadas. Pensado para encadenar pasadas mientras `prepare.py` sigue
# descargando: cada pasada solo toca lo que puede ejecutar.
#
# Uso: ./swebench/run_ready.sh <sample-file> [extra args para run_eval.py]
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

SAMPLE="${1:?uso: run_ready.sh <sample-file> [args]}"
shift || true

if [[ ! -f "$SAMPLE" ]]; then
    echo "ERROR: no existe la muestra '$SAMPLE'" >&2
    exit 1
fi

# Instancias listas según state.json.
python3 - "$SAMPLE" > /tmp/user/1000/opencode/ready_args.txt <<'PY'
import json, sys
from pathlib import Path

sample = [l.strip() for l in Path(sys.argv[1]).read_text().splitlines() if l.strip()]
state = json.loads(Path("swebench/data/state.json").read_text())
rows = state.get("instances", state)
ready = [i for i in sample if (rows.get(i) or {}).get("status") == "ready"]
print(" ".join(f"--instance {i}" for i in ready))
print(f"# ready={len(ready)} of {len(sample)}", file=sys.stderr)
PY

ARGS="$(grep -v '^#' /tmp/user/1000/opencode/ready_args.txt || true)"
if [[ -z "${ARGS// /}" ]]; then
    echo "No hay instancias listas todavía."
    exit 0
fi

COUNT=$(grep -o -- '--instance' /tmp/user/1000/opencode/ready_args.txt | wc -l)
echo "Ejecutando el agente sobre $COUNT instancias listas..."
# shellcheck disable=SC2086
python3 swebench/run_eval.py --skip-preflight --keep-going $ARGS "$@"
