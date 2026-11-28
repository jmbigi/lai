#!/usr/bin/env bash
# Ejecuta pasadas sucesivas sobre la muestra hasta que no queden instancias
# pendientes con imagen lista. Pensado para correr mientras prepare.py descarga.
#
# Uso: ./swebench/run_until_done.sh <sample-file> [args para run_eval.py]
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

SAMPLE="${1:?uso: run_until_done.sh <sample-file> [args]}"
shift || true

MAX_PASSES="${MAX_PASSES:-40}"
PASS=0
while [[ $PASS -lt $MAX_PASSES ]]; do
    PASS=$((PASS + 1))

    PENDING=$(python3 - "$SAMPLE" <<'PY'
import json, sys
from pathlib import Path

sample = [l.strip() for l in Path(sys.argv[1]).read_text().splitlines() if l.strip()]
state = json.loads(Path("swebench/data/state.json").read_text())
rows = state.get("instances", state)
ready = {i for i in sample if (rows.get(i) or {}).get("status") == "ready"}

done = set()
pred = Path("swebench/data/predictions/qwen3-8b.jsonl")
if pred.exists():
    for line in pred.read_text().splitlines():
        if line.strip():
            try:
                done.add(json.loads(line)["instance_id"])
            except (json.JSONDecodeError, KeyError):
                pass

print(len((ready - done)))
PY
)

    if [[ "$PENDING" -eq 0 ]]; then
        echo "== Sin instancias pendientes con imagen lista (pasada $PASS) =="
        exit 0
    fi

    echo "== Pasada $PASS: $PENDING instancias pendientes =="
    # shellcheck disable=SC2086
    ./swebench/run_ready.sh "$SAMPLE" "$@"
    echo "== Fin de la pasada $PASS =="
    sleep 20
done

echo "Aviso: se alcanzó MAX_PASSES=$MAX_PASSES con instancias pendientes." >&2
exit 1
