#!/usr/bin/env python3
"""Etapa 1 — Descargar SWE-bench Verified a disco local (reanudable).

Uso:
    python3 swebench/fetch_dataset.py                 # todo (500 instancias)
    python3 swebench/fetch_dataset.py --limit 10      # solo las primeras N
    python3 swebench/fetch_dataset.py --repos django  # filtrar por repo

Escribe:
    swebench/data/instances.jsonl   una instancia por línea
    swebench/data/manifest.json     metadatos de la descarga

Nota sobre la Regla 0: este script es una herramienta de desarrollo, no forma
parte del camino de ejecución de lai. La descarga se hace UNA vez; a partir de
ahí todo el trabajo (preparar, ejecutar, evaluar) ocurre en local.
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

DATASET = "princeton-nlp/SWE-bench_Verified"
ROOT = Path(__file__).resolve().parent
DATA_DIR = ROOT / "data"
INSTANCES = DATA_DIR / "instances.jsonl"
MANIFEST = DATA_DIR / "manifest.json"

# Campos que necesita el resto del pipeline.
KEEP_FIELDS = [
    "instance_id",
    "repo",
    "base_commit",
    "environment_setup_commit",
    "version",
    "problem_statement",
    "hints_text",
    "created_at",
    "patch",
    "test_patch",
    "FAIL_TO_PASS",
    "PASS_TO_PASS",
]


def load_existing() -> dict[str, dict]:
    """Instancias ya descargadas, para reanudar sin volver a escribir todo."""
    if not INSTANCES.exists():
        return {}
    out: dict[str, dict] = {}
    with INSTANCES.open("r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue  # línea truncada de una ejecución anterior
            out[obj["instance_id"]] = obj
    return out


def normalise(row: dict) -> dict:
    out = {}
    for field in KEEP_FIELDS:
        value = row.get(field)
        # FAIL_TO_PASS / PASS_TO_PASS llegan como str JSON en el dataset.
        if field in ("FAIL_TO_PASS", "PASS_TO_PASS") and isinstance(value, str):
            try:
                value = json.loads(value)
            except json.JSONDecodeError:
                value = []
        out[field] = value
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--limit", type=int, default=0, help="Máximo de instancias (0 = todas)")
    parser.add_argument("--repos", nargs="*", default=None, help="Subcadenas de repo a incluir")
    parser.add_argument("--force", action="store_true", help="Reescribir aunque ya exista el archivo")
    args = parser.parse_args()

    DATA_DIR.mkdir(parents=True, exist_ok=True)

    if INSTANCES.exists() and not args.force:
        existing = load_existing()
        print(f"Ya hay {len(existing)} instancias en {INSTANCES}")
        print("Usa --force para volver a descargar el dataset completo.")
        return 0

    try:
        from datasets import load_dataset
    except ImportError:
        print("Falta la librería 'datasets'. Instala con:", file=sys.stderr)
        print("  pip install datasets", file=sys.stderr)
        return 2

    print(f"Descargando {DATASET} ...")
    ds = load_dataset(DATASET, split="test")
    print(f"Dataset cargado: {len(ds)} instancias")

    rows = []
    for row in ds:
        repo = row.get("repo", "")
        if args.repos and not any(fragment in repo for fragment in args.repos):
            continue
        rows.append(normalise(row))

    if args.limit and args.limit > 0:
        rows = rows[: args.limit]

    with INSTANCES.open("w", encoding="utf-8") as fh:
        for row in rows:
            fh.write(json.dumps(row, ensure_ascii=False) + "\n")

    repos: dict[str, int] = {}
    for row in rows:
        repos[row["repo"]] = repos.get(row["repo"], 0) + 1

    manifest = {
        "dataset": DATASET,
        "downloaded_at": datetime.now(timezone.utc).isoformat(),
        "instances": len(rows),
        "limit": args.limit or None,
        "repos_filter": args.repos,
        "repos": dict(sorted(repos.items(), key=lambda kv: -kv[1])),
        "fields": KEEP_FIELDS,
    }
    MANIFEST.write_text(json.dumps(manifest, indent=2, ensure_ascii=False), encoding="utf-8")

    print(f"Escritas {len(rows)} instancias en {INSTANCES}")
    print(f"Repos: {len(repos)}")
    for repo, count in list(manifest["repos"].items())[:10]:
        print(f"  {repo}: {count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
