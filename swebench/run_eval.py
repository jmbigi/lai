#!/usr/bin/env python3
"""Etapa 3 — Ejecutar lai dentro del contenedor y capturar el parche.

Para cada instancia preparada:
  1. arranca su imagen oficial (la que indica el dataset),
  2. ejecuta `lai auto "<problem_statement>"` dentro de /testbed,
  3. captura `git diff` como predicción.

Es incremental: las instancias que ya tienen predicción se saltan salvo --force.
Los errores de infraestructura (Docker/imagen/binario) NO se escriben como
predicciones vacías: se marcan en el sidecar `.meta.json` para no contaminar la
tasa de resolución. Un timeout del agente sí se registra como parche vacío,
porque es un fallo del agente dentro de su presupuesto.

Uso:
    python3 swebench/run_eval.py --limit 1
    python3 swebench/run_eval.py --instance astropy__astropy-12907 --max-iterations 8
    python3 swebench/run_eval.py --limit 5 --model qwen3-coder-opt:latest
    python3 swebench/run_eval.py --skip-preflight --platform linux/amd64

Salida:
    swebench/data/predictions/<modelo>.jsonl       formato oficial de SWE-bench
    swebench/data/predictions/<modelo>.meta.json   estado/duración por instancia
    swebench/data/logs/<modelo>/<instance>.log     salida completa del agente
"""

from __future__ import annotations

import argparse
import json
import re
import shlex
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
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
AGENT_RC = "LAI_AGENT_EXIT_CODE"

LOCAL_HOSTS = {"127.0.0.1", "localhost", "::1", "host.docker.internal"}


def image_name(instance: dict) -> str:
    """Imagen oficial del dataset; fallback para datos antiguos."""
    return instance.get("image") or (
        f"swebench/sweb.eval.x86_64.{instance['instance_id'].replace('__', '_1776_')}".lower()
    )


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
        try:
            return json.loads(STATE.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            pass
    return {"instances": {}}


def save_state(state: dict) -> None:
    state["updated_at"] = datetime.now(timezone.utc).isoformat()
    tmp = STATE.with_suffix(".tmp")
    tmp.write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    tmp.replace(STATE)


def save_meta(path: Path, meta: dict) -> None:
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(meta, indent=2) + "\n", encoding="utf-8")
    tmp.replace(path)


def slug(model: str) -> str:
    return re.sub(r"[^a-zA-Z0-9._-]+", "_", model).strip("_") or "model"


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
        "- Use the tools to inspect the code and write the corrected versions.\n"
        "- For large repositories, use grep to locate the relevant symbols and read_file "
        "with start_line/end_line instead of reading whole files.\n"
        "- Change only what is necessary to fix the issue.\n"
        "- Do not modify test files.\n"
        "- When you are done, reply with a short summary of the change.\n\n"
        "ISSUE:\n"
        f"{instance['problem_statement']}\n"
    )


# ---------------------------------------------------------------------------
# Preflight: Docker, Ollama, modelo y contexto
# ---------------------------------------------------------------------------


def run(cmd: list[str], timeout: int = 60) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)


def docker_available() -> bool:
    try:
        result = run(["docker", "info", "--format", "{{.ServerVersion}}"])
    except (FileNotFoundError, subprocess.TimeoutExpired) as exc:
        print(f"❌ Docker no está disponible: {exc}", file=sys.stderr)
        return False
    if result.returncode != 0:
        print(f"❌ Docker no está disponible: {(result.stderr or result.stdout).strip()[:300]}", file=sys.stderr)
        return False
    return True


def image_present(image: str) -> bool:
    try:
        return run(["docker", "image", "inspect", image]).returncode == 0
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return False


def ollama_root(base_url: str) -> str:
    parsed = urllib.parse.urlparse(base_url)
    path = parsed.path.rstrip("/")
    if path.endswith("/v1"):
        path = path[:-3]
    return urllib.parse.urlunparse((parsed.scheme, parsed.netloc, path, "", "", "")).rstrip("/")


def http_json(url: str, payload: dict | None = None, timeout: int = 5) -> dict | None:
    data = None
    headers = {}
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(url, data=data, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return json.loads(response.read().decode("utf-8", errors="replace"))
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError, OSError):
        return None


