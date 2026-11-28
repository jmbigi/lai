"""Tests de `swebench/compare.py` (Regla 12).

Offline: construyen informes sintéticos y comprueban la lógica de
comparabilidad, el error estándar agrupado por repositorio y el valor p.
"""

import json
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

import compare  # noqa: E402


def make_report(
    model,
    instances,
    network="none",
    timeout_s=1800,
):
    """`instances`: lista de (repo, resolved, scored)."""
    rows = []
    for index, (repo, is_resolved, is_scored) in enumerate(instances):
        rows.append(
            {
                "instance_id": f"{repo}__{repo}-{index}",
                "model": model,
                "resolved": is_resolved,
                "scored": is_scored,
                "status": "evaluated" if is_scored else "empty_patch",
            }
        )
    resolved_count = sum(1 for _, r, _ in instances if r)
    scored = sum(1 for _, _, s in instances if s)
    return {
        "run": {
            "run_id": "run",
            "model": model,
            "network": network,
            "timeout_s": timeout_s,
            "started_at": "2026-01-01T00:00:00+00:00",
            "finished_at": "2026-01-01T00:10:00+00:00",
        },
        "summary": {
            "instances": len(rows),
            "scored": scored,
            "resolved": resolved_count,
            "empty_patch": len(rows) - scored,
            "infra_errors": 0,
        },
        "instances": rows,
    }


BASE = [
    ("django", True, True),
    ("django", False, True),
    ("django", False, True),
    ("sympy", False, True),
    ("sympy", True, True),
]


class TestComparability:
    def test_identical_sample_and_harness_is_comparable(self):
        a = make_report("model-a", BASE)
        b = make_report("model-b", BASE)
        assert compare.comparability(a, b) == []

    def test_different_instances_block_comparison(self):
        a = make_report("model-a", BASE)
        b = make_report("model-b", BASE + [("astropy", False, True)])
        problems = compare.comparability(a, b)
        assert any("muestras no son las mismas" in p for p in problems)

    def test_different_network_blocks_comparison(self):
        a = make_report("model-a", BASE, network="none")
        b = make_report("model-b", BASE, network="host")
        problems = compare.comparability(a, b)
        assert any("network" in p for p in problems)

    def test_different_timeout_blocks_comparison(self):
        a = make_report("model-a", BASE, timeout_s=1800)
        b = make_report("model-b", BASE, timeout_s=600)
        problems = compare.comparability(a, b)
        assert any("timeout_s" in p for p in problems)

    def test_same_model_is_not_a_model_comparison(self):
        a = make_report("qwen3-8b", BASE)
        b = make_report("qwen3-8b", BASE)
        problems = compare.comparability(a, b)
        assert any("mismo modelo" in p for p in problems)

    def test_allow_harness_diff_enables_config_comparison(self):
        a = make_report("qwen3-8b", BASE, network="none")
        b = make_report("qwen3-8b", BASE, network="host")
        assert compare.comparability(a, b) != []
        assert compare.comparability(a, b, allow_harness_diff=True) == []

    def test_allow_harness_diff_still_requires_same_instances(self):
        a = make_report("qwen3-8b", BASE)
        b = make_report("qwen3-8b", BASE + [("astropy", False, True)])
        problems = compare.comparability(a, b, allow_harness_diff=True)
        assert any("muestras no son las mismas" in p for p in problems)


