#!/usr/bin/env python3
"""Tests offline de evaluate.py (no requieren Docker ni el dataset).

Uso:
    python3 -m unittest swebench/test_evaluate.py -v
"""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("evaluate.py")
spec = importlib.util.spec_from_file_location("lai_swebench_evaluate", MODULE_PATH)
assert spec and spec.loader
evaluate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evaluate)


class ParserTests(unittest.TestCase):
    def test_pytest_parser_keeps_official_statuses_and_ignores_xpass(self):
        log = "\n".join(
            [
                "PASSED tests/test_a.py::test_a",
                "FAILED tests/test_b.py::test_b - AssertionError",
                "ERROR tests/test_c.py::test_c",
                "SKIPPED tests/test_d.py::test_d",
                "XFAIL tests/test_e.py::test_e",
                "XPASS tests/test_f.py::test_f",
            ]
        )
        status = evaluate.parse_log_pytest(log)
        self.assertEqual(status["tests/test_a.py::test_a"], "PASSED")
        self.assertEqual(status["tests/test_b.py::test_b"], "FAILED")
        self.assertEqual(status["tests/test_c.py::test_c"], "ERROR")
        self.assertEqual(status["tests/test_d.py::test_d"], "SKIPPED")
        self.assertEqual(status["tests/test_e.py::test_e"], "XFAIL")
        self.assertNotIn("tests/test_f.py::test_f", status)

    def test_pytest_v2_parser_handles_status_at_end_of_line(self):
        log = "tests/test_a.py::test_a PASSED\ntests/test_b.py::test_b FAILED\n"
        status = evaluate.parse_log_pytest_v2(log)
        self.assertEqual(status["tests/test_a.py::test_a"], "PASSED")
        self.assertEqual(status["tests/test_b.py::test_b"], "FAILED")

    def test_test_exit_code_parser(self):
        self.assertEqual(evaluate.parse_test_exit_code(">>>>> Test Exit Code: 2"), 2)
        self.assertIsNone(evaluate.parse_test_exit_code("no marker"))


class GradingTests(unittest.TestCase):
    def instance(self, **overrides):
        data = {
            "instance_id": "demo__demo-1",
            "FAIL_TO_PASS": ["tests/test_a.py::test_a"],
            "PASS_TO_PASS": ["tests/test_b.py::test_b"],
            "eval_type": "pass_and_fail",
        }
        data.update(overrides)
        return data

    def test_resolved_when_f2p_and_p2p_pass(self):
        result = evaluate.grade(
            {
                "tests/test_a.py::test_a": "PASSED",
                "tests/test_b.py::test_b": "PASSED",
            },
            self.instance(),
        )
        self.assertTrue(result["resolved"])
        self.assertEqual(result["resolution"], "RESOLVED_FULL")

    def test_not_resolved_when_f2p_fails(self):
        result = evaluate.grade(
            {
                "tests/test_a.py::test_a": "FAILED",
                "tests/test_b.py::test_b": "PASSED",
            },
            self.instance(),
        )
        self.assertFalse(result["resolved"])
        self.assertEqual(result["FAIL_TO_PASS"]["failure"], ["tests/test_a.py::test_a"])

    def test_p2p_skipped_is_maintained_but_not_for_f2p(self):
        result = evaluate.grade(
            {
                "tests/test_a.py::test_a": "SKIPPED",
                "tests/test_b.py::test_b": "SKIPPED",
            },
            self.instance(),
        )
        self.assertFalse(result["resolved"])
        self.assertEqual(result["FAIL_TO_PASS"]["failure"], ["tests/test_a.py::test_a"])
        self.assertEqual(result["PASS_TO_PASS"]["success"], ["tests/test_b.py::test_b"])

    def test_xfail_counts_as_pass(self):
        result = evaluate.grade(
            {
                "tests/test_a.py::test_a": "XFAIL",
                "tests/test_b.py::test_b": "XFAIL",
            },
            self.instance(),
        )
        self.assertTrue(result["resolved"])

    def test_truncated_parametrized_id(self):
        result = evaluate.grade(
            {
                "tests/test_a.py::test_a[param-value]": "PASSED",
                "tests/test_b.py::test_b": "PASSED",
            },
            self.instance(FAIL_TO_PASS=["tests/test_a.py::test_a[param"]),
        )
        self.assertTrue(result["resolved"])

    def test_fail_only_eval_type(self):
        result = evaluate.grade(
            {"tests/test_a.py::test_a": "FAILED"},
            self.instance(eval_type="fail_only", PASS_TO_PASS=[]),
        )
        self.assertFalse(result["resolved"])
        self.assertEqual(result["FAIL_TO_PASS"]["failure"], ["tests/test_a.py::test_a"])