def preflight_ollama(base_url: str, model: str) -> bool:
    host = (urllib.parse.urlparse(base_url).hostname or "").lower()
    if host not in LOCAL_HOSTS:
        print(
            f"⚠ base-url no es loopback ({host}). Regla 0: la inferencia debe ser local; "
            "continúa bajo tu responsabilidad.",
            file=sys.stderr,
        )

    models = http_json(base_url.rstrip("/") + "/models")
    if models is None:
        print(f"❌ No se pudo consultar Ollama en {base_url}.", file=sys.stderr)
        print("   Arranca Ollama y comprueba --base-url.", file=sys.stderr)
        return False
    ids = [item.get("id") for item in models.get("data", []) if isinstance(item, dict)]
    if model not in ids:
        print(f"❌ El modelo '{model}' no está disponible en Ollama.", file=sys.stderr)
        if ids:
            print("   Modelos disponibles: " + ", ".join(ids[:20]), file=sys.stderr)
        print(f"   Prueba: ollama pull {model}", file=sys.stderr)
        return False

    # Contexto: Ollama usa 4096 por defecto si el modelo no está cargado.
    root = ollama_root(base_url)
    ps = http_json(root + "/api/ps") or {}
    context = None
    for item in ps.get("models", []):
        names = {item.get("name"), item.get("model")}
        if model in names or any(name and name.split(":")[0] == model.split(":")[0] for name in names):
            context = item.get("context_length") or item.get("context")
            break
    if context is None:
        show = http_json(root + "/api/show", {"model": model}) or {}
        match = re.search(r"num_ctx\s+(\d+)", show.get("parameters") or "")
        if match:
            context = int(match.group(1))

    if context is None:
        print(
            "⚠ No se pudo leer el contexto de Ollama. Si no lo has cambiado, usa 4096 tokens "
            "y el agente puede perder el hilo.\n"
            "   Recomendado para SWE-bench: OLLAMA_CONTEXT_LENGTH=16384 (reinicia Ollama).",
            file=sys.stderr,
        )
    elif context < 8192:
        print(
            f"⚠ Contexto de Ollama: {context} tokens (< 8192). El agente probablemente pierda "
            "el hilo en repos grandes.\n"
            "   Recomendado: OLLAMA_CONTEXT_LENGTH=16384 y reiniciar Ollama.",
            file=sys.stderr,
        )
    return True


def preflight(instances: list[dict], base_url: str, model: str) -> bool:
    if not docker_available():
        return False
    if not preflight_ollama(base_url, model):
        return False
    missing = [inst for inst in instances if not image_present(image_name(inst))]
    if missing:
        print("❌ Faltan imágenes Docker. Prepáralas con:", file=sys.stderr)
        for inst in missing[:10]:
            print(f"  python3 swebench/prepare.py --instance {inst['instance_id']}", file=sys.stderr)
        if len(missing) > 10:
            print(f"  ... y {len(missing) - 10} más", file=sys.stderr)
        return False
    return True


# ---------------------------------------------------------------------------
# Ejecución
# ---------------------------------------------------------------------------


