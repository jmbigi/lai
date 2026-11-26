#!/usr/bin/env python3
"""Test del harness completo con un dataset simulado (sin Docker ni HuggingFace).

Valida la ida y vuelta de datos entre las etapas: normalizaciÃ³n de instancias,
escritura atÃ³mica, lectura por `evaluate.py` y validaciÃ³n de los campos que la
evaluaciÃ³n necesita.

Uso:
    python3 -m unittest swebench/test_harness_offline.py -v
"""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).parent


def load(name: str, filename: str):
    spec = importlib.util.spec_from_file_location(name, HERE / filename)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


fetch = load("lai_swebench_fetch", "fetch_dataset.py")
evaluate = load("lai_swebench_evaluate_offline", "evaluate.py")

RAW_INSTANCE = {
    "instance_id": "sympy__sympy-20590",
    "repo": "sympy/sympy",
    "base_commit": "abc123",
    "environment_setup_commit": "def456",
    "version": "1.7",
    "difficulty": "easy",
    "problem_statement": "Symbol assumptions are dropped.",
    "hints_text": "",
    "created_at": "2021-01-01",
    "patch": "diff --git a/x b/x\n",
    "test_patch": "diff --git a/t b/t\n",
    "FAIL_TO_PASS": json.dumps(["test_a", "test_b"]),
    "PASS_TO_PASS": json.dumps(["test_c"]),
    "image": "swebench/sweb.eval.x86_64.sympy_1776_sympy-20590:latest",
    "eval_script": "#!/bin/bash\npytest -rA\n",
    "log_parser": "parse_log_sympy",
    "eval_type": "pass_and_fail",
}


class FetchNormaliseTests(unittest.TestCase):
    def test_keeps_all_eval_fields(self):
        row = fetch.normalise(RAW_INSTANCE)
        for field in fetch.REQUIRED_EVAL_FIELDS:
            self.assertIn(field, row)
            self.assertTrue(row[field], f"{field} no deberÃ­a quedar vacÃ­o")

    def test_parses_json_string_lists(self):
        row = fetch.normalise(RAW_INSTANCE)
        self.assertEqual(row["FAIL_TO_PASS"], ["test_a", "test_b"])
        self.assertEqual(row["PASS_TO_PASS"], ["test_c"])

    def test_already_parsed_lists_are_kept(self):
        raw = dict(RAW_INSTANCE, FAIL_TO_PASS=["test_a"])
        self.assertEqual(fetch.normalise(raw)["FAIL_TO_PASS"], ["test_a"])

    def test_broken_json_list_becomes_empty(self):
        raw = dict(RAW_INSTANCE, FAIL_TO_PASS="{not json")
        self.assertEqual(fetch.normalise(raw)["FAIL_TO_PASS"], [])

    def test_missing_fields_are_none(self):
        row = fetch.normalise({"instance_id": "x"})
        self.assertIsNone(row["image"])
        self.assertIsNone(row["eval_script"])

    def test_knows_whether_existing_data_is_usable(self):
        usable = {RAW_INSTANCE["instance_id"]: fetch.normalise(RAW_INSTANCE)}
        old = {"a": {"instance_id": "a", "FAIL_TO_PASS": [], "PASS_TO_PASS": []}}
        self.assertTrue(fetch.existing_is_usable(usable))
        self.assertFalse(fetch.existing_is_usable(old))
        self.assertFalse(fetch.existing_is_usable({}))


