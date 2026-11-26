#!/usr/bin/env python3
"""Etapa 2 — Preparar entornos Docker para las instancias descargadas.

Cada instancia de SWE-bench tiene una imagen precompilada con el repositorio ya
en el commit base. Este script las descarga de forma **incremental**: puedes
parar cuando quieras y relanzarlo; solo continúa lo que falta.

Uso:
    python3 swebench/prepare.py --limit 2          # preparar 2 instancias
    python3 swebench/prepare.py --check-only       # solo informar estado
    python3 swebench/prepare.py --instance astropy__astropy-12907
    python3 swebench/prepare.py                    # todas las descargadas

Escribe:
    swebench/data/state.json    estado por instancia (pendiente/listo/error)
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent
DATA_DIR = ROOT / "data"
INSTANCES = DATA_DIR / "instances.jsonl"
STATE = DATA_DIR / "state.json"

# Límite blando de espacio libre: no empezar una descarga por debajo de esto.
MIN_FREE_GB = 20


def image_name(instance_id: str) -> str:
    """Convención oficial de imágenes de SWE-bench."""
    return f"swebench/sweb.eval.x86_64.{instance_id.replace('__', '_1776_')}".lower()


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kwargs)


def docker_available() -> bool:
    result = run(["docker", "info", "--format", "{{.ServerVersion}}"])
    if result.returncode != 0:
        print("❌ Docker no está disponible:", result.stderr.strip()[:200], file=sys.stderr)
        return False
    return True


def image_present(name: str) -> bool:
    return run(["docker", "image", "inspect", name]).returncode == 0


def free_gb(path: str = "/var/lib/docker") -> float:
    try:
        usage = shutil.disk_usage(path)
    except OSError:
        usage = shutil.disk_usage("/")
    return usage.free / (1024**3)


def load_instances() -> list[dict]:
    if not INSTANCES.exists():
        print(f"No existe {INSTANCES}. Ejecuta primero fetch_dataset.py", file=sys.stderr)
        raise SystemExit(2)
    rows = []
    with INSTANCES.open("r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def load_state() -> dict:
    if STATE.exists():
        try:
            return json.loads(STATE.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            pass
    return {"updated_at": None, "instances": {}}


def save_state(state: dict) -> None:
    state["updated_at"] = datetime.now(timezone.utc).isoformat()
    tmp = STATE.with_suffix(".tmp")
    tmp.write_text(json.dumps(state, indent=2), encoding="utf-8")
    tmp.replace(STATE)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--limit", type=int, default=0, help="Preparar como máximo N instancias nuevas")
    parser.add_argument("--instance", default=None, help="Preparar una sola instancia por id")
    parser.add_argument("--check-only", action="store_true", help="No descargar, solo informar")
    parser.add_argument("--timeout", type=int, default=1800, help="Timeout por descarga en segundos")
    args = parser.parse_args()

    if not docker_available():
        return 2

    instances = load_instances()
    if args.instance:
        instances = [i for i in instances if i["instance_id"] == args.instance]
        if not instances:
            print(f"Instancia no encontrada en el dataset: {args.instance}", file=sys.stderr)
            return 2

    state = load_state()
    entries = state.setdefault("instances", {})

    prepared = 0
    attempted = 0
    for row in instances:
        iid = row["instance_id"]
        image = image_name(iid)
        entry = entries.setdefault(iid, {})
        entry["image"] = image

        if image_present(image):
            if entry.get("status") != "ready":
                entry["status"] = "ready"
                entry["ready_at"] = datetime.now(timezone.utc).isoformat()
                prepared += 1
            continue

        if args.check_only:
            entry.setdefault("status", "pending")
            continue

        if args.limit and attempted >= args.limit:
            break

        if free_gb() < MIN_FREE_GB:
            print(f"⚠ Espacio libre insuficiente ({free_gb():.1f} GB). Deteniendo.")
            break

        print(f"⬇ {iid}  →  {image}")
        attempted += 1
        entry["status"] = "pulling"
        save_state(state)

        result = run(["docker", "pull", image], timeout=args.timeout)
        if result.returncode == 0:
            entry["status"] = "ready"
            entry["ready_at"] = datetime.now(timezone.utc).isoformat()
            entry.pop("error", None)
            prepared += 1
            print("  ✓ listo")
        else:
            entry["status"] = "error"
            entry["error"] = (result.stderr or result.stdout).strip()[:500]
            print(f"  ✗ error: {entry['error'][:160]}")
        save_state(state)

    save_state(state)

    # Resumen
    counts: dict[str, int] = {}
    for entry in entries.values():
        counts[entry.get("status", "pending")] = counts.get(entry.get("status", "pending"), 0) + 1

    print()
    print("Estado de preparación:")
    for status in ("ready", "pending", "pulling", "error"):
        if counts.get(status):
            print(f"  {status:8s} {counts[status]}")
    print(f"  espacio libre: {free_gb():.1f} GB")
    if prepared:
        print(f"\nNuevas preparadas en esta ejecución: {prepared}")
    print(f"Estado guardado en {STATE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
