#!/usr/bin/env python3
"""Estima la capacidad SWE-bench de un modelo local a partir de una corrida real.

Implementa la **Regla 12** (`DOCUMENTACION_REGLAS.md`): toda afirmación de
capacidad SWE debe salir de una corrida medida, con denominador explícito,
intervalo de confianza y tamaño de muestra. Este script no inventa resultados:
solo lee un `report.json` producido por `swebench/evaluate.py`.

Qué calcula
-----------
- **Tasa extremo a extremo**: resueltas / instancias intentadas.
- **Tasa sobre puntuables**: resueltas / parches evaluables (excluye vacíos).
- **Intervalo de Wilson 95 %** para ambas (mejor que el normal con n pequeño y
  proporciones cercanas a 0 o 1).
- **Regla de tres** cuando no hay ninguna resuelta: cota superior aproximada.
- **Desglose por repositorio** y tasa de errores de infraestructura.
- **Tamaño de muestra** necesario para un margen de error dado.
- **Extrapolación a 500** solo si se pide explícitamente (`--extrapolate`) y
  marcada como extrapolación.

Uso
---
    python swebench/estimate.py --run-id qwen3-8b-final
    python swebench/estimate.py --model qwen3-8b --md
    python swebench/estimate.py --report swebench/data/evaluation/x/report.json --json
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
EVALUATION = HERE / "data" / "evaluation"

# Por debajo de este n, la Regla 12 obliga a etiquetar el resultado como
# preliminar en lugar de presentarlo como una tasa.
MIN_N_FOR_CLAIM = 30
# Total de SWE-bench Verified; solo para extrapolaciones declaradas.
FULL_DATASET = 500


def wilson_interval(successes: int, total: int, z: float = 1.959963984540054) -> tuple[float, float]:
    """Intervalo de confianza de Wilson para una proporción.

    Con n pequeño y p cerca de 0 o 1 el intervalo normal (Wald) da límites
    imposibles (negativos o > 1) y anchos falsamente estrechos. Wilson no.
    Devuelve (0.0, 0.0) si total == 0.
    """
    if total <= 0:
        return (0.0, 0.0)
    p = successes / total
    denominator = 1 + z * z / total
    centre = (p + z * z / (2 * total)) / denominator
    margin = (z / denominator) * math.sqrt(p * (1 - p) / total + z * z / (4 * total * total))
    return (max(0.0, centre - margin), min(1.0, centre + margin))


def rule_of_three(total: int) -> float:
    """Cota superior aproximada del 95 % cuando hay 0 éxitos en `total` intentos."""
    return 3.0 / total if total > 0 else 1.0


def sample_size_for_margin(p: float, margin: float, z: float = 1.959963984540054) -> int:
    """n necesario para estimar `p` con ±`margin` (aprox. normal, sin corrección)."""
    if margin <= 0:
        raise ValueError("margin debe ser > 0")
    return math.ceil((z * z * p * (1 - p)) / (margin * margin))


def load_report(run_id: str | None, model: str | None, report_path: str | None) -> tuple[str, dict]:
    if report_path:
        path = Path(report_path)
        if not path.exists():
            raise SystemExit(f"estimate: no existe {path}")
        return path.parent.name, json.loads(path.read_text(encoding="utf-8"))

    if run_id:
        path = EVALUATION / run_id / "report.json"
        if not path.exists():
            raise SystemExit(f"estimate: no existe {path}")
        return run_id, json.loads(path.read_text(encoding="utf-8"))

    candidates = []
    if EVALUATION.is_dir():
        for path in EVALUATION.glob("*/report.json"):
            try:
                data = json.loads(path.read_text(encoding="utf-8"))
            except (json.JSONDecodeError, OSError):
                continue
            run = data.get("run") or {}
            if model is None or run.get("model") == model:
                candidates.append((path.parent.name, data))
    if not candidates:
        raise SystemExit(
            "estimate: no hay corridas evaluadas. "
            "Ejecuta antes: python swebench/evaluate.py --predictions ..."
        )
    candidates.sort(
        key=lambda item: (item[1].get("run") or {}).get("finished_at")
        or (item[1].get("run") or {}).get("started_at")
        or "",
        reverse=True,
    )
    return candidates[0]


def build_estimate(run_id: str, report: dict) -> dict:
    run = report.get("run") or {}
    summary = report.get("summary") or {}
    instances = report.get("instances") or []

    attempted = int(summary.get("instances") or len(instances) or 0)
    scored = int(summary.get("scored") or 0)
    resolved = int(summary.get("resolved") or 0)
    empty = int(summary.get("empty_patch") or 0)
    infra = int(summary.get("infra_errors") or 0)

    e2e = resolved / attempted if attempted else 0.0
    on_scored = resolved / scored if scored else 0.0
    e2e_ci = wilson_interval(resolved, attempted)
    scored_ci = wilson_interval(resolved, scored)

    # Regla de tres: con 0 resueltas no se puede decir "0 %"; se da una cota.
    upper_bound = rule_of_three(attempted) if resolved == 0 and attempted else None

    per_repo: dict[str, dict] = {}
    for instance in instances:
        repo = (instance.get("instance_id") or "?").split("__")[0]
        entry = per_repo.setdefault(repo, {"attempted": 0, "scored": 0, "resolved": 0})
        entry["attempted"] += 1
        if instance.get("scored"):
            entry["scored"] += 1
        if instance.get("resolved"):
            entry["resolved"] += 1

    return {
        "run_id": run_id,
        "model": run.get("model"),
        "started_at": run.get("started_at"),
        "finished_at": run.get("finished_at"),
        "network": run.get("network"),
        "timeout_s": run.get("timeout_s"),
        "attempted": attempted,
        "scored": scored,
        "resolved": resolved,
        "empty_patch": empty,
        "infra_errors": infra,
        "end_to_end_rate": e2e,
        "end_to_end_ci95": list(e2e_ci),
        "scored_rate": on_scored,
        "scored_ci95": list(scored_ci),
        "upper_bound_95": upper_bound,
        "preliminary": attempted < MIN_N_FOR_CLAIM,
        "per_repo": per_repo,
        "sample_sizes": {
            "margin_5pp_p20": sample_size_for_margin(0.2, 0.05),
            "margin_10pp_p20": sample_size_for_margin(0.2, 0.10),
            "margin_5pp_p50": sample_size_for_margin(0.5, 0.05),
            "margin_10pp_p50": sample_size_for_margin(0.5, 0.10),
        },
    }


def format_pct(value: float) -> str:
    return f"{value * 100:.1f} %"


def render_text(est: dict) -> str:
    lines = [
        f"Estimación SWE — corrida '{est['run_id']}'",
        f"  modelo:   {est['model']}",
        f"  ventana:  {est['started_at']} → {est['finished_at']}",
        f"  red:      {est['network']}    timeout: {est['timeout_s']} s",
        "",
        f"  intentadas:      {est['attempted']}",
        f"  parches evaluables: {est['scored']}  (vacíos: {est['empty_patch']})",
        f"  resueltas:       {est['resolved']}",
        f"  errores infra:   {est['infra_errors']}",
        "",
        "Tasas (con intervalo de Wilson 95 %):",
        f"  extremo a extremo: {format_pct(est['end_to_end_rate'])} "
        f"[{format_pct(est['end_to_end_ci95'][0])} – {format_pct(est['end_to_end_ci95'][1])}]"
        f"  = {est['resolved']}/{est['attempted']}",
        f"  sobre evaluables:  {format_pct(est['scored_rate'])} "
        f"[{format_pct(est['scored_ci95'][0])} – {format_pct(est['scored_ci95'][1])}]"
        f"  = {est['resolved']}/{est['scored']}",
    ]
    if est["upper_bound_95"] is not None:
        lines.append(
            f"  0 resueltas: la cota superior al 95 % es ≈ {format_pct(est['upper_bound_95'])} "
            f"(regla de tres); NO significa 0 %."
        )
    lines.append("")
    lines.append("Por repositorio:")
    for repo, entry in sorted(est["per_repo"].items()):
        lines.append(
            f"  {repo:<16} {entry['resolved']}/{entry['attempted']} "
            f"(evaluables {entry['scored']})"
        )
    lines.append("")
    sizes = est["sample_sizes"]
    lines.append("Muestra necesaria para estimar una tasa:")
    lines.append(f"  ±5 pp con p≈20 %:  n ≥ {sizes['margin_5pp_p20']}")
    lines.append(f"  ±10 pp con p≈20 %: n ≥ {sizes['margin_10pp_p20']}")
    lines.append(f"  ±5 pp con p≈50 %:  n ≥ {sizes['margin_5pp_p50']}")
    lines.append(f"  ±10 pp con p≈50 %: n ≥ {sizes['margin_10pp_p50']}")
    lines.append("")
    if est["preliminary"]:
        lines.append(
            f"⚠ PRELIMINAR: n={est['attempted']} < {MIN_N_FOR_CLAIM}. "
            "Es una estimación, no una tasa representativa (Regla 12)."
        )
    else:
        lines.append(
            f"✔ n={est['attempted']} ≥ {MIN_N_FOR_CLAIM}: se puede reportar como tasa, "
            "con el intervalo indicado."
        )
    return "\n".join(lines)


def render_markdown(est: dict) -> str:
    low, high = est["end_to_end_ci95"]
    slow, shigh = est["scored_ci95"]
    rows = [
        "| Métrica | Valor |",
        "|---|---:|",
        f"| Instancias intentadas | {est['attempted']} |",
        f"| Parches evaluables | {est['scored']} |",
        f"| Parches vacíos | {est['empty_patch']} |",
        f"| Resueltas | {est['resolved']} |",
        f"| Errores de infraestructura | {est['infra_errors']} |",
        f"| Tasa extremo a extremo | {format_pct(est['end_to_end_rate'])} "
        f"[{format_pct(low)} – {format_pct(high)}] |",
        f"| Tasa sobre evaluables | {format_pct(est['scored_rate'])} "
        f"[{format_pct(slow)} – {format_pct(shigh)}] |",
    ]
    out = [
        f"**Estimación SWE — `{est['model']}` (corrida `{est['run_id']}`)**",
        "",
        *rows,
        "",
        "Intervalos de Wilson al 95 %. "
        + (
            f"n={est['attempted']} < {MIN_N_FOR_CLAIM}: **preliminar**, no representativo."
            if est["preliminary"]
            else f"n={est['attempted']} ≥ {MIN_N_FOR_CLAIM}: tasa reportable."
        ),
    ]
    return "\n".join(out)


def extrapolate(est: dict) -> dict:
    """Extrapolación declarada a las 500 instancias de SWE-bench Verified.

    No es una predicción: asume que la muestra es representativa, cosa que con
    n pequeño no se puede sostener. Se devuelve junto con esa advertencia.
    """
    rate = est["end_to_end_rate"]
    low, high = est["end_to_end_ci95"]
    return {
        "assumption": "la muestra es representativa del dataset completo",
        "expected_resolved": round(rate * FULL_DATASET),
        "interval_resolved": [round(low * FULL_DATASET), round(high * FULL_DATASET)],
        "warning": (
            "Extrapolación, no medición. Con n pequeño el intervalo es tan ancho "
            "que el valor esperado no distingue modelos."
        ),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--run-id", help="ID de la corrida evaluada")
    parser.add_argument("--model", help="Modelo; usa la corrida evaluada más reciente")
    parser.add_argument("--report", help="Ruta directa a un report.json")
    parser.add_argument("--json", action="store_true", help="Salida JSON")
    parser.add_argument("--md", action="store_true", help="Salida markdown")
    parser.add_argument(
        "--extrapolate",
        action="store_true",
        help=f"Extrapolar a las {FULL_DATASET} instancias (marcado como extrapolación)",
    )
    args = parser.parse_args(argv)

    run_id, report = load_report(args.run_id, args.model, args.report)
    estimate = build_estimate(run_id, report)
    if args.extrapolate:
        estimate["extrapolation"] = extrapolate(estimate)

    if args.json:
        print(json.dumps(estimate, indent=2, ensure_ascii=False))
    elif args.md:
        print(render_markdown(estimate))
        if args.extrapolate:
            ext = estimate["extrapolation"]
            print(
                f"\n> Extrapolación a {FULL_DATASET}: ≈ {ext['expected_resolved']} resueltas "
                f"[{ext['interval_resolved'][0]} – {ext['interval_resolved'][1]}]. "
                f"{ext['warning']}"
            )
    else:
        print(render_text(estimate))
        if args.extrapolate:
            ext = estimate["extrapolation"]
            print("")
            print(
                f"Extrapolación a {FULL_DATASET} (declarada): ≈ {ext['expected_resolved']} "
                f"[{ext['interval_resolved'][0]} – {ext['interval_resolved'][1]}]"
            )
            print(f"  ⚠ {ext['warning']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