class AtomicWriteTests(unittest.TestCase):
    def test_jsonl_roundtrip_leaves_no_tmp_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "instances.jsonl"
            rows = [fetch.normalise(RAW_INSTANCE)]
            fetch.write_jsonl_atomic(path, rows)
            self.assertFalse(path.with_suffix(".jsonl.tmp").exists())
            loaded = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
            self.assertEqual(loaded[0]["instance_id"], "sympy__sympy-20590")
            self.assertEqual(loaded[0]["FAIL_TO_PASS"], ["test_a", "test_b"])

    def test_json_manifest_roundtrip(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest.json"
            fetch.write_json_atomic(path, {"instances": 1, "repos": {"sympy/sympy": 1}})
            self.assertFalse(path.with_suffix(".json.tmp").exists())
            self.assertEqual(json.loads(path.read_text(encoding="utf-8"))["instances"], 1)


class EvaluateLoadingTests(unittest.TestCase):
    def test_load_instances_parses_lists_and_skips_rows_without_id(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "instances.jsonl"
            path.write_text(
                "\n".join(
                    [
                        json.dumps(fetch.normalise(RAW_INSTANCE)),
                        json.dumps({"repo": "sin/id"}),
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            with mock.patch.object(evaluate, "INSTANCES", path):
                rows = evaluate.load_instances()
        self.assertEqual(list(rows), ["sympy__sympy-20590"])
        self.assertEqual(rows["sympy__sympy-20590"]["FAIL_TO_PASS"], ["test_a", "test_b"])

    def test_load_predictions_defaults_missing_patch(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "pred.jsonl"
            path.write_text(
                json.dumps({"instance_id": "a"}) + "\n" + json.dumps({"instance_id": "b", "model_patch": "x"}) + "\n",
                encoding="utf-8",
            )
            preds = evaluate.load_predictions(path)
        self.assertEqual(preds["a"]["model_patch"], "")
        self.assertEqual(preds["b"]["model_patch"], "x")

    def test_load_predictions_skips_rows_without_instance_id(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "pred.jsonl"
            path.write_text(json.dumps({"model_patch": "x"}) + "\n", encoding="utf-8")
            self.assertEqual(evaluate.load_predictions(path), {})

    def test_choose_predictions_requires_explicit_choice_when_ambiguous(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            (directory / "a.jsonl").write_text("", encoding="utf-8")
            (directory / "b.jsonl").write_text("", encoding="utf-8")
            with mock.patch.object(evaluate, "PREDICTIONS", directory):
                with self.assertRaises(SystemExit):
                    evaluate.choose_predictions(None)
                self.assertEqual(evaluate.choose_predictions(str(directory / "a.jsonl")), directory / "a.jsonl")

    def test_choose_predictions_autodetects_single_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            only = directory / "only.jsonl"
            only.write_text("", encoding="utf-8")
            with mock.patch.object(evaluate, "PREDICTIONS", directory):
                self.assertEqual(evaluate.choose_predictions(None), only)


class EvalScriptWrapperTests(unittest.TestCase):
    def test_wrapper_applies_model_patch_before_official_script(self):
        script, injected = evaluate.build_eval_script(fetch.normalise(RAW_INSTANCE))
        apply_at = script.index("git apply")
        official_at = script.index("pytest -rA")
        self.assertLess(apply_at, official_at)
        self.assertIn("LAI_MODEL_PATCH_APPLY_FAILED", script)
        self.assertIn("exit 86", script)
        self.assertIsInstance(injected, bool)

    def test_wrapper_requires_patch_and_script_fields(self):
        usable = fetch.normalise(RAW_INSTANCE)
        self.assertEqual(evaluate.validate_instances({RAW_INSTANCE["instance_id"]: usable}, [RAW_INSTANCE["instance_id"]]), [])

        broken = fetch.normalise(RAW_INSTANCE)
        broken["eval_script"] = ""
        errors = evaluate.validate_instances({RAW_INSTANCE["instance_id"]: broken}, [RAW_INSTANCE["instance_id"]])
        self.assertEqual(len(errors), 1)
        self.assertIn("eval_script", errors[0])


class EndToEndOfflineTests(unittest.TestCase):
    """Simula una predicciÃ³n y comprueba que la graduaciÃ³n se calcula sin Docker."""

    def test_grade_resolves_when_f2p_pass_and_p2p_maintained(self):
        instance = fetch.normalise(RAW_INSTANCE)
        status_map = {"test_a": "PASSED", "test_b": "PASSED", "test_c": "PASSED"}
        result = evaluate.grade(status_map, instance)
        self.assertEqual(result["resolution"], "RESOLVED_FULL")

    def test_grade_fails_when_a_p2p_regresses(self):
        instance = fetch.normalise(RAW_INSTANCE)
        status_map = {"test_a": "PASSED", "test_b": "PASSED", "test_c": "FAILED"}
        result = evaluate.grade(status_map, instance)
        self.assertEqual(result["resolution"], "RESOLVED_NO")

    def test_grade_is_partial_when_some_f2p_missing_but_p2p_maintained(self):
        instance = fetch.normalise(RAW_INSTANCE)
        status_map = {"test_a": "PASSED", "test_c": "PASSED"}
        result = evaluate.grade(status_map, instance)
        self.assertEqual(result["resolution"], "RESOLVED_PARTIAL")
        self.assertFalse(result["resolved"])

    def test_grade_not_resolved_when_a_f2p_fails(self):
        instance = fetch.normalise(RAW_INSTANCE)
        status_map = {"test_a": "PASSED", "test_b": "FAILED", "test_c": "PASSED"}
        result = evaluate.grade(status_map, instance)
        self.assertFalse(result["resolved"])
        self.assertAlmostEqual(result["FAIL_TO_PASS"]["ratio"], 0.5)


if __name__ == "__main__":
    unittest.main()
