#!/usr/bin/env python3
"""Etapa 1 — Descargar SWE-bench Verified a disco local (reanudable).

Uso:
    python3 swebench/fetch_dataset.py                 # todo (500 instancias)
    python3 swebench/fetch_dataset.py --limit 10      # solo las primeras N
    python3 swebench/fetch_dataset.py --repos django  # filtrar por repo
    python3 swebench/fetch_dataset.py --force         # volver a descargar

Escribe:
    swebench/data/instances.jsonl   una instancia por línea
    swebench/data/manifest.json     metadatos de la descarga

El dataset oficial (`SWE-bench/SWE-bench_Verified`) incluye la imagen Docker,
el script de evaluación y el parser de logs de cada instancia. Sin esos campos
la etapa 4 no puede evaluar: si tienes datos antiguos, vuelve a descargarlos con
`--force`.

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

DEFAULT_DATASET = "SWE-bench/SWE-bench_Verified"
ROOT = Path(__file__).resolve().parent
DATA_DIR = ROOT / "data"
INSTANCES = DATA_DIR / "instances.jsonl"
MANIFEST = DATA_DIR / "manifest.json"

# Campos que necesita el resto del pipeline. Los cuatro últimos son
# imprescindibles para evaluate.py; sin ellos no se puede reproducir la
# evaluación oficial.
KEEP_FIELDS = [
    "instance_id",
    "repo",
    "base_commit",
    "environment_setup_commit",
    "version",
    "difficulty",
    "problem_statement",
    "hints_text",
    "created_at",
    "patch",
    "test_patch",
    "FAIL_TO_PASS",
    "PASS_TO_PASS",
    "image",
    "eval_script",
    "log_parser",
    "eval_type",
]

REQUIRED_EVAL_FIELDS = ("image", "eval_script", "log_parser", "FAIL_TO_PASS", "PASS_TO_PASS")


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
            if isinstance(obj, dict) and obj.get("instance_id"):
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


def write_jsonl_atomic(path: Path, rows: list[dict]) -> None:
    tmp = path.with_suffix(path.suffix + ".tmp")
    with tmp.open("w", encoding="utf-8", newline="\n") as fh:
        for row in rows:
            fh.write(json.dumps(row, ensure_ascii=False) + "\n")
        fh.flush()
    tmp.replace(path)


def write_json_atomic(path: Path, data: dict) -> None:
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    tmp.replace(path)


def existing_is_usable(existing: dict[str, dict]) -> bool:
    """Comprueba que los datos ya descargados sirven para evaluate.py."""
    if not existing:
        return False
    sample = next(iter(existing.values()))
    return all(field in sample for field in REQUIRED_EVAL_FIELDS)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset", default=DEFAULT_DATASET, help="Dataset de HuggingFace")
    parser.add_argument("--split", default="test", help="Split del dataset")
    parser.add_argument("--limit", type=int, default=0, help="Máximo de instancias (0 = todas)")
    parser.add_argument("--repos", nargs="*", default=None, help="Subcadenas de repo a incluir")
    parser.add_argument("--force", action="store_true", help="Reescribir aunque ya exista el archivo")
    args = parser.parse_args()

    DATA_DIR.mkdir(parents=True, exist_ok=True)

    if INSTANCES.exists() and not args.force:
        existing = load_existing()
        if existing and existing_is_usable(existing):
            print(f"Ya hay {len(existing)} instancias utilizables en {INSTANCES}")
            print("Usa --force para volver a descargar el dataset completo.")
            return 0
        print(
            f"⚠ {INSTANCES} existe pero no contiene image/eval_script/log_parser.\n"
            "  Son datos del dataset antiguo y evaluate.py no puede usarlos.\n"
            "  Vuelve a descargar con: python3 swebench/fetch_dataset.py --force",
            file=sys.stderr,
        )
        return 2

    try:
        from datasets import load_dataset
    except ImportError:
        print("Falta la librería 'datasets'. Instala con:", file=sys.stderr)
        print("  pip install datasets", file=sys.stderr)
        return 2

    print(f"Descargando {args.dataset} (split={args.split}) ...")
    ds = load_dataset(args.dataset, split=args.split)
    print(f"Dataset cargado: {len(ds)} instancias")

    rows = []
    skipped = 0
    for row in ds:
        repo = row.get("repo", "")
        if args.repos and not any(fragment in repo for fragment in args.repos):
            continue
        normalised = normalise(row)
        if not normalised.get("instance_id"):
            skipped += 1
            continue
        missing = [f for f in REQUIRED_EVAL_FIELDS if not normalised.get(f)]
        if missing:
            print(
                f"⚠ {normalised['instance_id']}: faltan campos {', '.join(missing)}",
                file=sys.stderr,
            )
        rows.append(normalised)

    if args.limit and args.limit > 0:
        rows = rows[: args.limit]

    if not rows:
        print("No se descargó ninguna instancia. Revisa --repos/--limit.", file=sys.stderr)
        return 2

    write_jsonl_atomic(INSTANCES, rows)

    repos: dict[str, int] = {}
    for row in rows:
        repos[row["repo"]] = repos.get(row["repo"], 0) + 1

    manifest = {
        "dataset": args.dataset,
        "split": args.split,
        "downloaded_at": datetime.now(timezone.utc).isoformat(),
        "instances": len(rows),
        "skipped": skipped,
        "limit": args.limit or None,
        "repos_filter": args.repos,
        "repos": dict(sorted(repos.items(), key=lambda kv: -kv[1])),
        "fields": KEEP_FIELDS,
    }
    write_json_atomic(MANIFEST, manifest)

    print(f"Escritas {len(rows)} instancias en {INSTANCES}")
    print(f"Repos: {len(repos)}")
    for repo, count in list(manifest["repos"].items())[:10]:
        print(f"  {repo}: {count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