class LogEvaluationTests(unittest.TestCase):
    def instance(self):
        return {
            "instance_id": "demo__demo-1",
            "log_parser": "parse_log_pytest",
            "FAIL_TO_PASS": ["tests/test_a.py::test_a"],
            "PASS_TO_PASS": ["tests/test_b.py::test_b"],
            "eval_type": "pass_and_fail",
        }

    def test_evaluate_log_slices_between_markers_and_grades(self):
        log = "\n".join(
            [
                "some setup output",
                ">>>>> Start Test Output",
                "PASSED tests/test_a.py::test_a",
                "PASSED tests/test_b.py::test_b",
                ">>>>> End Test Output",
                ">>>>> Test Exit Code: 0",
            ]
        )
        status_map, error = evaluate.evaluate_log(self.instance(), log)
        self.assertEqual(error, {})
        self.assertIsNotNone(status_map)
        result = evaluate.grade(status_map or {}, self.instance())
        self.assertTrue(result["resolved"])

    def test_evaluate_log_rejects_missing_markers(self):
        status_map, error = evaluate.evaluate_log(self.instance(), "PASSED tests/test_a.py::test_a")
        self.assertIsNone(status_map)
        self.assertEqual(error["status"], "no_test_output")

    def test_evaluate_log_rejects_patch_apply_failure(self):
        log = evaluate.MODEL_PATCH_FAIL
        status_map, error = evaluate.evaluate_log(self.instance(), log)
        self.assertIsNone(status_map)
        self.assertEqual(error["status"], "apply_failed")

    def test_inject_test_exit_capture_before_end_marker(self):
        script = "\n".join(
            [
                ": '>>>>> Start Test Output'",
                "pytest -q",
                ": '>>>>> End Test Output'",
            ]
        )
        injected, changed = evaluate.inject_test_exit_capture(script)
        self.assertTrue(changed)
        self.assertIn("SWEBENCH_TEST_EXIT_CODE=$?", injected)
        self.assertLess(injected.index("SWEBENCH_TEST_EXIT_CODE"), injected.index(evaluate.END_TEST_OUTPUT))

    def test_build_eval_script_applies_model_patch_first(self):
        script, _ = evaluate.build_eval_script(
            {"eval_script": "echo official\n", "instance_id": "demo__demo-1"}
        )
        self.assertIn("git apply", script)
        self.assertLess(script.index("git apply"), script.index("echo official"))

    def test_slug_does_not_escape_evaluation_directory(self):
        self.assertEqual(evaluate.slug(".."), "run")
        self.assertEqual(evaluate.slug("../evil"), "evil")
        self.assertNotIn("/", evaluate.slug("a/b"))


class ValidationTests(unittest.TestCase):
    def test_empty_pass_to_pass_is_valid(self):
        """El dataset oficial trae PASS_TO_PASS vacío en algunas instancias."""
        instances = {
            "demo__demo-1": {
                "image": "img:1",
                "eval_script": "echo hi",
                "log_parser": "parse_log_pytest",
                "FAIL_TO_PASS": ["tests/test_a.py::test_a"],
                "PASS_TO_PASS": [],
            }
        }
        self.assertEqual(evaluate.validate_instances(instances, ["demo__demo-1"]), [])

    def test_missing_eval_script_is_reported(self):
        instances = {
            "demo__demo-1": {
                "image": "img:1",
                "log_parser": "parse_log_pytest",
                "FAIL_TO_PASS": ["a"],
                "PASS_TO_PASS": ["b"],
            }
        }
        errors = evaluate.validate_instances(instances, ["demo__demo-1"])
        self.assertEqual(len(errors), 1)
        self.assertIn("eval_script", errors[0])

    def test_missing_instance_is_reported(self):
        errors = evaluate.validate_instances({}, ["ghost__ghost-1"])
        self.assertIn("ghost__ghost-1", errors[0])


if __name__ == "__main__":
    unittest.main()
