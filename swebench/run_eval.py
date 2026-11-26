#!/usr/bin/env python3
"""Etapa 3 — Ejecutar lai dentro del contenedor y capturar el parche.

Para cada instancia preparada:
  1. arranca su imagen (Ubuntu 22.04 + repo en el commit base),
  2. ejecuta `lai auto "<problem_statement>"` dentro de /testbed,
  3. captura `git diff` como predicción.

Es incremental: las instancias que ya tienen predicción se saltan salvo --force.

Uso:
    python3 swebench/run_eval.py --limit 1
    python3 swebench/run_eval.py --instance astropy__astropy-12907 --max-iterations 8
    python3 swebench/run_eval.py --limit 5 --model qwen3-coder-opt:latest

Salida:
    swebench/data/predictions/<modelo>.jsonl   formato oficial de SWE-bench
    swebench/data/logs/<instance_id>.log       salida completa del agente
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent
DATA_DIR = ROOT / "data"
INSTANCES = DATA_DIR / "instances.jsonl"
STATE = DATA_DIR / "state.json"
LOGS = DATA_DIR / "logs"
PREDICTIONS = DATA_DIR / "predictions"
LAI_BIN = ROOT / "bin" / "lai"

DIFF_BEGIN = "LAI_DIFF_BEGIN_MARKER"
DIFF_END = "LAI_DIFF_END_MARKER"


def image_name(instance_id: str) -> str:
    return f"swebench/sweb.eval.x86_64.{instance_id.replace('__', '_1776_')}".lower()


def load_jsonl(path: Path) -> list[dict]:
    rows = []
    with path.open("r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def load_state() -> dict:
    if STATE.exists():
        return json.loads(STATE.read_text(encoding="utf-8"))
    return {"instances": {}}


def save_state(state: dict) -> None:
    state["updated_at"] = datetime.now(timezone.utc).isoformat()
    tmp = STATE.with_suffix(".tmp")
    tmp.write_text(json.dumps(state, indent=2), encoding="utf-8")
    tmp.replace(STATE)


def slug(model: str) -> str:
    return re.sub(r"[^a-zA-Z0-9._-]+", "_", model)


def build_task(instance: dict) -> str:
    """Enmarca el issue como tarea de ingeniería, igual que hacen los agentes SWE-bench.

    Sin este encuadre los modelos pequeños responden con un análisis en prosa en
    lugar de modificar el repositorio.
    """
    return (
        "You are an expert software engineer working inside the repository at /testbed "
        "(your current working directory).\n\n"
        "Fix the issue described below by editing the project's source files.\n"
        "Rules:\n"
        "- Use the tools to read files and write the corrected versions.\n"
        "- Change only what is necessary to fix the issue.\n"
        "- Do not modify test files.\n"
        "- When you are done, reply with a short summary of the change.\n\n"
        "ISSUE:\n"
        f"{instance['problem_statement']}\n"
    )


def run_instance(instance: dict, model: str, base_url: str, max_iter: int, timeout: int) -> dict:
    iid = instance["instance_id"]
    image = image_name(iid)
    problem = build_task(instance)

    # El enunciado va por archivo para no pelearse con el quoting del shell.
    problem_file = DATA_DIR / "logs" / f"{iid}.problem.txt"
    problem_file.parent.mkdir(parents=True, exist_ok=True)
    problem_file.write_text(problem, encoding="utf-8")

    inner = (
        "set -e; cd /testbed; "
        "git checkout -- . 2>/dev/null || true; git clean -fdq 2>/dev/null || true; "
        f"lai --model {model} --base-url {base_url} "
        'auto "$(cat /tmp/lai_problem.txt)" '
        f"--max-iterations {max_iter} 2>&1; "
        "git add -A; "
        f"echo \"{DIFF_BEGIN}\"; git diff --cached; echo \"{DIFF_END}\""
    )

    cmd = [
        "docker", "run", "--rm",
        "--network", "host",                       # alcanza el ollama del host en 127.0.0.1
        "-v", f"{LAI_BIN}:/usr/local/bin/lai:ro",
        "-v", f"{problem_file}:/tmp/lai_problem.txt:ro",
        image,
        "bash", "-lc", inner,
    ]

    started = time.time()
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        stdout, stderr, rc = proc.stdout, proc.stderr, proc.returncode
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout or ""
        stderr = f"TIMEOUT after {timeout}s"
        rc = 124
    duration = time.time() - started

    # Extraer el diff entre marcadores
    patch = ""
    if DIFF_BEGIN in stdout and DIFF_END in stdout:
        patch = stdout.split(DIFF_BEGIN, 1)[1].rsplit(DIFF_END, 1)[0].strip()

    log_path = LOGS / f"{iid}.log"
    log_path.write_text(
        f"# instance: {iid}\n# model: {model}\n# rc: {rc}\n# duration_s: {duration:.1f}\n"
        f"\n===== STDOUT =====\n{stdout}\n\n===== STDERR =====\n{stderr}\n",
        encoding="utf-8",
    )

    if rc == 124:
        status = "timeout"
    elif not patch:
        status = "no_patch"
    else:
        status = "patched"

    return {
        "instance_id": iid,
        "status": status,
        "rc": rc,
        "duration_s": round(duration, 1),
        "patch": patch,
        "patch_bytes": len(patch),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--limit", type=int, default=0, help="Máximo de instancias nuevas")
    parser.add_argument("--instance", default=None, help="Solo esta instancia")
    parser.add_argument("--model", default="qwen3-coder-opt:latest", help="Modelo local (ollama)")
    parser.add_argument("--base-url", default="http://127.0.0.1:11434/v1")
    parser.add_argument("--max-iterations", type=int, default=10)
    parser.add_argument("--timeout", type=int, default=1800, help="Timeout por instancia (s)")
    parser.add_argument("--force", action="store_true", help="Repetir aunque ya haya predicción")
    args = parser.parse_args()

    if not LAI_BIN.exists():
        print(f"Falta {LAI_BIN}. Ejecuta antes build_lai_for_swebench.sh", file=sys.stderr)
        return 2
    if not INSTANCES.exists():
        print(f"Falta {INSTANCES}. Ejecuta antes fetch_dataset.py", file=sys.stderr)
        return 2

    LOGS.mkdir(parents=True, exist_ok=True)
    PREDICTIONS.mkdir(parents=True, exist_ok=True)
    pred_path = PREDICTIONS / f"{slug(args.model)}.jsonl"

    # Predicciones existentes (para reanudar)
    done: dict[str, dict] = {}
    if pred_path.exists():
        for row in load_jsonl(pred_path):
            done[row["instance_id"]] = row

    instances = load_jsonl(INSTANCES)
    state = load_state()
    entries = state.get("instances", {})

    if args.instance:
        instances = [i for i in instances if i["instance_id"] == args.instance]
        if not instances:
            print(f"Instancia no encontrada: {args.instance}", file=sys.stderr)
            return 2

    ran = 0
    for instance in instances:
        iid = instance["instance_id"]
        entry = entries.get(iid, {})
        if entry.get("status") != "ready":
            continue
        if iid in done and not args.force:
            continue
        if args.limit and ran >= args.limit:
            break

        print(f"🤖 {iid}  ({args.model})")
        result = run_instance(instance, args.model, args.base_url, args.max_iterations, args.timeout)
        ran += 1

        print(
            f"   {result['status']:9s} rc={result['rc']:<4d} "
            f"{result['duration_s']:7.1f}s  patch={result['patch_bytes']}B"
        )

        # Guardar predicción en formato oficial (solo si hay parche)
        if result["patch"]:
            done[iid] = {
                "instance_id": iid,
                "model_name_or_path": args.model,
                "model_patch": result["patch"],
            }
        else:
            # Parche vacío: se registra igual para que el harness lo cuente como fallo.
            done[iid] = {
                "instance_id": iid,
                "model_name_or_path": args.model,
                "model_patch": "",
            }

        with pred_path.open("w", encoding="utf-8") as fh:
            for row in done.values():
                fh.write(json.dumps(row, ensure_ascii=False) + "\n")

        entry["eval"] = {
            "status": result["status"],
            "rc": result["rc"],
            "duration_s": result["duration_s"],
            "patch_bytes": result["patch_bytes"],
            "model": args.model,
            "at": datetime.now(timezone.utc).isoformat(),
        }
        entries[iid] = entry
        save_state(state)

    # Resumen
    counts: dict[str, int] = {}
    for row in done.values():
        pass
    for iid, entry in entries.items():
        ev = entry.get("eval")
        if ev:
            counts[ev["status"]] = counts.get(ev["status"], 0) + 1

    print()
    print("Predicciones:")
    for status, count in sorted(counts.items()):
        print(f"  {status:9s} {count}")
    print(f"  archivo: {pred_path}")
    print()
    print("Siguiente etapa: python3 swebench/evaluate.py --predictions", pred_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
