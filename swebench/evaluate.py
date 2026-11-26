#!/usr/bin/env python3
"""Etapa 4 — Evaluar predicciones con el script oficial de SWE-bench.

El dataset `SWE-bench/SWE-bench_Verified` incluye, por instancia, la imagen
Docker, el `eval_script` y el `log_parser` oficiales. Este script:

  1. aplica el parche del modelo dentro de `/testbed`,
  2. ejecuta el `eval_script` (que aplica el test_patch y lanza los tests),
  3. parsea la salida con el parser oficial,
  4. calcula FAIL_TO_PASS / PASS_TO_PASS y *resolved*.

No reimplementa los comandos de test de cada repo: usa el script del dataset,
que es la fuente de verdad. Tampoco usa la librería `swebench`: todo es local.

Uso:
    python3 swebench/evaluate.py --predictions swebench/data/predictions/qwen3.jsonl
    python3 swebench/evaluate.py --limit 5 --keep-going
    python3 swebench/evaluate.py --instance astropy__astropy-12907 --dry-run

Salida:
    swebench/data/evaluation/<run_id>/report.json
    swebench/data/evaluation/<run_id>/report.md
    swebench/data/evaluation/<run_id>/logs/<instance>.log
    swebench/data/evaluation/<run_id>/test_logs/<instance>.txt
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import traceback
import uuid
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent
DATA_DIR = ROOT / "data"
INSTANCES = DATA_DIR / "instances.jsonl"
STATE = DATA_DIR / "state.json"
PREDICTIONS = DATA_DIR / "predictions"
EVALUATION = DATA_DIR / "evaluation"

START_TEST_OUTPUT = ">>>>> Start Test Output"
END_TEST_OUTPUT = ">>>>> End Test Output"
TEST_EXIT_CODE = ">>>>> Test Exit Code"
APPLY_PATCH_FAIL = ">>>>> Patch Apply Failed"
RESET_FAILED = ">>>>> Reset Failed"
TESTS_ERROR = ">>>>> Tests Errored"
TESTS_TIMEOUT = ">>>>> Tests Timed Out"
MODEL_PATCH_FAIL = "LAI_MODEL_PATCH_APPLY_FAILED"

TEST_EXIT_CODE_RE = re.compile(rf"{re.escape(TEST_EXIT_CODE)}:\s*(-?\d+)")

# Evidencia de que el runner llegó a ejecutar tests. Se usa solo cuando el
# parser no devuelve ningún resultado: evita confundir "no corrió" con "todo
# pasó" (relevante en eval_type=fail_only).
SUITE_RAN = re.compile(
    r"Executed [1-9]\d* of \d+"
    r"|TOTAL: [1-9]\d* (?:SUCCESS|FAILED)"
    r"|[1-9]\d* passing"
    r"|[1-9]\d* passed"
    r"|Tests:\s+[1-9]\d*"
    r"|Test Suites:\s+(?:\d+ \w+, )*[1-9]\d* total"
    r"|^# tests [1-9]\d*"
    r"|[1-9]\d* specs?, \d+ failures?"
    r"|': ok$",
    re.M,
)

STATUSES = ("FAILED", "PASSED", "SKIPPED", "ERROR", "XFAIL")
BAD_CODES = (APPLY_PATCH_FAIL, RESET_FAILED, TESTS_ERROR, TESTS_TIMEOUT)


# ---------------------------------------------------------------------------
# Utilidades de archivos
# ---------------------------------------------------------------------------


def load_jsonl(path: Path) -> list[dict]:
    rows = []
    with path.open("r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def load_instances() -> dict[str, dict]:
    if not INSTANCES.exists():
        raise SystemExit(
            f"Falta {INSTANCES}. Ejecuta antes: python3 swebench/fetch_dataset.py --force"
        )
    out: dict[str, dict] = {}
    for row in load_jsonl(INSTANCES):
        iid = row.get("instance_id")
        if not iid:
            continue
        for key in ("FAIL_TO_PASS", "PASS_TO_PASS"):
            value = row.get(key)
            if isinstance(value, str):
                try:
                    row[key] = json.loads(value)
                except json.JSONDecodeError:
                    row[key] = []
        out[iid] = row
    return out


def load_predictions(path: Path) -> dict[str, dict]:
    if not path.exists():
        raise SystemExit(f"No existe el archivo de predicciones: {path}")
    out: dict[str, dict] = {}
    for row in load_jsonl(path):
        iid = row.get("instance_id")
        if not iid:
            print(f"⚠ predicción sin instance_id en {path}; se ignora", file=sys.stderr)
            continue
        if "model_patch" not in row:
            print(f"⚠ {iid}: predicción sin model_patch; se trata como vacía", file=sys.stderr)
            row["model_patch"] = ""
        out[iid] = row
    return out


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


def slug(value: str) -> str:
    # Dots are allowed inside names, but never as leading/trailing characters:
    # otherwise a user-supplied run_id like ".." could escape EVALUATION.
    cleaned = re.sub(r"[^a-zA-Z0-9._-]+", "_", value).strip("._-")
    return cleaned or "run"


def write_text_lf(path: Path, text: str) -> None:
    """Escribe siempre LF: Docker/Linux no deben recibir CRLF en parches."""
    path.write_bytes(text.replace("\r\n", "\n").replace("\r", "\n").encode("utf-8"))


def rel(path: Path) -> str:
    try:
        return str(path.relative_to(DATA_DIR))
    except ValueError:
        return str(path)


# ---------------------------------------------------------------------------
# Parsers oficiales (Python / SWE-bench Verified)
# ---------------------------------------------------------------------------

_SKIP_SUMMARY_COUNT = re.compile(r"^\[\d+\]$")


def _is_skip_summary(status: str, name: str) -> bool:
    return status == "SKIPPED" and bool(_SKIP_SUMMARY_COUNT.match(name))


def parse_log_pytest(log: str) -> dict[str, str]:
    status_map: dict[str, str] = {}
    for line in log.split("\n"):
        if any(line.startswith(status) for status in STATUSES):
            if line.startswith("FAILED"):
                line = line.replace(" - ", " ")
            parts = line.split()
            if len(parts) <= 1:
                continue
            if _is_skip_summary(parts[0], parts[1]):
                continue
            status_map[parts[1]] = parts[0]
    return status_map


def parse_log_pytest_options(log: str) -> dict[str, str]:
    option_pattern = re.compile(r"(.*?)\[(.*)\]")
    status_map: dict[str, str] = {}
    for line in log.split("\n"):
        if any(line.startswith(status) for status in STATUSES):
            if line.startswith("FAILED"):
                line = line.replace(" - ", " ")
            parts = line.split()
            if len(parts) <= 1:
                continue
            if _is_skip_summary(parts[0], parts[1]):
                continue
            match = option_pattern.search(parts[1])
            if match:
                main, option = match.groups()
                if option.startswith("/") and not option.startswith("//") and "*" not in option:
                    option = "/" + option.split("/")[-1]
                test_name = f"{main}[{option}]"
            else:
                test_name = parts[1]
            status_map[test_name] = parts[0]
    return status_map


def parse_log_django(log: str) -> dict[str, str]:
    status_map: dict[str, str] = {}
    prev_test = None
    for raw_line in log.split("\n"):
        line = raw_line.strip()

        if "--version is equivalent to version" in line:
            status_map["--version is equivalent to version"] = "PASSED"

        if " ... " in line:
            prev_test = line.split(" ... ")[0]

        for suffix in (" ... ok", " ... OK", " ...  OK"):
            if line.endswith(suffix):
                if line.strip().startswith(
                    "Applying sites.0002_alter_domain_unique...test_no_migrations"
                ):
                    line = line.split("...", 1)[-1].strip()
                test = line.rsplit(suffix, 1)[0]
                status_map[test] = "PASSED"
                break
        if " ... skipped" in line:
            status_map[line.split(" ... skipped")[0]] = "SKIPPED"
        if line.endswith(" ... FAIL"):
            status_map[line.split(" ... FAIL")[0]] = "FAILED"
        if line.startswith("FAIL:"):
            status_map[line.split()[1].strip()] = "FAILED"
        if line.endswith(" ... ERROR"):
            status_map[line.split(" ... ERROR")[0]] = "ERROR"
        if line.startswith("ERROR:"):
            status_map[line.split()[1].strip()] = "ERROR"
        if line.lstrip().startswith("ok") and prev_test is not None:
            status_map[prev_test] = "PASSED"

    patterns = [
        r"^(.*?)\s\.\.\.\sTesting\ against\ Django\ installed\ in\ ((?s:.*?))\ silenced\)\.\nok$",
        r"^(.*?)\s\.\.\.\sInternal\ Server\ Error:\ \/(.*)\/\nok$",
        r"^(.*?)\s\.\.\.\sSystem check identified no issues \(0 silenced\)\nok$",
    ]
    for pattern in patterns:
        for match in re.finditer(pattern, log, re.MULTILINE):
            status_map[match.group(1)] = "PASSED"
    return status_map


def parse_log_pytest_v2(log: str) -> dict[str, str]:
    status_map: dict[str, str] = {}
    escapes = "".join(chr(char) for char in range(1, 32))
    translator = str.maketrans("", "", escapes)
    for line in log.split("\n"):
        line = re.sub(r"\[(\d+)m", "", line)
        line = line.translate(translator)
        if any(line.startswith(status) for status in STATUSES):
            if line.startswith("FAILED"):
                line = line.split(" - ", 1)[0]
            parts = line.split()
            if len(parts) >= 2 and not _is_skip_summary(parts[0], parts[1]):
                status_map[" ".join(parts[1:])] = parts[0]
        elif any(line.endswith(status) for status in STATUSES):
            parts = line.split()
            if len(parts) >= 2:
                status_map[" ".join(parts[:-1])] = parts[-1]
    return status_map


def parse_log_seaborn(log: str) -> dict[str, str]:
    status_map: dict[str, str] = {}
    for line in log.split("\n"):
        parts = line.split()
        if len(parts) < 2:
            continue
        if line.startswith("FAILED"):
            status_map[parts[1]] = "FAILED"
        elif " PASSED " in line:
            if parts[1] == "PASSED":
                status_map[parts[0]] = "PASSED"
        elif line.startswith("PASSED"):
            status_map[parts[1]] = "PASSED"
    return status_map


def parse_log_sympy(log: str) -> dict[str, str]:
    status_map: dict[str, str] = {}
    for match in re.findall(r"(_*) (.*)\.py:(.*) (_*)", log):
        status_map[f"{match[1]}.py:{match[2]}"] = "FAILED"
    for raw_line in log.split("\n"):
        line = raw_line.strip()
        if line.startswith("test_"):
            if line.endswith(" E"):
                status_map[line.split()[0]] = "ERROR"
            if line.endswith(" F"):
                status_map[line.split()[0]] = "FAILED"
            if line.endswith(" ok"):
                status_map[line.split()[0]] = "PASSED"
    return status_map


def parse_log_matplotlib(log: str) -> dict[str, str]:
    status_map: dict[str, str] = {}
    for line in log.split("\n"):
        line = line.replace("MouseButton.LEFT", "1").replace("MouseButton.RIGHT", "3")
        if any(line.startswith(status) for status in STATUSES):
            if line.startswith("FAILED"):
                line = line.replace(" - ", " ")
            parts = line.split()
            if len(parts) <= 1:
                continue
            if _is_skip_summary(parts[0], parts[1]):
                continue
            status_map[parts[1]] = parts[0]
    return status_map


# Alias oficiales: el dataset trae el nombre del parser, no el repo.
PARSERS = {
    "parse_log_pytest": parse_log_pytest,
    "parse_log_pytest_options": parse_log_pytest_options,
    "parse_log_django": parse_log_django,
    "parse_log_pytest_v2": parse_log_pytest_v2,
    "parse_log_seaborn": parse_log_seaborn,
    "parse_log_sympy": parse_log_sympy,
    "parse_log_matplotlib": parse_log_matplotlib,
    "parse_log_astroid": parse_log_pytest,
    "parse_log_flask": parse_log_pytest,
    "parse_log_marshmallow": parse_log_pytest,
    "parse_log_pvlib": parse_log_pytest,
    "parse_log_pyvista": parse_log_pytest,
    "parse_log_sqlfluff": parse_log_pytest,
    "parse_log_xarray": parse_log_pytest,
    "parse_log_pydicom": parse_log_pytest_options,
    "parse_log_requests": parse_log_pytest_options,
    "parse_log_pylint": parse_log_pytest_options,
    "parse_log_astropy": parse_log_pytest_v2,
    "parse_log_scikit": parse_log_pytest_v2,
    "parse_log_sphinx": parse_log_pytest_v2,
}


# ---------------------------------------------------------------------------
# Grading oficial
# ---------------------------------------------------------------------------


def resolve_case(case: str, status_map: dict[str, str]) -> str | None:
    """Tolera ids parametrizados truncados (issue #290 de SWE-bench)."""
    if case in status_map:
        return case
    if case.count("[") > case.count("]"):
        matches = [key for key in status_map if key.startswith(case)]
        passing = {"PASSED", "XFAIL"}
        if matches and len({status_map[key] in passing for key in matches}) == 1:
            return matches[0]
    return None


def test_passed(case: str, status_map: dict[str, str]) -> bool:
    key = resolve_case(case, status_map)
    return key is not None and status_map[key] in ("PASSED", "XFAIL")


def test_maintained(case: str, status_map: dict[str, str]) -> bool:
    key = resolve_case(case, status_map)
    return test_passed(case, status_map) or (key is not None and status_map[key] == "SKIPPED")


def test_failed(case: str, status_map: dict[str, str]) -> bool:
    key = resolve_case(case, status_map)
    return key is None or status_map[key] in ("FAILED", "ERROR", "SKIPPED")


def grade(status_map: dict[str, str], instance: dict) -> dict:
    f2p_cases = instance.get("FAIL_TO_PASS") or []
    p2p_cases = instance.get("PASS_TO_PASS") or []
    eval_type = instance.get("eval_type") or "pass_and_fail"

    def check_pass_and_fail(case: str, success: list, failure: list) -> None:
        if test_passed(case, status_map):
            success.append(case)
        elif test_failed(case, status_map):
            failure.append(case)

    def check_maintained(case: str, success: list, failure: list) -> None:
        if test_maintained(case, status_map):
            success.append(case)
        elif test_failed(case, status_map):
            failure.append(case)

    def check_fail_only(case: str, success: list, failure: list) -> None:
        if case in status_map and status_map[case] == "FAILED":
            failure.append(case)
        else:
            success.append(case)

    check_f2p = check_pass_and_fail if eval_type == "pass_and_fail" else check_fail_only
    check_p2p = check_maintained if eval_type == "pass_and_fail" else check_fail_only

    f2p_success: list[str] = []
    f2p_failure: list[str] = []
    for case in f2p_cases:
        check_f2p(case, f2p_success, f2p_failure)

    p2p_success: list[str] = []
    p2p_failure: list[str] = []
    for case in p2p_cases:
        check_p2p(case, p2p_success, p2p_failure)

    f2p_total = len(f2p_success) + len(f2p_failure)
    p2p_total = len(p2p_success) + len(p2p_failure)
    f2p_ratio = len(f2p_success) / f2p_total if f2p_total else 1.0
    p2p_ratio = len(p2p_success) / p2p_total if p2p_total else 1.0

    if f2p_ratio == 1 and p2p_ratio == 1:
        resolution = "RESOLVED_FULL"
        resolved = True
    elif 0 < f2p_ratio < 1 and p2p_ratio == 1:
        resolution = "RESOLVED_PARTIAL"
        resolved = False
    else:
        resolution = "RESOLVED_NO"
        resolved = False

    return {
        "resolved": resolved,
        "resolution": resolution,
        "eval_type": eval_type,
        "FAIL_TO_PASS": {"success": f2p_success, "failure": f2p_failure, "ratio": f2p_ratio},
        "PASS_TO_PASS": {"success": p2p_success, "failure": p2p_failure, "ratio": p2p_ratio},
    }


def parse_test_exit_code(content: str) -> int | None:
    match = TEST_EXIT_CODE_RE.search(content)
    return int(match.group(1)) if match else None


def evaluate_log(instance: dict, content: str) -> tuple[dict[str, str] | None, dict]:
    """Devuelve (status_map, error). Si error no es None, no se puntúa."""
    parser_name = instance.get("log_parser") or ""
    parser = PARSERS.get(parser_name)
    if parser is None:
        return None, {"status": "unknown_log_parser", "error": f"parser desconocido: {parser_name}"}

    if MODEL_PATCH_FAIL in content:
        return None, {"status": "apply_failed", "error": "el parche del modelo no se pudo aplicar"}
    if APPLY_PATCH_FAIL in content:
        return None, {
            "status": "test_patch_apply_failed",
            "error": "el test_patch oficial no se pudo aplicar",
        }
    for marker in (RESET_FAILED, TESTS_ERROR, TESTS_TIMEOUT):
        if marker in content:
            return None, {"status": "eval_error", "error": f"el eval_script reportó: {marker}"}

    if START_TEST_OUTPUT not in content or END_TEST_OUTPUT not in content:
        return None, {
            "status": "no_test_output",
            "error": "faltan los marcadores de inicio/fin de tests en la salida",
        }

    sliced = content.split(START_TEST_OUTPUT, 1)[1].split(END_TEST_OUTPUT, 1)[0]
    status_map = parser(sliced)
    if not status_map:
        # Algunos runners emiten resultados fuera de los marcadores.
        status_map = parser(content)
    if not status_map and not SUITE_RAN.search(content):
        return None, {
            "status": "no_test_results",
            "error": "el parser no encontró resultados y no hay evidencia de que los tests corrieran",
        }

    test_exit = parse_test_exit_code(content)
    if (
        test_exit not in (None, 0)
        and status_map
        and not any(status in ("FAILED", "ERROR") for status in status_map.values())
    ):
        return None, {
            "status": "inconsistent_exit_code",
            "error": f"el comando de tests salió con {test_exit} pero el log no reporta fallos",
        }

    return status_map, {}


# ---------------------------------------------------------------------------
# Construcción del script y ejecución Docker
# ---------------------------------------------------------------------------


def inject_test_exit_capture(script: str) -> tuple[str, bool]:
    """Añade la captura del $? del comando de tests antes del marcador final.

    El eval_script oficial termina reseteando archivos con git checkout, así que
    el exit code del script no sirve. El dataset nuevo ya imprime
    `>>>>> Test Exit Code`; en datasets antiguos lo inyectamos aquí.
    """
    if TEST_EXIT_CODE in script:
        return script, False

    lines = script.splitlines(keepends=True)
    out: list[str] = []
    injected = False
    for line in lines:
        if not injected and END_TEST_OUTPUT in line:
            out.append("SWEBENCH_TEST_EXIT_CODE=$?\n")
            out.append(f'echo "{TEST_EXIT_CODE}: ${{SWEBENCH_TEST_EXIT_CODE}}"\n')
            injected = True
        out.append(line)
    return "".join(out), injected


def build_eval_script(instance: dict) -> tuple[str, bool]:
    eval_script = (instance.get("eval_script") or "").lstrip("\ufeff")
    eval_script, injected = inject_test_exit_capture(eval_script)
    wrapper = (
        "#!/bin/bash\n"
        "set -uxo pipefail\n"
        "cd /testbed || { echo \"" + MODEL_PATCH_FAIL + "\"; exit 86; }\n"
        "if ! git apply -v --whitespace=nowarn /tmp/lai_model.patch; then\n"
        f"  echo \"{MODEL_PATCH_FAIL}\"\n"
        "  exit 86\n"
        "fi\n"
        "# ---- eval_script oficial del dataset ----\n"
    )
    if not eval_script.endswith("\n"):
        eval_script += "\n"
    return wrapper + eval_script, injected


def docker_available() -> bool:
    import docker_env

    return docker_env.check()


def image_present(image: str) -> bool:
    try:
        result = subprocess.run(
            ["docker", "image", "inspect", image],
            capture_output=True,
            text=True,
            timeout=60,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return False
    return result.returncode == 0


def remove_container(name: str) -> None:
    try:
        subprocess.run(
            ["docker", "rm", "-f", name],
            capture_output=True,
            text=True,
            timeout=30,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass


def run_container(
    instance: dict,
    patch: str,
    script: str,
    *,
    run_id: str,
    timeout: int,
    platform: str | None,
    network: str,
) -> tuple[str, int, float, str, str]:
    """Ejecuta el contenedor. Devuelve (stdout+stderr, rc, duración, container, error)."""
    iid = instance["instance_id"]
    image = instance["image"]
    name = f"lai-eval-{slug(run_id)[:24]}-{slug(iid)[:32]}-{uuid.uuid4().hex[:6]}"

    with tempfile.TemporaryDirectory(prefix="lai-eval-") as tmp:
        tmp_path = Path(tmp)
        patch_path = tmp_path / "model.patch"
        script_path = tmp_path / "eval.sh"
        write_text_lf(patch_path, patch if patch.endswith("\n") else patch + "\n")
        write_text_lf(script_path, script)

        cmd = ["docker", "run", "--rm", "--name", name]
        if platform:
            cmd += ["--platform", platform]
        cmd += [
            "--network",
            network,
            "--mount",
            f"type=bind,source={patch_path.resolve()},target=/tmp/lai_model.patch,readonly",
            "--mount",
            f"type=bind,source={script_path.resolve()},target=/tmp/lai_eval.sh,readonly",
            image,
            "bash",
            "/tmp/lai_eval.sh",
        ]

        started = time.time()
        try:
            proc = subprocess.run(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=timeout,
            )
            output, rc = proc.stdout or "", proc.returncode
            error = ""
        except subprocess.TimeoutExpired as exc:
            output = exc.stdout or ""
            if isinstance(output, bytes):
                output = output.decode("utf-8", errors="replace")
            rc = 124
            error = f"timeout después de {timeout}s"
            remove_container(name)
        except FileNotFoundError:
            output, rc, error = "", 127, "docker no encontrado"
        except KeyboardInterrupt:
            remove_container(name)
            raise
        duration = time.time() - started
        return output, rc, duration, name, error


# ---------------------------------------------------------------------------
# Evaluación de una instancia
# ---------------------------------------------------------------------------


def evaluate_instance(
    instance: dict,
    prediction: dict,
    *,
    run_id: str,
    run_dir: Path,
    timeout: int,
    platform: str | None,
    network: str,
) -> dict:
    iid = instance["instance_id"]
    patch = prediction.get("model_patch") or ""
    started = time.time()

    result: dict = {
        "instance_id": iid,
        "model": prediction.get("model_name_or_path"),
        "image": instance.get("image"),
        "parser": instance.get("log_parser"),
        "eval_type": instance.get("eval_type") or "pass_and_fail",
        "patch_bytes": len(patch),
        "status": "pending",
        "resolved": False,
        "resolution": "RESOLVED_NO",
        "scored": False,
        "test_exit_code": None,
        "duration_s": 0.0,
        "error": "",
        "log": "",
        "test_log": "",
        "FAIL_TO_PASS": None,
        "PASS_TO_PASS": None,
    }

    if not patch.strip():
        result.update(status="empty_patch", error="el modelo no produjo parche")
        result["duration_s"] = round(time.time() - started, 1)
        return result

    script, injected = build_eval_script(instance)
    if not injected and TEST_EXIT_CODE not in script:
        print(f"⚠ {iid}: no se encontró el marcador final de tests; se ejecuta igual", file=sys.stderr)

    output, rc, duration, container, run_error = run_container(
        instance,
        patch,
        script,
        run_id=run_id,
        timeout=timeout,
        platform=platform,
        network=network,
    )
    result["duration_s"] = round(duration, 1)

    logs_dir = run_dir / "logs"
    test_logs_dir = run_dir / "test_logs"
    logs_dir.mkdir(parents=True, exist_ok=True)
    test_logs_dir.mkdir(parents=True, exist_ok=True)
    log_path = logs_dir / f"{slug(iid)}.log"
    log_path.write_text(
        f"# instance: {iid}\n# run_id: {run_id}\n# image: {instance.get('image')}\n"
        f"# rc: {rc}\n# duration_s: {duration:.1f}\n# container: {container}\n"
        f"\n{output}\n",
        encoding="utf-8",
    )
    result["log"] = rel(log_path)

    if START_TEST_OUTPUT in output and END_TEST_OUTPUT in output:
        test_output = output.split(START_TEST_OUTPUT, 1)[1].split(END_TEST_OUTPUT, 1)[0]
        test_log_path = test_logs_dir / f"{slug(iid)}.txt"
        test_log_path.write_text(test_output, encoding="utf-8")
        result["test_log"] = rel(test_log_path)

    if run_error:
        result.update(status="timeout" if rc == 124 else "infra_error", error=run_error)
        return result
    if rc in (125, 126, 127) and START_TEST_OUTPUT not in output:
        result.update(status="infra_error", error=f"docker/entorno devolvió rc={rc}")
        return result

    status_map, eval_error = evaluate_log(instance, output)
    result["test_exit_code"] = parse_test_exit_code(output)
    if eval_error:
        result.update(eval_error)
        result["scored"] = False
        return result

    assert status_map is not None
    report = grade(status_map, instance)
    result.update(report)
    result["status"] = "evaluated"
    result["scored"] = True
    result["tests_parsed"] = len(status_map)
    return result


# ---------------------------------------------------------------------------
# Informes
# ---------------------------------------------------------------------------


def write_reports(run_dir: Path, run_meta: dict, results: list[dict]) -> None:
    scored = [r for r in results if r.get("scored")]
    resolved = [r for r in scored if r.get("resolved")]
    infra = [r for r in results if r.get("status") in ("infra_error", "timeout")]
    summary = {
        "instances": len(results),
        "scored": len(scored),
        "resolved": len(resolved),
        "resolution_rate": (len(resolved) / len(scored)) if scored else None,
        "infra_errors": len(infra),
        "empty_patch": sum(1 for r in results if r.get("status") == "empty_patch"),
        "status_counts": {},
    }
    for result in results:
        status = result.get("status", "unknown")
        summary["status_counts"][status] = summary["status_counts"].get(status, 0) + 1

    report = {"run": run_meta, "summary": summary, "instances": results}
    (run_dir / "report.json").write_text(
        json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )

    lines = [
        f"# Evaluación SWE-bench Verified — {run_meta['run_id']}",
        "",
        f"- Predicciones: `{run_meta['predictions']}`",
        f"- Modelo: `{run_meta.get('model') or 'desconocido'}`",
        f"- Instancias evaluadas: {summary['instances']}",
        f"- Puntuadas: {summary['scored']}",
        f"- Resueltas: {summary['resolved']}",
    ]
    if summary["resolution_rate"] is not None:
        lines.append(f"- Tasa de resolución: {summary['resolution_rate']:.1%}")
    else:
        lines.append("- Tasa de resolución: no calculable (sin instancias puntuadas)")
    lines += [
        f"- Errores de infraestructura: {summary['infra_errors']}",
        f"- Parches vacíos: {summary['empty_patch']}",
        "",
        "| Instancia | Estado | Resuelta | F2P | P2P | Exit tests | Tiempo |",
        "|-----------|--------|----------|-----|-----|------------|--------|",
    ]
    for result in results:
        f2p = result.get("FAIL_TO_PASS")
        p2p = result.get("PASS_TO_PASS")
        f2p_text = f"{len(f2p['success'])}/{len(f2p['success']) + len(f2p['failure'])}" if f2p else "-"
        p2p_text = f"{len(p2p['success'])}/{len(p2p['success']) + len(p2p['failure'])}" if p2p else "-"
        exit_text = "-" if result.get("test_exit_code") is None else str(result["test_exit_code"])
        lines.append(
            f"| `{result['instance_id']}` | {result.get('status')} | "
            f"{'sí' if result.get('resolved') else 'no'} | {f2p_text} | {p2p_text} | "
            f"{exit_text} | {result.get('duration_s', 0):.1f}s |"
        )
    lines += [
        "",
        "> Solo se puntúan las instancias con tests parseados. Los errores de",
        "> infraestructura y los parches vacíos no cuentan como resueltos ni se",
        "> incluyen en la tasa de resolución.",
        "",
    ]
    (run_dir / "report.md").write_text("\n".join(lines), encoding="utf-8")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def choose_predictions(path: str | None) -> Path:
    if path:
        return Path(path)
    files = sorted(PREDICTIONS.glob("*.jsonl"))
    if len(files) == 1:
        return files[0]
    if not files:
        raise SystemExit(
            f"No hay predicciones en {PREDICTIONS}. Ejecuta antes run_eval.py o pasa --predictions."
        )
    listing = "\n".join(f"  {p}" for p in files)
    raise SystemExit(f"Hay varias predicciones; elige una con --predictions:\n{listing}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--predictions", default=None, help="JSONL de predicciones (autodetecta si hay una)")
    parser.add_argument("--limit", type=int, default=0, help="Máximo de instancias a evaluar")
    parser.add_argument("--instance", action="append", default=None, help="Instancia concreta (repetible)")
    parser.add_argument("--run-id", default=None, help="Nombre de la corrida (por defecto timestamp+modelo)")
    parser.add_argument("--timeout", type=int, default=1800, help="Timeout por instancia en segundos")
    parser.add_argument("--platform", default=None, help="Plataforma Docker, p. ej. linux/amd64")
    parser.add_argument("--network", default="none", help="Red del contenedor (por defecto none)")
    parser.add_argument("--keep-going", action="store_true", help="Continuar tras un error de infraestructura")
    parser.add_argument("--skip-preflight", action="store_true", help="No comprobar Docker/imágenes")
    parser.add_argument("--dry-run", action="store_true", help="Validar y listar sin ejecutar Docker")
    parser.add_argument("--force", action="store_true", help="Permitir reutilizar un run_id existente")
    return parser.parse_args()


REQUIRED_FIELDS = ("image", "eval_script", "log_parser", "FAIL_TO_PASS", "PASS_TO_PASS")

#: Listas que deben existir pero pueden estar vacías. En el dataset oficial,
#: varias instancias (p. ej. django__django-12209, sphinx-doc__sphinx-8595)
#: traen `PASS_TO_PASS: []` y eso es legítimo: no hay tests de regresión
#: declarados, sólo los que deben pasar a pasar.
ALLOW_EMPTY_LIST_FIELDS = ("FAIL_TO_PASS", "PASS_TO_PASS")


def validate_instances(instances: dict[str, dict], ids: list[str]) -> list[str]:
    """Campos imprescindibles para evaluar. Devuelve una lista de errores legibles.

    Se comprueba antes de tocar Docker: sin `eval_script` o `log_parser` no hay
    evaluación oficial posible y no tiene sentido gastar minutos de contenedor.
    """
    errors = []
    for iid in ids:
        instance = instances.get(iid) or {}
        missing = []
        for field in REQUIRED_FIELDS:
            value = instance.get(field)
            if value is None or value == "":
                missing.append(field)
            elif isinstance(value, list) and not value and field not in ALLOW_EMPTY_LIST_FIELDS:
                missing.append(field)
        if missing:
            errors.append(f"{iid}: faltan {', '.join(missing)}")
    return errors


def main() -> int:
    args = parse_args()

    instances = load_instances()
    pred_path = choose_predictions(args.predictions)
    predictions = load_predictions(pred_path)

    selected_ids = [iid for iid in predictions if iid in instances]
    unknown = [iid for iid in predictions if iid not in instances]
    if unknown:
        print(f"⚠ {len(unknown)} predicciones no están en el dataset; se ignoran", file=sys.stderr)

    if args.instance:
        wanted = []
        for value in args.instance:
            wanted.extend(part.strip() for part in value.split(",") if part.strip())
        missing = [iid for iid in wanted if iid not in predictions]
        if missing:
            print(f"❌ Sin predicción para: {', '.join(missing)}", file=sys.stderr)
            return 2
        selected_ids = [iid for iid in wanted if iid in predictions]
    if args.limit and args.limit > 0:
        selected_ids = selected_ids[: args.limit]

    if not selected_ids:
        print("No hay instancias seleccionadas.", file=sys.stderr)
        return 2

    model = predictions[selected_ids[0]].get("model_name_or_path")
    run_id = slug(
        args.run_id
        or f"{datetime.now(timezone.utc).strftime('%Y%m%d-%H%M%S')}-{slug(str(model or 'model'))}"
    )
    run_dir = EVALUATION / run_id

    # Validar campos imprescindibles antes de tocar Docker.
    errors = validate_instances(instances, selected_ids)
    if errors:
        print("❌ Datos incompletos. Vuelve a descargar el dataset con --force:", file=sys.stderr)
        for error in errors[:10]:
            print(f"  {error}", file=sys.stderr)
        return 2

    print(f"Run: {run_id}")
    print(f"Predicciones: {pred_path}")
    print(f"Modelo: {model}")
    print(f"Instancias: {len(selected_ids)}")
    if args.dry_run:
        for iid in selected_ids:
            instance = instances[iid]
            print(
                f"  {iid}  image={instance['image']}  parser={instance['log_parser']}  "
                f"F2P={len(instance.get('FAIL_TO_PASS') or [])}  "
                f"P2P={len(instance.get('PASS_TO_PASS') or [])}"
            )
        return 0

    if not args.skip_preflight:
        if not docker_available():
            print("Inicia Docker y vuelve a intentarlo, o usa --skip-preflight.", file=sys.stderr)
            return 2
        missing_images = [
            instances[iid]["image"] for iid in selected_ids if not image_present(instances[iid]["image"])
        ]
        if missing_images:
            print("❌ Faltan imágenes Docker. Prepáralas con:", file=sys.stderr)
            print("  python3 swebench/prepare.py --limit N", file=sys.stderr)
            for image in missing_images[:10]:
                print(f"  {image}", file=sys.stderr)
            return 2

    if run_dir.exists() and not args.force:
        print(f"❌ Ya existe {run_dir}. Usa otro --run-id o --force.", file=sys.stderr)
        return 2
    run_dir.mkdir(parents=True, exist_ok=True)

    state = load_state()
    entries = state.setdefault("instances", {})
    results: list[dict] = []
    run_meta = {
        "run_id": run_id,
        "predictions": str(pred_path),
        "model": model,
        "started_at": datetime.now(timezone.utc).isoformat(),
        "timeout_s": args.timeout,
        "platform": args.platform,
        "network": args.network,
        "instances": len(selected_ids),
    }

    try:
        for index, iid in enumerate(selected_ids, 1):
            instance = instances[iid]
            prediction = predictions[iid]
            print(f"[{index}/{len(selected_ids)}] 🔎 {iid}")
            try:
                result = evaluate_instance(
                    instance,
                    prediction,
                    run_id=run_id,
                    run_dir=run_dir,
                    timeout=args.timeout,
                    platform=args.platform,
                    network=args.network,
                )
            except Exception as exc:  # noqa: BLE001 - una instancia no debe abortar el run
                log_dir = run_dir / "logs"
                log_dir.mkdir(parents=True, exist_ok=True)
                log_path = log_dir / f"{slug(iid)}.log"
                log_path.write_text(traceback.format_exc(), encoding="utf-8")
                result = {
                    "instance_id": iid,
                    "status": "infra_error",
                    "resolved": False,
                    "resolution": None,
                    "error": f"excepción no controlada: {type(exc).__name__}: {exc}",
                    "duration_s": 0.0,
                    "patch_bytes": len(prediction.get("model_patch") or ""),
                    "test_exit_code": None,
                    "FAIL_TO_PASS": None,
                    "PASS_TO_PASS": None,
                    "log": rel(log_path),
                }
            results.append(result)
            status = result["status"]
            resolved = "✓ resuelta" if result.get("resolved") else "· no resuelta"
            print(
                f"   {status:20s} {resolved:12s} "
                f"{result.get('duration_s', 0):7.1f}s  patch={result.get('patch_bytes', 0)}B"
            )
            if result.get("error"):
                print(f"   ⚠ {result['error']}")

            entry = entries.setdefault(iid, {})
            entry["evaluation"] = {
                "run_id": run_id,
                "status": status,
                "resolved": bool(result.get("resolved")),
                "resolution": result.get("resolution"),
                "at": datetime.now(timezone.utc).isoformat(),
                "report": rel(run_dir / "report.json"),
            }
            save_state(state)

            if status in ("infra_error", "timeout") and not args.keep_going:
                print("❌ Error de infraestructura; deteniendo. Usa --keep-going para continuar.", file=sys.stderr)
                break
    except KeyboardInterrupt:
        print("\n⚠ Interrumpido por el usuario. Se guardará el informe parcial.", file=sys.stderr)

    run_meta["finished_at"] = datetime.now(timezone.utc).isoformat()
    write_reports(run_dir, run_meta, results)
    print()
    print(f"Informe: {run_dir / 'report.md'}")
    print(f"JSON:    {run_dir / 'report.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
