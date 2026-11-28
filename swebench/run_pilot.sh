#!/usr/bin/env bash
# Piloto SWE-bench: 5 instancias, una por parser oficial, y evaluación al final.
#
# Uso:
#   ./swebench/run_pilot.sh                 # modelo por defecto, evalúa al terminar
#   MODEL=qwen2.5-coder:3b ./swebench/run_pilot.sh
#   SKIP_EVAL=1 ./swebench/run_pilot.sh     # solo el agente, sin evaluar
#
# Requisitos: Docker funcionando, swebench/bin/lai compilado, Ollama con el
# modelo, e imágenes descargadas (prepare.py).
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.." || exit 1

MODEL="${MODEL:-qwen2.5-coder:7b}"
MAX_ITER="${MAX_ITER:-10}"
# Medido el 2026-11-26 con qwen2.5-coder:7b en CPU: django completó 10
# iteraciones en 1048 s, pero sphinx necesitó más de 1500 s para solo 4. El
# coste por iteración depende de cuánto contexto lee el agente, así que el
# margen por defecto es amplio; ajústalo con TIMEOUT=... si tu equipo es más
# rápido.
TIMEOUT="${TIMEOUT:-2700}"
PYTHON="${PYTHON:-python}"
SKIP_EVAL="${SKIP_EVAL:-0}"
RUN_ID="${RUN_ID:-piloto-$(date +%Y%m%d-%H%M%S)}"

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
echo "Run id de evaluación: $RUN_ID"
echo

for iid in "${INSTANCES[@]}"; do
  echo "=================================================================="
  echo "== $iid"
  echo "=================================================================="
  # --force: reejecuta aunque ya exista una predicción de una corrida anterior.
  # Sin esto, un piloto repetido no hace nada y parece que "no hay pendientes".
  "$PYTHON" swebench/run_eval.py \
    --instance "$iid" \
    --model "$MODEL" \
    --max-iterations "$MAX_ITER" \
    --timeout "$TIMEOUT" \
    --keep-going \
    --force
  echo
done

echo "=================================================================="
echo "Predicciones: swebench/data/predictions/"
echo "=================================================================="

if [ "$SKIP_EVAL" = "1" ]; then
  echo "SKIP_EVAL=1: no se evalúa. Para hacerlo a mano:"
  echo "  $PYTHON swebench/evaluate.py --predictions swebench/data/predictions/<modelo>.jsonl --run-id $RUN_ID"
  exit 0
fi

echo
echo "== Evaluación oficial =="
"$PYTHON" swebench/evaluate.py \
  --predictions "swebench/data/predictions/$(echo "$MODEL" | sed 's/[^a-zA-Z0-9._-]\+/_/g').jsonl" \
  --run-id "$RUN_ID" \
  --force
EVAL_RC=$?

echo
echo "== Resumen para BENCHMARKS.md =="
"$PYTHON" swebench/summarize.py --model "$MODEL" --run-id "$RUN_ID"
SUM_RC=$?

echo
echo "== Estimación SWE (Regla 12: Wilson 95 %, tamaño de muestra) =="
"$PYTHON" swebench/estimate.py --run-id "$RUN_ID"
EST_RC=$?

echo
if [ "$EVAL_RC" -ne 0 ] || [ "$SUM_RC" -ne 0 ] || [ "$EST_RC" -ne 0 ]; then
  echo "Aviso: evaluación rc=$EVAL_RC, resumen rc=$SUM_RC, estimación rc=$EST_RC. Revisa los logs." >&2
  exit 1
fi
echo "Listo. Informe: swebench/data/evaluation/$RUN_ID/report.md"
