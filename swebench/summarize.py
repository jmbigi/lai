#!/usr/bin/env python3
"""Resume una corrida del piloto en una tabla markdown lista para BENCHMARKS.md.

Cruza dos artefactos que produce el harness:

- `data/predictions/<modelo>.meta.json`  → estado del agente, duración, contexto
- `data/evaluation/<run_id>/report.json` → resultado oficial de la evaluación

No inventa nada: lo que no está medido se muestra como `—`. Si falta la
evaluación, la columna de resueltas aparece vacía y el script lo dice.

Uso:
    python swebench/summarize.py --model qwen2.5-coder:7b
    python swebench/summarize.py --model qwen2.5-coder:7b --run-id piloto7b
    python swebench/summarize.py --model qwen2.5-coder:7b --md      # solo markdown
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
DATA_DIR = HERE / "data"
PREDICTIONS = DATA_DIR / "predictions"
EVALUATION = DATA_DIR / "evaluation"


def slug(text: str) -> str:
    cleaned = "".join(c if c.isalnum() else "_" for c in text)
    return cleaned.strip("._-") or "run"


def load_json(path: Path) -> dict | None:
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as exc:
        print(f"⚠ no se pudo leer {path}: {exc}", file=sys.stderr)
        return None


def find_report(run_id: str | None, model: str) -> tuple[str | None, dict | None]:
    """Localiza el report.json más reciente de ese modelo si no se da run_id."""
    if run_id:
        report = load_json(EVALUATION / run_id / "report.json")
        return (run_id, report) if report else (run_id, None)

    candidates = []
    if EVALUATION.is_dir():
        for path in EVALUATION.glob("*/report.json"):
            data = load_json(path)
            if not data:
                continue
            run = data.get("run") or {}
            if run.get("model") == model:
                candidates.append((path.parent.name, data))
    if not candidates:
        return None, None
    # El más reciente por finished_at/started_at.
    candidates.sort(
        key=lambda item: (item[1].get("run") or {}).get("finished_at")
        or (item[1].get("run") or {}).get("started_at")
        or "",
        reverse=True,
    )
    return candidates[0]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--model", required=True, help="Modelo tal como se pasó a run_eval.py")
    parser.add_argument("--run-id", help="Run de evaluate.py (por defecto, el más reciente del modelo)")
    parser.add_argument("--md", action="store_true", help="Imprimir solo la tabla markdown")
    args = parser.parse_args()

    meta = load_json(PREDICTIONS / f"{slug(args.model)}.meta.json") or {}
    rows = {iid: row for iid, row in meta.items() if row.get("model") == args.model}

    run_id, report = find_report(args.run_id, args.model)

    if not rows:
        # Sin metadatos de agente se puede resumir igualmente una evaluación
        # (por ejemplo la de validación con parches oficiales).
        if not report:
            print(
                f"❌ No hay metadatos de agente para '{args.model}' en {PREDICTIONS} "
                "ni evaluación que coincida.\n"
                "   Ejecuta primero swebench/run_eval.py con ese --model.",
                file=sys.stderr,
            )
            return 2
        rows = {
            item.get("instance_id"): {
                "status": "evaluada",
                "duration_s": None,
                "patch_bytes": item.get("patch_bytes"),
                "context_length": None,
            }
            for item in report.get("instances") or []
        }
    evaluated: dict[str, dict] = {}
    if report:
        for item in report.get("instances") or []:
            evaluated[item.get("instance_id")] = item

    if not args.md:
        print(f"Modelo: {args.model}")
        print(f"Metadatos: {PREDICTIONS / (slug(args.model) + '.meta.json')}")
        if report:
            print(f"Evaluación: {EVALUATION / run_id / 'report.json'}")
        else:
            print("Evaluación: (no encontrada; ejecuta evaluate.py para completar la tabla)")
        print()

    lines = [
        "| Instancia | Agente | Parche | Duración | Contexto | Resuelta |",
        "|-----------|--------|-------:|---------:|---------:|----------|",
    ]
    agent_total = 0.0
    agent_counted = 0
    resolved_count = 0
    scored = 0
    infra = 0

    for iid in sorted(rows):
        row = rows[iid]
        status = row.get("status", "?")
        duration = row.get("duration_s")
        if isinstance(duration, (int, float)):
            agent_total += duration
            agent_counted += 1
            duration_txt = f"{duration:.1f} s"
        else:
            duration_txt = "—"
        patch_bytes = row.get("patch_bytes")
        patch_txt = f"{patch_bytes} B" if isinstance(patch_bytes, int) else "—"
        context = row.get("context_length")
        context_txt = str(context) if context else "—"

        if status == "infra_error":
            infra += 1

        result = evaluated.get(iid)
        if result is None:
            resolved_txt = "—"
        elif not result.get("scored", True):
            resolved_txt = f"no puntuada ({result.get('status', '?')})"
        else:
            scored += 1
            if result.get("resolved"):
                resolved_count += 1
                resolved_txt = "sí"
            else:
                resolved_txt = f"no ({result.get('resolution', '?')})"

        lines.append(
            f"| {iid} | {status} | {patch_txt} | {duration_txt} | {context_txt} | {resolved_txt} |"
        )

    print("\n".join(lines))
    print()

    if not args.md:
        if agent_counted:
            print(f"Agente: {len(rows)} instancias, {agent_total:.1f} s en total")
            print(f"  tiempo medio: {agent_total / agent_counted:.1f} s")
        else:
            print(f"Instancias: {len(rows)} (sin datos de agente; solo evaluación)")
        if infra:
            print(f"  errores de infraestructura: {infra} (no cuentan como parche)")
        if report:
            rate = (resolved_count / scored) if scored else None
            if rate is None:
                print("  evaluación: 0 instancias puntuables")
            else:
                print(
                    f"  resueltas: {resolved_count}/{scored} "
                    f"({rate:.1%} de las puntuadas)"
                )
        else:
            print("  resueltas: — (falta evaluate.py)")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
