"""Genera predicciones 'oracle' usando el patch oficial del dataset.

Sirve para validar evaluate.py: el parche de referencia debe salir RESOLVED.
No mide al agente; mide al evaluador.

Uso:
    python swebench/make_oracle_predictions.py --out swebench/data/predictions/oracle.jsonl
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
INSTANCES = ROOT / "data" / "instances.jsonl"
DEFAULT_OUT = ROOT / "data" / "predictions" / "oracle.jsonl"


def load(path: Path) -> list[dict]:
    if not path.exists():
        print(f"No existe {path}. Ejecuta antes fetch_dataset.py", file=sys.stderr)
        raise SystemExit(2)
    rows = []
    with path.open("r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--instances", default=str(INSTANCES))
    parser.add_argument("--out", default=str(DEFAULT_OUT))
    parser.add_argument("--instance", action="append", default=None, help="Filtrar (repetible o separado por comas)")
    parser.add_argument("--limit", type=int, default=0)
    args = parser.parse_args()

    wanted: list[str] = []
    for value in args.instance or []:
        wanted.extend(part.strip() for part in value.split(",") if part.strip())

    rows = load(Path(args.instances))
    if wanted:
        rows = [r for r in rows if r["instance_id"] in wanted]
    if args.limit:
        rows = rows[: args.limit]
    if not rows:
        print("No hay instancias que coincidan.", file=sys.stderr)
        return 2

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8", newline="\n") as fh:
        for row in rows:
            prediction = {
                "instance_id": row["instance_id"],
                "model_name_or_path": "oracle-gold-patch",
                "model_patch": row.get("patch") or "",
            }
            fh.write(json.dumps(prediction, ensure_ascii=False) + "\n")

    print(f"Escritas {len(rows)} predicciones oracle en {out}")
    print("Son los parches oficiales: evaluate.py debe dar resolved en todas.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