def run_instance(
    instance: dict,
    model: str,
    base_url: str,
    max_iter: int,
    timeout: int,
    platform: str | None,
    network: str,
) -> dict:
    iid = instance["instance_id"]
    image = image_name(instance)
    problem = build_task(instance)

    # Archivo único por ejecución: evita colisiones entre corridas paralelas.
    problem_dir = LOGS / "problems"
    problem_dir.mkdir(parents=True, exist_ok=True)
    problem_file = problem_dir / f"{slug(model)}__{slug(iid)}__{uuid.uuid4().hex[:8]}.txt"
    problem_file.write_text(problem, encoding="utf-8", newline="\n")

    inner = (
        "set -uo pipefail; "
        "cd /testbed || exit 90; "
        "git checkout -- . 2>/dev/null || true; git clean -fdq 2>/dev/null || true; "
        f"lai --model {shlex.quote(model)} --base-url {shlex.quote(base_url)} "
        'auto "$(cat /tmp/lai_problem.txt)" '
        f"--max-iterations {int(max_iter)} 2>&1; "
        "LAI_RC=$?; "
        "git add -A 2>/dev/null || true; "
        f"echo {DIFF_BEGIN}; git diff --cached; echo {DIFF_END}; "
        f'echo "{AGENT_RC}=$LAI_RC"; exit 0'
    )

    container = f"lai-run-{slug(model)[:20]}-{slug(iid)[:28]}-{uuid.uuid4().hex[:6]}"
    cmd = ["docker", "run", "--rm", "--name", container]
    if platform:
        cmd += ["--platform", platform]
    cmd += [
        "--network",
        network,
        "--mount",
        f"type=bind,source={LAI_BIN.resolve()},target=/usr/local/bin/lai,readonly",
        "--mount",
        f"type=bind,source={problem_file.resolve()},target=/tmp/lai_problem.txt,readonly",
        image,
        "bash",
        "-lc",
        inner,
    ]

    started = time.time()
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=timeout,
        )
        stdout, stderr, rc = proc.stdout or "", proc.stderr or "", proc.returncode
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout or ""
        stderr = exc.stderr or ""
        if isinstance(stdout, bytes):
            stdout = stdout.decode("utf-8", errors="replace")
        if isinstance(stderr, bytes):
            stderr = stderr.decode("utf-8", errors="replace")
        stderr += f"\nTIMEOUT after {timeout}s"
        rc = 124
        try:
            run(["docker", "rm", "-f", container], timeout=30)
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass
    except FileNotFoundError:
        stdout, stderr, rc = "", "docker no encontrado", 127
    duration = time.time() - started

    patch = ""
    if DIFF_BEGIN in stdout and DIFF_END in stdout:
        patch = stdout.split(DIFF_BEGIN, 1)[1].rsplit(DIFF_END, 1)[0].strip()

    agent_rc = None
    match = re.search(rf"^{re.escape(AGENT_RC)}=(-?\d+)$", stdout, re.M)
    if match:
        agent_rc = int(match.group(1))

    log_dir = LOGS / slug(model)
    log_dir.mkdir(parents=True, exist_ok=True)
    log_path = log_dir / f"{slug(iid)}.log"
    log_path.write_text(
        f"# instance: {iid}\n# model: {model}\n# image: {image}\n# rc: {rc}\n"
        f"# agent_rc: {agent_rc}\n# duration_s: {duration:.1f}\n"
        f"\n===== STDOUT =====\n{stdout}\n\n===== STDERR =====\n{stderr}\n",
        encoding="utf-8",
    )

    # Clasificación. Un error de Docker/entorno no debe convertirse en un
    # parche vacío: eso falsearía la tasa de resolución.
    infra_error = ""
    combined = (stdout + stderr).lower()
    markers_present = DIFF_BEGIN in stdout and DIFF_END in stdout
    if rc == 124:
        status = "timeout"
    elif rc in (125, 126, 127) and not markers_present:
        status = "infra_error"
        infra_error = f"docker/entorno devolvió rc={rc}"
    elif not markers_present and (
        "error while loading shared libraries" in combined
        or ("glibc" in combined and ("not found" in combined or "version" in combined))
    ):
        status = "infra_error"
        infra_error = "el binario lai requiere una glibc más nueva que la de la imagen"
    elif "lai: command not found" in (stdout + stderr) or "exec format error" in combined:
        status = "infra_error"
        infra_error = "el binario lai no se pudo ejecutar dentro del contenedor"
    elif not markers_present:
        status = "infra_error"
        infra_error = "no aparecieron los marcadores de diff; el contenedor no completó el guion"
    elif not patch:
        status = "no_patch"
    else:
        status = "patched"

    return {
        "instance_id": iid,
        "status": status,
        "rc": rc,
        "agent_rc": agent_rc,
        "duration_s": round(duration, 1),
        "patch": patch,
        "patch_bytes": len(patch),
        "infra_error": infra_error,
        "log": str(log_path.relative_to(DATA_DIR)),
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--limit", type=int, default=0, help="Máximo de instancias nuevas")
    parser.add_argument("--instance", action="append", default=None, help="Solo esta instancia (repetible)")
    parser.add_argument("--model", default="qwen3-coder-opt:latest", help="Modelo local (ollama)")
    parser.add_argument("--base-url", default="http://127.0.0.1:11434/v1")
    parser.add_argument("--max-iterations", type=int, default=10)
    parser.add_argument("--timeout", type=int, default=1800, help="Timeout por instancia (s)")
    parser.add_argument("--platform", default=None, help="Plataforma Docker, p. ej. linux/amd64")
    parser.add_argument("--network", default="host", help="Red del contenedor (host para Ollama)")
    parser.add_argument("--skip-preflight", action="store_true", help="No comprobar Docker/Ollama/imágenes")
    parser.add_argument("--keep-going", action="store_true", help="Continuar tras un error de infraestructura")
    parser.add_argument("--force", action="store_true", help="Repetir aunque ya haya predicción")
    args = parser.parse_args()

    if not LAI_BIN.exists():
        print(f"Falta {LAI_BIN}. Ejecuta antes build_lai_for_swebench.sh", file=sys.stderr)
        return 2
    if not INSTANCES.exists():
        print(f"Falta {INSTANCES}. Ejecuta antes fetch_dataset.py --force", file=sys.stderr)
        return 2

    LOGS.mkdir(parents=True, exist_ok=True)
    PREDICTIONS.mkdir(parents=True, exist_ok=True)
    model_slug = slug(args.model)
    pred_path = PREDICTIONS / f"{model_slug}.jsonl"
    meta_path = PREDICTIONS / f"{model_slug}.meta.json"

    done: dict[str, dict] = {}
    if pred_path.exists():
        for row in load_jsonl(pred_path):
            if row.get("instance_id"):
                done[row["instance_id"]] = row

    meta: dict[str, dict] = {}
    if meta_path.exists():
        try:
            meta = json.loads(meta_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            meta = {}

    instances = load_jsonl(INSTANCES)
    state = load_state()
    entries = state.setdefault("instances", {})

    if args.instance:
        wanted = []
        for value in args.instance:
            wanted.extend(part.strip() for part in value.split(",") if part.strip())
        by_id = {inst["instance_id"]: inst for inst in instances}
        missing = [iid for iid in wanted if iid not in by_id]
        if missing:
            print(f"Instancia no encontrada: {', '.join(missing)}", file=sys.stderr)
            return 2
        instances = [by_id[iid] for iid in wanted]

    pending: list[dict] = []
    for instance in instances:
        iid = instance["instance_id"]
        if iid in done and not args.force:
            continue
        if args.limit and len(pending) >= args.limit:
            break
        pending.append(instance)

    if not pending:
        print("No hay instancias pendientes para este modelo.")
    elif not args.skip_preflight:
        # Solo comprobamos las imágenes que realmente se van a usar.
        if not preflight(pending, args.base_url, args.model):
            print("Usa --skip-preflight para saltarte estas comprobaciones.", file=sys.stderr)
            return 2

    ran = 0
    infra_failed = False
    for instance in pending:
        iid = instance["instance_id"]
        print(f"🤖 {iid}  ({args.model})")
        if args.force:
            done.pop(iid, None)

        result = run_instance(
            instance,
            args.model,
            args.base_url,
            args.max_iterations,
            args.timeout,
            args.platform,
            args.network,
        )
        ran += 1

        print(
            f"   {result['status']:11s} rc={result['rc']:<4d} agent_rc={str(result['agent_rc']):<4s} "
            f"{result['duration_s']:7.1f}s  patch={result['patch_bytes']}B"
        )
        if result["infra_error"]:
            print(f"   ⚠ {result['infra_error']}")

        if result["status"] == "infra_error":
            infra_failed = True
            meta[iid] = {
                "status": result["status"],
                "rc": result["rc"],
                "agent_rc": result["agent_rc"],
                "duration_s": result["duration_s"],
                "patch_bytes": 0,
                "error": result["infra_error"],
                "model": args.model,
                "at": datetime.now(timezone.utc).isoformat(),
                "log": result["log"],
            }
        else:
            # timeout, no_patch y patched producen una predicción (posiblemente vacía).
            done[iid] = {
                "instance_id": iid,
                "model_name_or_path": args.model,
                "model_patch": result["patch"],
            }
            meta[iid] = {
                "status": result["status"],
                "rc": result["rc"],
                "agent_rc": result["agent_rc"],
                "duration_s": result["duration_s"],
                "patch_bytes": result["patch_bytes"],
                "model": args.model,
                "at": datetime.now(timezone.utc).isoformat(),
                "log": result["log"],
            }

        with pred_path.open("w", encoding="utf-8", newline="\n") as fh:
            for row in done.values():
                fh.write(json.dumps(row, ensure_ascii=False) + "\n")
        save_meta(meta_path, meta)

        entry = entries.setdefault(iid, {})
        entry["eval"] = meta[iid]
        save_state(state)

        if infra_failed and not args.keep_going:
            print("❌ Error de infraestructura; deteniendo. Usa --keep-going para continuar.", file=sys.stderr)
            break

    # Resumen acotado a este archivo de predicciones/modelo.
    counts: dict[str, int] = {}
    for row in meta.values():
        if row.get("model") == args.model:
            status = row.get("status", "unknown")
            counts[status] = counts.get(status, 0) + 1

    print()
    print("Predicciones:")
    for status, count in sorted(counts.items()):
        print(f"  {status:11s} {count}")
    print(f"  archivo: {pred_path}")
    print(f"  metadatos: {meta_path}")
    print()
    print("Siguiente etapa: python3 swebench/evaluate.py --predictions", pred_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
