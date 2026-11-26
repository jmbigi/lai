#!/usr/bin/env python3
"""Etapa 2 — Preparar entornos Docker para las instancias descargadas.

Cada instancia de SWE-bench tiene una imagen precompilada con el repositorio ya
en el commit base. Este script las descarga de forma **incremental**: puedes
parar cuando quieras y relanzarlo; solo continúa lo que falta.

Uso:
    python3 swebench/prepare.py --limit 2          # preparar 2 instancias
    python3 swebench/prepare.py --check-only       # solo informar estado
    python3 swebench/prepare.py --instance astropy__astropy-12907
    python3 swebench/prepare.py --platform linux/amd64
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

DEFAULT_MIN_FREE_GB = 20


def image_name(instance: dict) -> str:
    """Imagen oficial del dataset; fallback para datos antiguos."""
    return instance.get("image") or (
        f"swebench/sweb.eval.x86_64.{instance['instance_id'].replace('__', '_1776_')}".lower()
    )


def run(cmd: list[str], timeout: int = 120) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)


def docker_available() -> bool:
    try:
        result = run(["docker", "info", "--format", "{{.ServerVersion}}"], timeout=30)
    except (FileNotFoundError, subprocess.TimeoutExpired) as exc:
        print(f"❌ Docker no está disponible: {exc}", file=sys.stderr)
        return False
    if result.returncode != 0:
        print(f"❌ Docker no está disponible: {(result.stderr or result.stdout).strip()[:300]}", file=sys.stderr)
        return False
    return True


def image_state(name: str) -> bool | None:
    """True si existe, False si no, None si Docker falló de forma inesperada."""
    try:
        result = run(["docker", "image", "inspect", name], timeout=60)
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return None
    if result.returncode == 0:
        return True
    error = (result.stderr or result.stdout).lower()
    if "no such image" in error or "no such object" in error:
        return False
    # Docker puede devolver 1 también para "permission denied" o daemon caído.
    return None


def free_gb(path: str = "/var/lib/docker") -> float:
    try:
        usage = shutil.disk_usage(path)
    except OSError:
        usage = shutil.disk_usage("/")
    return usage.free / (1024**3)


def load_instances() -> list[dict]:
    if not INSTANCES.exists():
        print(f"No existe {INSTANCES}. Ejecuta primero fetch_dataset.py --force", file=sys.stderr)
        raise SystemExit(2)
    rows = []
    with INSTANCES.open("r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError:
                continue
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
    tmp.write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    tmp.replace(STATE)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--limit", type=int, default=0, help="Preparar como máximo N instancias nuevas")
    parser.add_argument("--instance", action="append", default=None, help="Preparar una instancia (repetible)")
    parser.add_argument("--check-only", action="store_true", help="No descargar, solo informar")
    parser.add_argument("--timeout", type=int, default=1800, help="Timeout por descarga en segundos")
    parser.add_argument("--platform", default=None, help="Plataforma Docker, p. ej. linux/amd64")
    parser.add_argument(
        "--min-free-gb",
        type=float,
        default=DEFAULT_MIN_FREE_GB,
        help=f"Espacio libre mínimo antes de descargar (por defecto {DEFAULT_MIN_FREE_GB} GB)",
    )
    args = parser.parse_args()

    if not docker_available():
        return 2

    instances = load_instances()
    if args.instance:
        wanted = []
        for value in args.instance:
            wanted.extend(part.strip() for part in value.split(",") if part.strip())
        by_id = {row["instance_id"]: row for row in instances}
        missing = [iid for iid in wanted if iid not in by_id]
        if missing:
            print(f"Instancia no encontrada en el dataset: {', '.join(missing)}", file=sys.stderr)
            return 2
        instances = [by_id[iid] for iid in wanted]

    state = load_state()
    entries = state.setdefault("instances", {})

    prepared = 0
    attempted = 0
    try:
        for row in instances:
            iid = row["instance_id"]
            image = image_name(row)
            entry = entries.setdefault(iid, {})
            entry["image"] = image

            present = image_state(image)
            if present is None:
                print(f"❌ No se pudo inspeccionar la imagen {image}. ¿Docker está sano?", file=sys.stderr)
                return 2
            if present:
                if entry.get("status") != "ready" or entry.get("error"):
                    entry["status"] = "ready"
                    entry["ready_at"] = datetime.now(timezone.utc).isoformat()
                    entry.pop("error", None)
                    prepared += 1
                continue

            if args.check_only:
                entry.setdefault("status", "pending")
                continue

            if args.limit and attempted >= args.limit:
                break

            if free_gb() < args.min_free_gb:
                print(f"⚠ Espacio libre insuficiente ({free_gb():.1f} GB < {args.min_free_gb} GB). Deteniendo.")
                break

            print(f"⬇ {iid}  →  {image}")
            attempted += 1
            entry["status"] = "pulling"
            entry.pop("error", None)
            save_state(state)

            pull_cmd = ["docker", "pull"]
            if args.platform:
                pull_cmd += ["--platform", args.platform]
            pull_cmd.append(image)

            try:
                result = run(pull_cmd, timeout=args.timeout)
            except subprocess.TimeoutExpired:
                entry["status"] = "error"
                entry["error"] = f"timeout después de {args.timeout}s"
                print(f"  ✗ {entry['error']}")
                save_state(state)
                continue
            except KeyboardInterrupt:
                entry["status"] = "error"
                entry["error"] = "interrumpido por el usuario"
                save_state(state)
                raise

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
    except KeyboardInterrupt:
        print("\n⚠ Interrumpido por el usuario. Estado guardado.", file=sys.stderr)

    save_state(state)

    # Resumen acotado a las instancias del dataset actual (no a estados viejos).
    counts: dict[str, int] = {}
    for row in instances:
        entry = entries.get(row["instance_id"], {})
        status = entry.get("status", "pending")
        counts[status] = counts.get(status, 0) + 1

    print()
    print("Estado de preparación (dataset actual):")
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
