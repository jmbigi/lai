#!/usr/bin/env python3
"""Compara dos corridas SWE-bench bajo la Regla 12.

La Regla 12 prohíbe comparar modelos si no comparten **n y harness**. Este
script verifica esa condición y, si se cumple, calcula la diferencia de tasas
con un error estándar **robusto a clusters** (repositorios) y un test de dos
proporciones. Si no se cumple, se niega a comparar en lugar de producir una
cifra que parezca una conclusión.

Uso:
    python swebench/compare.py --a <run_id_o_report.json> --b <...>
    python swebench/compare.py --a qwen3-8b-final --b qwen3-8b-20261128-local

Salida: texto o `--json`. Código de salida 2 si las corridas no son comparables.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import estimate  # noqa: E402

# Campos del harness que deben coincidir para que la comparación tenga sentido.
HARNESS_FIELDS = ("network", "timeout_s")


def load_any(spec: str) -> tuple[str, dict]:
    path = Path(spec)
    if path.suffix == ".json" and path.exists():
        return path.parent.name, json.loads(path.read_text(encoding="utf-8"))
    return estimate.load_report(spec, None, None)


def harness_of(report: dict) -> dict:
    run = report.get("run") or {}
    return {field: run.get(field) for field in HARNESS_FIELDS}


def comparability(a: dict, b: dict, allow_harness_diff: bool = False) -> list[str]:
    """Devuelve la lista de motivos por los que NO son comparables (vacía = sí).

    Con `allow_harness_diff` se permite comparar dos **configuraciones** del
    mismo modelo (p. ej. contexto 18432 vs 16384): las muestras deben seguir
    siendo idénticas, pero las diferencias de harness se listan como aviso en
    lugar de bloquear.
    """
    problems: list[str] = []

    a_instances = {i.get("instance_id") for i in a.get("instances") or []}
    b_instances = {i.get("instance_id") for i in b.get("instances") or []}
    if a_instances != b_instances:
        only_a = len(a_instances - b_instances)
        only_b = len(b_instances - a_instances)
        problems.append(
            f"las muestras no son las mismas ({only_a} solo en A, {only_b} solo en B); "
            "comparar exige el mismo conjunto de instancias"
        )

    a_harness = harness_of(a)
    b_harness = harness_of(b)
    harness_diffs = [
        f"harness distinto en '{field}': {a_harness[field]!r} vs {b_harness[field]!r}"
        for field in HARNESS_FIELDS
        if a_harness[field] != b_harness[field]
    ]
    if harness_diffs and not allow_harness_diff:
        problems.extend(harness_diffs)

    a_model = (a.get("run") or {}).get("model")
    b_model = (b.get("run") or {}).get("model")
    if a_model == b_model and not allow_harness_diff:
        problems.append(f"ambas corridas son del mismo modelo ({a_model!r})")

    return problems


def cluster_robust_se(a: dict, b: dict) -> float:
    """Error estándar de la diferencia de proporciones, agrupado por repositorio.

    Para una proporción estimada como razón de totales, `p = Σᵢkᵢ / Σᵢnᵢ`, el
    estimador robusto a clusters de su varianza es:

        Var(p) ≈ (1/N²) · Σᵢ (kᵢ − p·nᵢ)²

    y, con muestras independientes, `Var(p_a − p_b)` es la suma de ambas. Se
    agrupa por repositorio porque las instancias de un mismo repo comparten
    código y tipo de fallo (Regla 12).
    """
    est_a = estimate.build_estimate("a", a)
    est_b = estimate.build_estimate("b", b)
    p_a = est_a["end_to_end_rate"]
    p_b = est_b["end_to_end_rate"]
    n_a = est_a["attempted"]
    n_b = est_b["attempted"]
    if n_a == 0 or n_b == 0:
        return 0.0

    def per_repo(report: dict) -> dict[str, tuple[int, int]]:
        out: dict[str, list[int]] = {}
        for instance in report.get("instances") or []:
            repo = (instance.get("instance_id") or "?").split("__")[0]
            entry = out.setdefault(repo, [0, 0])
            entry[1] += 1
            if instance.get("resolved"):
                entry[0] += 1
        return {repo: (k, n) for repo, (k, n) in out.items()}

    a_repo = per_repo(a)
    b_repo = per_repo(b)
    variance = 0.0
    for repo in set(a_repo) | set(b_repo):
        k_a, n_repo_a = a_repo.get(repo, (0, 0))
        k_b, n_repo_b = b_repo.get(repo, (0, 0))
        variance += (k_a - p_a * n_repo_a) ** 2 / (n_a * n_a)
        variance += (k_b - p_b * n_repo_b) ** 2 / (n_b * n_b)
    return math.sqrt(variance) if variance > 0 else 0.0


def build_comparison(
    a_id: str, a: dict, b_id: str, b: dict, allow_harness_diff: bool = False
) -> dict:
    est_a = estimate.build_estimate(a_id, a)
    est_b = estimate.build_estimate(b_id, b)
    problems = comparability(a, b, allow_harness_diff)
    warnings = [
        f"harness distinto en '{field}': "
        f"{(a.get('run') or {}).get(field)!r} vs {(b.get('run') or {}).get(field)!r}"
        for field in HARNESS_FIELDS
        if (a.get("run") or {}).get(field) != (b.get("run") or {}).get(field)
    ]

    result = {
        "a": {
            "run_id": a_id,
            "model": est_a["model"],
            "attempted": est_a["attempted"],
            "resolved": est_a["resolved"],
            "rate": est_a["end_to_end_rate"],
            "ci95": est_a["end_to_end_ci95"],
        },
        "b": {
            "run_id": b_id,
            "model": est_b["model"],
            "attempted": est_b["attempted"],
            "resolved": est_b["resolved"],
            "rate": est_b["end_to_end_rate"],
            "ci95": est_b["end_to_end_ci95"],
        },
        "comparable": not problems,
        "problems": problems,
        "warnings": warnings,
        "kind": (
            "configuración (mismo modelo)"
            if allow_harness_diff and est_a["model"] == est_b["model"]
            else "modelos distintos"
        ),
    }

    if problems:
        result["note"] = (
            "No se calcula diferencia ni valor p: las corridas no son comparables "
            "según la Regla 12."
        )
        return result

    diff = est_a["end_to_end_rate"] - est_b["end_to_end_rate"]
    se = cluster_robust_se(a, b)
    z = diff / se if se > 0 else 0.0
    # Dos colas, aproximación normal.
    p_value = math.erfc(abs(z) / math.sqrt(2)) if se > 0 else 1.0
    result.update(
        {
            "difference": diff,
            "se_cluster": se,
            "z": z,
            "p_value": p_value,
            "significant_at_0_05": p_value < 0.05,
            "note": (
                "Diferencia de tasas extremo a extremo con error estándar agrupado "
                "por repositorio. Con n pequeño y pocos clusters el test tiene poca "
                "potencia: 'no significativo' no significa 'iguales'."
            ),
        }
    )
    return result


def render_text(result: dict) -> str:
    lines = [
        "Comparación SWE (Regla 12)",
        "",
        f"  A: {result['a']['model']}  [{result['a']['run_id']}]",
        f"     {result['a']['resolved']}/{result['a']['attempted']} = "
        f"{estimate.format_pct(result['a']['rate'])} "
        f"[{estimate.format_pct(result['a']['ci95'][0])} – "
        f"{estimate.format_pct(result['a']['ci95'][1])}]",
        f"  B: {result['b']['model']}  [{result['b']['run_id']}]",
        f"     {result['b']['resolved']}/{result['b']['attempted']} = "
        f"{estimate.format_pct(result['b']['rate'])} "
        f"[{estimate.format_pct(result['b']['ci95'][0])} – "
        f"{estimate.format_pct(result['b']['ci95'][1])}]",
        "",
    ]
    for warning in result.get("warnings") or []:
        lines.append(f"⚠ {warning}")
    if result.get("warnings"):
        lines.append("")

    if not result["comparable"]:
        lines.append("⛔ NO COMPARABLES:")
        for problem in result["problems"]:
            lines.append(f"   - {problem}")
        lines.append("")
        lines.append(result["note"])
        return "\n".join(lines)

    lines.append(
        f"Diferencia (A − B): {result['difference'] * 100:+.1f} pp"
    )
    lines.append(f"  error estándar por clusters: {result['se_cluster'] * 100:.1f} pp")
    lines.append(f"  z = {result['z']:.2f}, p = {result['p_value']:.3f}")
    verdict = (
        "significativa al 5 %"
        if result["significant_at_0_05"]
        else "NO significativa al 5 %"
    )
    lines.append(f"  → {verdict}")
    lines.append("")
    lines.append(result["note"])
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--a", required=True, help="run_id o report.json de A")
    parser.add_argument("--b", required=True, help="run_id o report.json de B")
    parser.add_argument("--json", action="store_true")
    parser.add_argument(
        "--allow-harness-diff",
        action="store_true",
        help="Comparar dos configuraciones del mismo modelo (las diferencias de "
        "harness se muestran como aviso, no bloquean)",
    )
    args = parser.parse_args(argv)

    a_id, a = load_any(args.a)
    b_id, b = load_any(args.b)
    result = build_comparison(a_id, a, b_id, b, args.allow_harness_diff)

    if args.json:
        print(json.dumps(result, indent=2, ensure_ascii=False))
    else:
        print(render_text(result))
    return 0 if result["comparable"] else 2


if __name__ == "__main__":
    sys.exit(main())
