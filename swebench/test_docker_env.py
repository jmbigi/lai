#!/usr/bin/env python3
"""Tests offline de swebench/docker_env.py (sin Docker, sin red)."""

from __future__ import annotations

import importlib.util
import io
import subprocess
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock

MODULE_PATH = Path(__file__).resolve().parent / "docker_env.py"
spec = importlib.util.spec_from_file_location("lai_docker_env", MODULE_PATH)
docker_env = importlib.util.module_from_spec(spec)
spec.loader.exec_module(docker_env)


def completed(returncode=0, stdout="", stderr=""):
    return subprocess.CompletedProcess(args=["docker"], returncode=returncode, stdout=stdout, stderr=stderr)


class ServerVersionTests(unittest.TestCase):
    def test_returns_version(self):
        with mock.patch.object(docker_env.subprocess, "run", return_value=completed(stdout="29.9.3\n")):
            version, problem = docker_env.server_version()
        self.assertEqual(version, "29.9.3")
        self.assertEqual(problem, "")

    def test_missing_binary(self):
        with mock.patch.object(docker_env.subprocess, "run", side_effect=FileNotFoundError):
            version, problem = docker_env.server_version()
        self.assertIsNone(version)
        self.assertIn("no existe en PATH", problem)

    def test_timeout(self):
        with mock.patch.object(
            docker_env.subprocess, "run", side_effect=subprocess.TimeoutExpired(cmd="docker", timeout=30)
        ):
            version, problem = docker_env.server_version()
        self.assertIsNone(version)
        self.assertIn("no respondió", problem)

    def test_nonzero_exit_reports_stderr(self):
        with mock.patch.object(
            docker_env.subprocess,
            "run",
            return_value=completed(returncode=1, stderr="error during connect: pipe not found"),
        ):
            version, problem = docker_env.server_version()
        self.assertIsNone(version)
        self.assertIn("pipe not found", problem)

    def test_empty_stdout_is_a_failure(self):
        with mock.patch.object(docker_env.subprocess, "run", return_value=completed(stdout="   \n")):
            version, problem = docker_env.server_version()
        self.assertIsNone(version)
        self.assertIn("ninguna versión", problem)


class MajorOfTests(unittest.TestCase):
    def test_parses(self):
        self.assertEqual(docker_env.major_of("29.9.3"), 29)
        self.assertEqual(docker_env.major_of("24.0.5"), 24)

    def test_non_numeric(self):
        self.assertIsNone(docker_env.major_of("dev"))
        self.assertIsNone(docker_env.major_of(""))


class CheckTests(unittest.TestCase):
    def test_ok_is_silent(self):
        buffer = io.StringIO()
        with mock.patch.object(docker_env.subprocess, "run", return_value=completed(stdout="29.9.3")):
            with redirect_stdout(buffer):
                ok = docker_env.check()
        self.assertTrue(ok)
        self.assertEqual(buffer.getvalue(), "")

    def test_old_server_warns_but_passes(self):
        buffer = io.StringIO()
        with mock.patch.object(docker_env.subprocess, "run", return_value=completed(stdout="19.03.8")):
            with redirect_stdout(buffer):
                ok = docker_env.check()
        self.assertTrue(ok)
        self.assertIn("antiguo", buffer.getvalue())

    def test_failure_prints_hints(self):
        buffer = io.StringIO()
        with mock.patch.object(docker_env.subprocess, "run", side_effect=FileNotFoundError):
            with redirect_stdout(buffer):
                ok = docker_env.check()
        self.assertFalse(ok)
        output = buffer.getvalue()
        self.assertIn("Docker no está disponible", output)
        self.assertIn("winget upgrade", output)

    def test_quiet_suppresses_output(self):
        buffer = io.StringIO()
        with mock.patch.object(docker_env.subprocess, "run", side_effect=FileNotFoundError):
            with redirect_stdout(buffer):
                ok = docker_env.check(quiet=True)
        self.assertFalse(ok)
        self.assertEqual(buffer.getvalue(), "")


class DescribeTests(unittest.TestCase):
    def test_available(self):
        with mock.patch.object(docker_env.subprocess, "run", return_value=completed(stdout="29.9.3")):
            self.assertEqual(docker_env.describe(), "docker 29.9.3")

    def test_unavailable(self):
        with mock.patch.object(docker_env.subprocess, "run", side_effect=FileNotFoundError):
            self.assertIn("no disponible", docker_env.describe())


if __name__ == "__main__":
    unittest.main()
