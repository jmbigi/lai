#!/usr/bin/env python3
"""Tests offline de prepare.py (no requieren Docker ni el dataset).

Se mockea `subprocess.run` para simular respuestas de Docker, de modo que los
tests se ejecutan en cualquier equipo (Windows incluido) sin daemon.

Uso:
    python3 -m unittest swebench/test_prepare.py -v
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

MODULE_PATH = Path(__file__).with_name("prepare.py")
spec = importlib.util.spec_from_file_location("lai_swebench_prepare", MODULE_PATH)
assert spec and spec.loader
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


def completed(returncode: int = 0, stdout: str = "", stderr: str = "") -> subprocess.CompletedProcess:
    return subprocess.CompletedProcess(args=["docker"], returncode=returncode, stdout=stdout, stderr=stderr)


class ImageNameTests(unittest.TestCase):
    def test_prefers_dataset_image(self):
        row = {"instance_id": "astropy__astropy-12907", "image": "swebench/sweb.eval.x86_64.astropy_1776_astropy-12907:latest"}
        self.assertEqual(prepare.image_name(row), row["image"])

    def test_fallback_uses_convention_for_old_data(self):
        row = {"instance_id": "django__django-11099"}
        self.assertEqual(
            prepare.image_name(row),
            "swebench/sweb.eval.x86_64.django_1776_django-11099",
        )

    def test_fallback_is_lowercase(self):
        row = {"instance_id": "SymPy__SymPy-1234"}
        self.assertTrue(prepare.image_name(row).islower())


class ImageStateTests(unittest.TestCase):
    def test_present(self):
        with mock.patch.object(prepare, "run", return_value=completed(0)):
            self.assertIs(prepare.image_state("img"), True)

    def test_absent(self):
        with mock.patch.object(prepare, "run", return_value=completed(1, stderr="Error: No such image: img")):
            self.assertIs(prepare.image_state("img"), False)

    def test_unexpected_error_is_unknown(self):
        with mock.patch.object(prepare, "run", return_value=completed(1, stderr="permission denied")):
            self.assertIsNone(prepare.image_state("img"))

    def test_docker_missing_is_unknown(self):
        with mock.patch.object(prepare, "run", side_effect=FileNotFoundError("docker")):
            self.assertIsNone(prepare.image_state("img"))

    def test_timeout_is_unknown(self):
        with mock.patch.object(prepare, "run", side_effect=subprocess.TimeoutExpired("docker", 60)):
            self.assertIsNone(prepare.image_state("img"))


class StatePersistenceTests(unittest.TestCase):
    def test_save_state_is_atomic_and_writes_updated_at(self):
        with tempfile.TemporaryDirectory() as tmp:
            state_path = Path(tmp) / "state.json"
            with mock.patch.object(prepare, "STATE", state_path):
                prepare.save_state({"instances": {"a": {"status": "ready"}}})
                self.assertTrue(state_path.exists())
                self.assertFalse(state_path.with_suffix(".tmp").exists())
                data = json.loads(state_path.read_text(encoding="utf-8"))
            self.assertEqual(data["instances"]["a"]["status"], "ready")
            self.assertIsNotNone(data["updated_at"])

    def test_save_state_leaves_no_temporary_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            state_path = Path(tmp) / "state.json"
            with mock.patch.object(prepare, "STATE", state_path):
                prepare.save_state({"instances": {"a": {"status": "ready"}}})
            leftovers = [p.name for p in Path(tmp).iterdir() if p.name != "state.json"]
            self.assertEqual(leftovers, [])

    def test_concurrent_saves_use_unique_temporaries(self):
        """Dos `prepare.py` en paralelo chocaban en `state.tmp` (Windows)."""
        with tempfile.TemporaryDirectory() as tmp:
            state_path = Path(tmp) / "state.json"
            seen = []
            real_write_text = Path.write_text

            def spy(self, *args, **kwargs):
                seen.append(self.name)
                return real_write_text(self, *args, **kwargs)

            with mock.patch.object(prepare, "STATE", state_path):
                with mock.patch.object(Path, "write_text", spy):
                    prepare.save_state({"instances": {}})
                    prepare.save_state({"instances": {}})
            self.assertEqual(len(seen), 2)
            self.assertNotEqual(seen[0], seen[1])
            self.assertTrue(all(name.endswith(".tmp") for name in seen))

    def test_load_state_tolerates_corrupt_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            state_path = Path(tmp) / "state.json"
            state_path.write_text("{not json", encoding="utf-8")
            with mock.patch.object(prepare, "STATE", state_path):
                self.assertEqual(prepare.load_state(), {"updated_at": None, "instances": {}})


class LoadInstancesTests(unittest.TestCase):
    def test_skips_blank_and_corrupt_lines(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "instances.jsonl"
            path.write_text(
                '{"instance_id": "a"}\n\n{broken\n{"instance_id": "b"}\n',
                encoding="utf-8",
            )
            with mock.patch.object(prepare, "INSTANCES", path):
                rows = prepare.load_instances()
            self.assertEqual([r["instance_id"] for r in rows], ["a", "b"])

    def test_missing_file_exits(self):
        with tempfile.TemporaryDirectory() as tmp:
            with mock.patch.object(prepare, "INSTANCES", Path(tmp) / "nope.jsonl"):
                with self.assertRaises(SystemExit) as ctx:
                    prepare.load_instances()
            self.assertEqual(ctx.exception.code, 2)


class FreeSpaceTests(unittest.TestCase):
    def test_returns_gigabytes(self):
        fake = mock.Mock(free=50 * 1024**3)
        with mock.patch.object(prepare.shutil, "disk_usage", return_value=fake):
            self.assertAlmostEqual(prepare.free_gb(), 50.0, places=3)


if __name__ == "__main__":
    unittest.main()
