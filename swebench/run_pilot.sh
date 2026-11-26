#!/usr/bin/env bash
# Piloto SWE-bench: 5 instancias, una por parser oficial.
#
# Uso:
#   ./swebench/run_pilot.sh                 # modelo por defecto
#   MODEL=qwen2.5-coder:3b ./swebench/run_pilot.sh
#
# Requisitos: Docker funcionando, swebench/bin/lai compilado, Ollama con el
# modelo, e imágenes descargadas (prepare.py).
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.." || exit 1

MODEL="${MODEL:-qwen2.5-coder:7b}"
MAX_ITER="${MAX_ITER:-10}"
TIMEOUT="${TIMEOUT:-1500}"
PYTHON="${PYTHON:-python}"

INSTANCES=(
  django__django-12209
  pytest-dev__pytest-5809
  scikit-learn__scikit-learn-14141
  sphinx-doc__sphinx-8595
  sympy__sympy-20916
)

if [ ! -x "swebench/bin/lai" ]; then
  echo "ERROR: falta swebench/bin/lai. Ejecuta ./swebench/build_lai_for_swebench.sh" >&2
  exit 2
fi

echo "Piloto: ${#INSTANCES[@]} instancias, modelo $MODEL"
echo "Iteraciones: $MAX_ITER   timeout por instancia: ${TIMEOUT}s"
echo

for iid in "${INSTANCES[@]}"; do
  echo "=================================================================="
  echo "== $iid"
  echo "=================================================================="
  "$PYTHON" swebench/run_eval.py \
    --instance "$iid" \
    --model "$MODEL" \
    --max-iterations "$MAX_ITER" \
    --timeout "$TIMEOUT" \
    --keep-going
  echo
done

echo "=================================================================="
echo "Predicciones: swebench/data/predictions/"
echo "Evaluar con:"
echo "  $PYTHON swebench/evaluate.py --predictions swebench/data/predictions/<modelo>.jsonl"