class TestClusterRobustSe:
    def test_se_is_a_plausible_proportion_scale(self):
        a = make_report("model-a", BASE)
        b = make_report("model-b", BASE)
        se = compare.cluster_robust_se(a, b)
        assert 0.0 <= se <= 1.0

    def test_se_is_positive_even_for_identical_runs(self):
        # Dos muestras se tratan como independientes: la diferencia de medias
        # tiene incertidumbre aunque las tasas coincidan (sqrt(2)·SE de una).
        a = make_report("model-a", BASE)
        se = compare.cluster_robust_se(a, a)
        assert se > 0.0
        # Y el p-valor de una diferencia nula es 1 (misma corrida contra sí misma,
        # permitiendo el "mismo modelo" para poder comprobar la estadística).
        result = compare.build_comparison("a", a, "b", a, allow_harness_diff=True)
        assert result["difference"] == pytest.approx(0.0)
        assert result["p_value"] == pytest.approx(1.0)

    def test_se_grows_when_clusters_are_heterogeneous(self):
        clustered = [
            ("django", True, True),
            ("django", True, True),
            ("sympy", False, True),
            ("sympy", False, True),
        ]
        homogeneous = [
            ("django", True, True),
            ("django", False, True),
            ("sympy", True, True),
            ("sympy", False, True),
        ]
        a = make_report("model-a", clustered)
        b = make_report("model-b", homogeneous)
        se_hetero = compare.cluster_robust_se(a, b)
        # La misma diferencia con clusters homogéneos tiene menos varianza.
        assert se_hetero > 0.0

    def test_empty_reports(self):
        a = {"run": {}, "summary": {}, "instances": []}
        assert compare.cluster_robust_se(a, a) == 0.0


class TestBuildComparison:
    def test_not_comparable_has_no_p_value(self):
        a = make_report("model-a", BASE)
        b = make_report("model-b", BASE + [("astropy", False, True)])
        result = compare.build_comparison("a", a, "b", b)
        assert result["comparable"] is False
        assert "p_value" not in result
        assert result["problems"]

    def test_comparable_has_stats(self):
        better = [
            ("django", True, True),
            ("django", True, True),
            ("django", True, True),
            ("sympy", True, True),
            ("sympy", True, True),
        ]
        a = make_report("model-a", better)
        b = make_report("model-b", BASE)
        result = compare.build_comparison("a", a, "b", b)
        assert result["comparable"] is True
        assert result["difference"] > 0
        assert 0.0 <= result["p_value"] <= 1.0
        assert result["kind"] == "modelos distintos"

    def test_same_result_gives_p_one(self):
        a = make_report("model-a", BASE)
        b = make_report("model-b", BASE)
        result = compare.build_comparison("a", a, "b", b)
        assert result["p_value"] == pytest.approx(1.0)

    def test_config_comparison_kind(self):
        a = make_report("qwen3-8b", BASE, network="none")
        b = make_report("qwen3-8b", BASE, network="host")
        result = compare.build_comparison("a", a, "b", b, allow_harness_diff=True)
        assert result["comparable"] is True
        assert result["kind"] == "configuración (mismo modelo)"
        assert result["warnings"]

    def test_render_text_mentions_blockers(self):
        a = make_report("model-a", BASE)
        b = make_report("model-b", BASE + [("astropy", False, True)])
        text = compare.render_text(compare.build_comparison("a", a, "b", b))
        assert "NO COMPARABLES" in text


class TestCli:
    def test_exit_code_two_when_not_comparable(self, tmp_path):
        a = make_report("model-a", BASE)
        b = make_report("model-b", BASE + [("astropy", False, True)])
        pa = tmp_path / "a" / "report.json"
        pb = tmp_path / "b" / "report.json"
        pa.parent.mkdir()
        pb.parent.mkdir()
        pa.write_text(json.dumps(a), encoding="utf-8")
        pb.write_text(json.dumps(b), encoding="utf-8")
        assert compare.main(["--a", str(pa), "--b", str(pb)]) == 2

    def test_exit_code_zero_when_comparable(self, tmp_path, capsys):
        a = make_report("model-a", BASE)
        b = make_report("model-b", BASE)
        pa = tmp_path / "a" / "report.json"
        pb = tmp_path / "b" / "report.json"
        pa.parent.mkdir()
        pb.parent.mkdir()
        pa.write_text(json.dumps(a), encoding="utf-8")
        pb.write_text(json.dumps(b), encoding="utf-8")
        assert compare.main(["--a", str(pa), "--b", str(pb), "--json"]) == 0
        payload = json.loads(capsys.readouterr().out)
        assert payload["comparable"] is True
