"""Tests de `swebench/estimate.py` (Regla 12).

Offline: no leen corridas reales ni necesitan Docker. Comprueban la matemática
del intervalo de Wilson, la regla de tres, el tamaño de muestra y el formato.
"""

import json
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

import estimate  # noqa: E402


def _report(instances, resolved, scored, empty, infra=0, model="m"):
    rows = []
    for index, (repo, is_resolved, is_scored) in enumerate(instances):
        rows.append(
            {
                "instance_id": f"{repo}__{repo}-{index}",
                "model": model,
                "scored": is_scored,
                "resolved": is_resolved,
                "status": "evaluated" if is_scored else "empty_patch",
            }
        )
    return {
        "run": {
            "run_id": "run-1",
            "model": model,
            "started_at": "2026-01-01T00:00:00+00:00",
            "finished_at": "2026-01-01T00:10:00+00:00",
            "network": "none",
            "timeout_s": 60,
        },
        "summary": {
            "instances": len(rows),
            "scored": scored,
            "resolved": resolved,
            "empty_patch": empty,
            "infra_errors": infra,
        },
        "instances": rows,
    }


class TestWilson:
    def test_known_interval_for_two_of_ten(self):
        low, high = estimate.wilson_interval(2, 10)
        # Valores calculados con la fórmula de Wilson (z=1.96).
        assert low == pytest.approx(0.057, abs=0.002)
        assert high == pytest.approx(0.510, abs=0.002)

    def test_extremes_are_clamped(self):
        low, high = estimate.wilson_interval(0, 10)
        assert low == 0.0
        assert 0 < high < 1
        low, high = estimate.wilson_interval(10, 10)
        assert high == pytest.approx(1.0)
        assert 0 < low < 1

    def test_zero_total(self):
        assert estimate.wilson_interval(0, 0) == (0.0, 0.0)

    def test_interval_contains_point_estimate(self):
        for successes, total in [(1, 5), (3, 7), (17, 40), (49, 50)]:
            low, high = estimate.wilson_interval(successes, total)
            assert low <= successes / total <= high


class TestRuleOfThree:
    def test_zero_successes_upper_bound(self):
        assert estimate.rule_of_three(10) == pytest.approx(0.3)

    def test_zero_total(self):
        assert estimate.rule_of_three(0) == 1.0


class TestSampleSize:
    def test_margin_ten_points(self):
        # n ≈ 1.96²·0.2·0.8 / 0.1² ≈ 61.5 → 62
        assert estimate.sample_size_for_margin(0.2, 0.10) == 62

    def test_tighter_margin_needs_more_samples(self):
        assert estimate.sample_size_for_margin(0.2, 0.05) > estimate.sample_size_for_margin(
            0.2, 0.10
        )

    def test_rejects_non_positive_margin(self):
        with pytest.raises(ValueError):
            estimate.sample_size_for_margin(0.2, 0)


class TestBuildEstimate:
    def test_end_to_end_and_scored_rates(self):
        report = _report(
            [
                ("a", True, True),
                ("a", False, True),
                ("b", False, True),
                ("b", False, False),
            ],
            resolved=1,
            scored=3,
            empty=1,
        )
        est = estimate.build_estimate("run-1", report)
        assert est["attempted"] == 4
        assert est["scored"] == 3
        assert est["resolved"] == 1
        assert est["end_to_end_rate"] == pytest.approx(0.25)
        assert est["scored_rate"] == pytest.approx(1 / 3)
        assert est["preliminary"] is True

    def test_per_repo_breakdown(self):
        report = _report(
            [
                ("pytest-dev", True, True),
                ("pytest-dev", False, True),
                ("astropy", False, True),
            ],
            resolved=1,
            scored=3,
            empty=0,
        )
        est = estimate.build_estimate("run-1", report)
        assert est["per_repo"]["pytest-dev"] == {"attempted": 2, "scored": 2, "resolved": 1}
        assert est["per_repo"]["astropy"] == {"attempted": 1, "scored": 1, "resolved": 0}

    def test_zero_resolved_uses_upper_bound(self):
        report = _report(
            [("a", False, True), ("a", False, True)], resolved=0, scored=2, empty=0
        )
        est = estimate.build_estimate("run-1", report)
        assert est["end_to_end_rate"] == 0.0
        assert est["upper_bound_95"] == pytest.approx(1.5)  # 3/2, se recorta al formatear

    def test_not_preliminary_at_thirty(self):
        instances = [("a", i < 6, True) for i in range(30)]
        report = _report(instances, resolved=6, scored=30, empty=0)
        est = estimate.build_estimate("run-1", report)
        assert est["preliminary"] is False

    def test_infra_errors_are_reported(self):
        report = _report([("a", False, True)], resolved=0, scored=1, empty=0, infra=2)
        est = estimate.build_estimate("run-1", report)
        assert est["infra_errors"] == 2


class TestRendering:
    def test_text_mentions_preliminary(self):
        report = _report([("a", True, True)], resolved=1, scored=1, empty=0)
        text = estimate.render_text(estimate.build_estimate("run-1", report))
        assert "PRELIMINAR" in text
        assert "Wilson" in text
        assert "100.0 %" in text

    def test_markdown_table(self):
        report = _report([("a", True, True)], resolved=1, scored=1, empty=0)
        md = estimate.render_markdown(estimate.build_estimate("run-1", report))
        assert "| Tasa extremo a extremo |" in md
        assert "`run-1`" in md

    def test_extrapolation_is_labelled(self):
        report = _report([("a", True, True)], resolved=1, scored=1, empty=0)
        ext = estimate.extrapolate(estimate.build_estimate("run-1", report))
        assert ext["expected_resolved"] == 500
        assert "Extrapolación" in ext["warning"]


class TestCli:
    def test_json_output(self, tmp_path, capsys):
        report = _report([("a", True, True), ("a", False, True)], resolved=1, scored=2, empty=0)
        path = tmp_path / "report.json"
        path.write_text(json.dumps(report), encoding="utf-8")
        rc = estimate.main(["--report", str(path), "--json"])
        assert rc == 0
        payload = json.loads(capsys.readouterr().out)
        assert payload["resolved"] == 1
        assert payload["attempted"] == 2

    def test_missing_report_fails_loudly(self, tmp_path):
        with pytest.raises(SystemExit):
            estimate.main(["--report", str(tmp_path / "nope.json")])
