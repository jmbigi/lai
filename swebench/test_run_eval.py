#!/usr/bin/env python3
"""Tests offline de run_eval.py (no requieren Docker ni Ollama).

La clasificación de resultados y la extracción del diff son funciones puras, así
que se pueden probar sin contenedores. El *preflight* se prueba con `http_json`
y `run` mockeados.

Uso:
    python3 -m unittest swebench/test_run_eval.py -v
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import unittest
from pathlib import Path
from unittest import mock

MODULE_PATH = Path(__file__).with_name("run_eval.py")
spec = importlib.util.spec_from_file_location("lai_swebench_run_eval", MODULE_PATH)
assert spec and spec.loader
run_eval = importlib.util.module_from_spec(spec)
spec.loader.exec_module(run_eval)

B = run_eval.DIFF_BEGIN
E = run_eval.DIFF_END


def completed(returncode: int = 0, stdout: str = "", stderr: str = "") -> subprocess.CompletedProcess:
    return subprocess.CompletedProcess(args=["docker"], returncode=returncode, stdout=stdout, stderr=stderr)


class ImageNameTests(unittest.TestCase):
    def test_prefers_dataset_image(self):
        row = {"instance_id": "sympy__sympy-20590", "image": "swebench/sweb.eval.x86_64.sympy_1776_sympy-20590:latest"}
        self.assertEqual(run_eval.image_name(row), row["image"])

    def test_fallback_convention(self):
        row = {"instance_id": "django__django-11099"}
        self.assertEqual(
            run_eval.image_name(row),
            "swebench/sweb.eval.x86_64.django_1776_django-11099",
        )


class SlugTests(unittest.TestCase):
    def test_slug_is_filesystem_safe(self):
        self.assertEqual(run_eval.slug("qwen3-coder-opt:latest"), "qwen3-coder-opt_latest")

    def test_slug_strips_leading_dots(self):
        self.assertEqual(run_eval.slug("../etc/passwd"), "etc_passwd")

    def test_slug_never_empty(self):
        self.assertEqual(run_eval.slug("///"), "model")


class BuildTaskTests(unittest.TestCase):
    def test_task_frames_issue_and_forbids_test_edits(self):
        task = run_eval.build_task({"problem_statement": "The parser drops quoted fields."})
        self.assertIn("/testbed", task)
        self.assertIn("The parser drops quoted fields.", task)
        self.assertIn("Do not modify test files", task)
        self.assertIn("start_line", task)

    def test_task_is_stable(self):
        inst = {"problem_statement": "x"}
        self.assertEqual(run_eval.build_task(inst), run_eval.build_task(inst))


class OllamaRootTests(unittest.TestCase):
    def test_strips_v1_suffix(self):
        self.assertEqual(run_eval.ollama_root("http://127.0.0.1:11434/v1"), "http://127.0.0.1:11434")

    def test_keeps_custom_path(self):
        self.assertEqual(run_eval.ollama_root("http://localhost:8080/ollama/v1"), "http://localhost:8080/ollama")

    def test_without_v1(self):
        self.assertEqual(run_eval.ollama_root("http://localhost:11434"), "http://localhost:11434")


class ContainerBaseUrlTests(unittest.TestCase):
    def test_loopback_becomes_host_docker_internal(self):
        self.assertEqual(
            run_eval.container_base_url("http://127.0.0.1:11434/v1"),
            "http://host.docker.internal:11434/v1",
        )

    def test_localhost_and_ipv6(self):
        self.assertEqual(
            run_eval.container_base_url("http://localhost:11434/v1"),
            "http://host.docker.internal:11434/v1",
        )
        self.assertEqual(
            run_eval.container_base_url("http://[::1]:11434/v1"),
            "http://host.docker.internal:11434/v1",
        )

    def test_non_loopback_is_untouched(self):
        for url in ("http://host.docker.internal:11434/v1", "http://192.168.1.50:11434/v1"):
            self.assertEqual(run_eval.container_base_url(url), url)

    def test_warning_only_for_loopback(self):
        self.assertIsNotNone(run_eval.container_base_url_warning("http://127.0.0.1:11434/v1"))
        self.assertIsNone(run_eval.container_base_url_warning("http://host.docker.internal:11434/v1"))


class ExtractPatchTests(unittest.TestCase):
    def test_extracts_between_markers(self):
        stdout = f"noise\n{B}\ndiff --git a/x b/x\n+hello\n{E}\nmore"
        self.assertEqual(run_eval.extract_patch(stdout), "diff --git a/x b/x\n+hello")

    def test_missing_markers_returns_empty(self):
        self.assertEqual(run_eval.extract_patch("just output"), "")

    def test_empty_patch_returns_empty(self):
        self.assertEqual(run_eval.extract_patch(f"{B}\n\n{E}"), "")

    def test_only_first_block_is_used(self):
        stdout = f"{B}\nfirst\n{E}\n{B}\nsecond\n{E}"
        self.assertEqual(run_eval.extract_patch(stdout), "first")


class ClassifyResultTests(unittest.TestCase):
    def test_timeout_wins(self):
        status, reason = run_eval.classify_result("", "TIMEOUT after 1800s", 124)
        self.assertEqual(status, "timeout")
        self.assertEqual(reason, "")

    def test_docker_run_failure_without_markers_is_infra(self):
        status, reason = run_eval.classify_result("", "docker: Error response", 125)
        self.assertEqual(status, "infra_error")
        self.assertIn("rc=125", reason)

    def test_glibc_mismatch_is_infra(self):
        stderr = "/usr/local/bin/lai: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found"
        status, reason = run_eval.classify_result("", stderr, 127)
        self.assertEqual(status, "infra_error")
        self.assertIn("glibc", reason)

    def test_shared_library_error_is_infra(self):
        stderr = "lai: error while loading shared libraries: libssl.so.3: cannot open shared object file"
        status, reason = run_eval.classify_result("", stderr, 127)
        self.assertEqual(status, "infra_error")

    def test_command_not_found_is_infra(self):
        stdout = f"bash: line 1: lai: command not found\n{B}\n{E}\n"
        status, reason = run_eval.classify_result(stdout, "", 0)
        self.assertEqual(status, "infra_error")
        self.assertIn("no se pudo ejecutar", reason)

    def test_missing_markers_is_infra(self):
        status, reason = run_eval.classify_result("agent ran fine but no markers", "", 0)
        self.assertEqual(status, "infra_error")
        self.assertIn("marcadores", reason)

    def test_empty_patch_is_no_patch(self):
        status, _ = run_eval.classify_result(f"{B}\n{E}\n", "", 0)
        self.assertEqual(status, "no_patch")

    def test_nonzero_agent_rc_with_patch_is_still_patched(self):
        stdout = f"{B}\ndiff --git a/x b/x\n+x\n{E}\n{run_eval.AGENT_RC}=3\n"
        status, _ = run_eval.classify_result(stdout, "", 0)
        self.assertEqual(status, "patched")

    def test_binary_copy_failure_is_infra(self):
        status, reason = run_eval.classify_result("", "", 91)
        self.assertEqual(status, "infra_error")
        self.assertIn("copiar el binario", reason)

    def test_missing_testbed_is_infra(self):
        status, reason = run_eval.classify_result("", "", 90)
        self.assertEqual(status, "infra_error")
        self.assertIn("/testbed", reason)

    def test_rule4_rejection_is_infra_not_no_patch(self):
        stdout = (
            "Error: Regla 4: el endpoint 'http://host.docker.internal:11434/v1' "
            "no es local. lai solo usa modelos locales por defecto.\n"
            f"{B}\n{E}\n{run_eval.AGENT_RC}=0\n"
        )
        status, reason = run_eval.classify_result(stdout, "", 0)
        self.assertEqual(status, "infra_error")
        self.assertIn("Regla 4", reason)


class PreflightTests(unittest.TestCase):
    def test_ollama_root_url_is_checked(self):
        payload = {"data": [{"id": "qwen3:4b"}]}
        with mock.patch.object(run_eval, "http_json", return_value=payload) as http:
            self.assertTrue(run_eval.preflight_ollama("http://127.0.0.1:11434/v1", "qwen3:4b"))
        self.assertEqual(http.call_args_list[0].args[0], "http://127.0.0.1:11434/v1/models")

    def test_missing_model_fails(self):
        payload = {"data": [{"id": "other:latest"}]}
        with mock.patch.object(run_eval, "http_json", return_value=payload):
            self.assertFalse(run_eval.preflight_ollama("http://127.0.0.1:11434/v1", "qwen3:4b"))

    def test_unreachable_ollama_fails(self):
        with mock.patch.object(run_eval, "http_json", return_value=None):
            self.assertFalse(run_eval.preflight_ollama("http://127.0.0.1:11434/v1", "qwen3:4b"))

    def test_low_context_only_warns(self):
        models = {"data": [{"id": "m"}]}
        ps = {"models": [{"name": "m", "context_length": 4096}]}

        def fake(url, payload=None, timeout=5):
            return models if url.endswith("/models") else ps

        with mock.patch.object(run_eval, "http_json", side_effect=fake):
            self.assertTrue(run_eval.preflight_ollama("http://127.0.0.1:11434/v1", "m"))

    def test_preflight_reports_missing_images(self):
        with mock.patch.object(run_eval, "docker_available", return_value=True), mock.patch.object(
            run_eval, "preflight_ollama", return_value=True
        ), mock.patch.object(run_eval, "image_present", return_value=False):
            self.assertFalse(
                run_eval.preflight([{"instance_id": "a__b-1", "image": "img"}], "http://127.0.0.1:11434/v1", "m")
            )


class PendingSelectionTests(unittest.TestCase):
    """El bucle de main() debe saltar lo ya hecho y respetar --limit."""

    def test_pending_selection_logic(self):
        instances = [{"instance_id": f"i{n}"} for n in range(5)]
        done = {"i0": {}, "i1": {}}
        limit = 2
        pending = []
        for instance in instances:
            if instance["instance_id"] in done:
                continue
            if limit and len(pending) >= limit:
                break
            pending.append(instance)
        self.assertEqual([p["instance_id"] for p in pending], ["i2", "i3"])


if __name__ == "__main__":
    unittest.main()
