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
import os
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

# Cuando la salida va a un archivo (job en segundo plano) Python la bufferiza por
# bloques y el proceso parece colgado hasta que termina. Medido el 2026-11-26:
# un piloto de 22 minutos no mostró ni una línea de progreso.
try:
    sys.stdout.reconfigure(line_buffering=True)
except (AttributeError, ValueError):  # pragma: no cover - stdout no reconfigurable
    pass

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
    # Nombre único: varias corridas en paralelo no deben chocar en el temporal.
    tmp = STATE.with_suffix(f".{os.getpid()}.{uuid.uuid4().hex[:6]}.tmp")
    tmp.write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    try:
        tmp.replace(STATE)
    finally:
        if tmp.exists():
            tmp.unlink(missing_ok=True)


def save_meta(path: Path, meta: dict) -> None:
    tmp = path.with_suffix(f"{path.suffix}.{os.getpid()}.{uuid.uuid4().hex[:6]}.tmp")
    tmp.write_text(json.dumps(meta, indent=2) + "\n", encoding="utf-8")
    try:
        tmp.replace(path)
    finally:
        if tmp.exists():
            tmp.unlink(missing_ok=True)


def slug(model: str) -> str:
    value = re.sub(r"[^a-zA-Z0-9._-]+", "_", model)
    return value.strip("._-") or "model"


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
    import docker_env

    return docker_env.check()


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


LOOPBACK_HOSTS = {"127.0.0.1", "localhost", "::1", "0.0.0.0"}


def container_base_url_warning(base_url: str) -> str | None:
    """Aviso informativo si el base-url se va a traducir para el contenedor.

    Medido el 2026-11-26 en Docker Desktop (Windows): con `--network host` el
    contenedor NO ve el loopback del host, así que `127.0.0.1:11434` falla.
    `host.docker.internal` sí resuelve en ambos modos de red. La traducción es
    automática; el aviso solo deja constancia para que el log sea interpretable.
    """
    host = (urllib.parse.urlparse(base_url).hostname or "").lower()
    if host in LOOPBACK_HOSTS:
        return (
            f"ℹ base-url {host}: dentro del contenedor es el propio contenedor, "
            "así que se traduce automáticamente a host.docker.internal."
        )
    return None


def container_base_url(base_url: str) -> str:
    """Traduce el base-url para usarlo DENTRO del contenedor.

    Medido el 2026-11-26 en Docker Desktop (Windows): con `--network host` el
    contenedor no ve el loopback del host, así que `127.0.0.1:11434` no llega a
    Ollama; `host.docker.internal` sí resuelve en red host y en bridge. En Linux
    nativo `--network host` sí comparte el loopback, y esta traducción también
    funciona porque Docker añade la entrada.
    """
    parsed = urllib.parse.urlparse(base_url)
    host = (parsed.hostname or "").lower()
    if host not in LOOPBACK_HOSTS:
        return base_url
    netloc = "host.docker.internal"
    if parsed.port:
        netloc += f":{parsed.port}"
    return urllib.parse.urlunparse(parsed._replace(netloc=netloc))


def read_context(model: str, base_url: str) -> int | None:
    """Longitud de contexto REAL del modelo, o None si no se puede saber.

    `/api/ps` solo informa de los modelos cargados; `/api/show` puede declarar
    `num_ctx`. No se asume ningún valor por defecto: es preferible un None
    honesto a un 4096 inventado.
    """
    root = ollama_root(base_url)
    ps = http_json(root + "/api/ps") or {}
    for item in ps.get("models", []):
        names = {item.get("name"), item.get("model")}
        if model in names or any(name and name.split(":")[0] == model.split(":")[0] for name in names):
            value = item.get("context_length") or item.get("context")
            if value:
                return int(value)
    show = http_json(root + "/api/show", {"model": model}) or {}
    match = re.search(r"num_ctx\s+(\d+)", show.get("parameters") or "")
    if match:
        return int(match.group(1))
    return None


def preflight_ollama(base_url: str, model: str) -> bool:
    host = (urllib.parse.urlparse(base_url).hostname or "").lower()
    if host not in LOCAL_HOSTS:
        print(
            f"⚠ base-url no es loopback ({host}). Regla 0: la inferencia debe ser local; "
            "continúa bajo tu responsabilidad.",
            file=sys.stderr,
        )
    container_warning = container_base_url_warning(base_url)
    if container_warning:
        print(container_warning, file=sys.stderr)

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

    # Contexto: si el modelo no está cargado no se puede leer de /api/ps, así
    # que se consulta /api/show. No se asume ningún valor por defecto.
    context = read_context(model, base_url)

    if context is None:
        print(
            "ℹ No se pudo determinar el contexto de Ollama (el modelo no está cargado y "
            "/api/show no lo declara).\n"
            "   No se asume ningún valor: compruébalo con `ollama ps` durante una corrida.",
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


def extract_patch(stdout: str) -> str:
    """Devuelve el diff capturado entre los marcadores ("" si no hay marcadores).

    Se usa el primer par de marcadores: si el agente (o un test) imprime los
    marcadores otra vez, no se arrastra contenido de más.
    """
    start = stdout.find(DIFF_BEGIN)
    if start == -1:
        return ""
    start += len(DIFF_BEGIN)
    end = stdout.find(DIFF_END, start)
    if end == -1:
        return ""
    return stdout[start:end].strip()


def classify_result(stdout: str, stderr: str, rc: int) -> tuple[str, str]:
    """Clasifica el resultado de una ejecución Docker.

    Devuelve ``(estado, motivo)``. Un error de Docker/entorno NO debe convertirse
    en un parche vacío: eso falsearía la tasa de resolución. Por eso se distingue
    ``infra_error`` de ``no_patch`` y de ``timeout``.

    Es una función pura para poder probarla sin Docker.
    """
    combined = (stdout + stderr).lower()
    markers_present = DIFF_BEGIN in stdout and DIFF_END in stdout

    if rc == 124:
        return "timeout", ""
    if rc == 91:
        return "infra_error", "no se pudo copiar el binario lai dentro del contenedor"
    if rc == 90:
        return "infra_error", "/testbed no existe en la imagen"
    # La guarda de la Regla 4 aborta antes de generar ningún parche. Si aparece
    # aquí es que el binario no acepta el endpoint del contenedor: error de
    # configuración, no del modelo.
    if "regla 4" in combined and "no es local" in combined:
        return "infra_error", "el binario lai rechazó el endpoint por la guarda de la Regla 4"
    # Los fallos de enlazado dinámico suelen salir con 127, así que se comprueban
    # antes que el mensaje genérico de rc.
    if not markers_present and (
        "error while loading shared libraries" in combined
        or ("glibc" in combined and ("not found" in combined or "version" in combined))
    ):
        return "infra_error", "el binario lai requiere una glibc más nueva que la de la imagen"
    if "lai: command not found" in (stdout + stderr) or "exec format error" in combined:
        return "infra_error", "el binario lai no se pudo ejecutar dentro del contenedor"
    if rc in (125, 126, 127) and not markers_present:
        return "infra_error", f"docker/entorno devolvió rc={rc}"
    if not markers_present:
        return "infra_error", "no aparecieron los marcadores de diff; el contenedor no completó el guion"
    if not extract_patch(stdout):
        return "no_patch", ""
    return "patched", ""


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
        # El binario llega por bind mount; en Windows el bit de ejecución no
        # viaja con el archivo, así que se copia y se marca explícitamente.
        "cp /tmp/lai_bin/lai /usr/local/bin/lai && chmod +x /usr/local/bin/lai || exit 91; "
        "cd /testbed || exit 90; "
        "git checkout -- . 2>/dev/null || true; git clean -fdq 2>/dev/null || true; "
        f"lai --model {shlex.quote(model)} --base-url {shlex.quote(container_base_url(base_url))} "
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
        # as_posix(): Docker Desktop no acepta bien las barras invertidas en
        # --mount type=bind,source=...
        # El destino es un ARCHIVO dentro de /tmp: si se monta sobre el
        # directorio, `cp` falla con "Not a directory".
        f"type=bind,source={LAI_BIN.resolve().as_posix()},target=/tmp/lai_bin/lai,readonly",
        "--mount",
        f"type=bind,source={problem_file.resolve().as_posix()},target=/tmp/lai_problem.txt,readonly",
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

    patch = extract_patch(stdout)

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
    status, infra_error = classify_result(stdout, stderr, rc)

    # Contexto REAL usado. Se consulta después de la corrida porque /api/ps solo
    # informa de modelos ya cargados. Regla 11: sin este dato la medición no es
    # reproducible.
    context = read_context(model, base_url)

    return {
        "instance_id": iid,
        "status": status,
        "rc": rc,
        "agent_rc": agent_rc,
        "duration_s": round(duration, 1),
        "patch": patch,
        "patch_bytes": len(patch),
        "infra_error": infra_error,
        "context_length": context,
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
                "context_length": result.get("context_length"),
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
                "context_length": result.get("context_length"),
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
    print("Resumen de esta corrida:")
    for status, count in sorted(counts.items()):
        print(f"  {status:11s} {count}")
    print(f"  metadatos: {meta_path}")

    written = sum(count for status, count in counts.items() if status != "infra_error")
    if written:
        print(f"  predicciones escritas: {written}  ({pred_path})")
        print()
        print("Siguiente etapa: python3 swebench/evaluate.py --predictions", pred_path)
    else:
        print()
        print(
            "No se escribió ninguna predicción: todos los intentos fallaron por "
            "infraestructura. Revisa los logs antes de gastar otra corrida.",
            file=sys.stderr,
        )
    return 1 if infra_failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
