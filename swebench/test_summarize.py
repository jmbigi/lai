"""Tests offline de swebench/summarize.py (sin Docker ni dataset)."""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent

spec = importlib.util.spec_from_file_location("lai_swebench_summarize", HERE / "summarize.py")
summarize = importlib.util.module_from_spec(spec)
spec.loader.exec_module(summarize)


class TempDataMixin(unittest.TestCase):
    """Redirige DATA_DIR/PREDICTIONS/EVALUATION a un directorio temporal."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        root = Path(self._tmp.name)
        self.data = root
        self.preds = root / "predictions"
        self.eval = root / "evaluation"
        self.preds.mkdir()
        self.eval.mkdir()
        self._orig = (summarize.DATA_DIR, summarize.PREDICTIONS, summarize.EVALUATION)
        summarize.DATA_DIR = root
        summarize.PREDICTIONS = self.preds
        summarize.EVALUATION = self.eval

    def tearDown(self):
        summarize.DATA_DIR, summarize.PREDICTIONS, summarize.EVALUATION = self._orig
        self._tmp.cleanup()

    def write_meta(self, model: str, rows: dict) -> None:
        path = self.preds / f"{summarize.slug(model)}.meta.json"
        path.write_text(json.dumps(rows), encoding="utf-8")

    def write_report(self, run_id: str, model: str, instances: list, summary: dict | None = None) -> None:
        run_dir = self.eval / run_id
        run_dir.mkdir()
        payload = {
            "run": {"run_id": run_id, "model": model, "started_at": "2026-01-01T00:00:00+00:00"},
            "summary": summary or {},
            "instances": instances,
        }
        (run_dir / "report.json").write_text(json.dumps(payload), encoding="utf-8")


class SlugTests(unittest.TestCase):
    def test_replaces_separators(self):
        self.assertEqual(summarize.slug("qwen2.5-coder:7b"), "qwen2_5_coder_7b")

    def test_strips_leading_dots(self):
        self.assertEqual(summarize.slug("..a.."), "a")

    def test_never_empty(self):
        self.assertEqual(summarize.slug("..."), "run")


class FindReportTests(TempDataMixin):
    def test_explicit_run_id(self):
        self.write_report("r1", "m", [])
        run_id, report = summarize.find_report("r1", "m")
        self.assertEqual(run_id, "r1")
        self.assertIsNotNone(report)

    def test_missing_run_id_returns_none_report(self):
        run_id, report = summarize.find_report("nope", "m")
        self.assertEqual(run_id, "nope")
        self.assertIsNone(report)

    def test_auto_discovers_by_model(self):
        self.write_report("r1", "otro-modelo", [])
        self.write_report("r2", "m", [])
        run_id, report = summarize.find_report(None, "m")
        self.assertEqual(run_id, "r2")
        self.assertIsNotNone(report)

    def test_auto_returns_none_when_no_match(self):
        self.write_report("r1", "otro-modelo", [])
        run_id, report = summarize.find_report(None, "m")
        self.assertIsNone(run_id)
        self.assertIsNone(report)

    def test_ignores_corrupt_report(self):
        run_dir = self.eval / "roto"
        run_dir.mkdir()
        (run_dir / "report.json").write_text("{no es json", encoding="utf-8")
        self.write_report("bueno", "m", [])
        run_id, report = summarize.find_report(None, "m")
        self.assertEqual(run_id, "bueno")
        self.assertIsNotNone(report)


class LoadJsonTests(TempDataMixin):
    def test_missing_file_is_none(self):
        self.assertIsNone(summarize.load_json(self.data / "no-existe.json"))

    def test_corrupt_file_is_none(self):
        path = self.data / "roto.json"
        path.write_text("{", encoding="utf-8")
        self.assertIsNone(summarize.load_json(path))


class EndToEndTests(TempDataMixin):
    def call_main(self, argv: list[str]) -> tuple[int, str]:
        import sys

        buf = io.StringIO()
        old = sys.argv
        sys.argv = ["summarize.py"] + argv
        try:
            with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(io.StringIO()):
                code = summarize.main()
        finally:
            sys.argv = old
        return code, buf.getvalue()

    def test_merges_agent_and_evaluation(self):
        self.write_meta(
            "m",
            {
                "i-1": {"status": "patched", "duration_s": 100.0, "patch_bytes": 500, "model": "m", "context_length": 32768},
                "i-2": {"status": "no_patch", "duration_s": 20.0, "patch_bytes": 0, "model": "m", "context_length": 32768},
                "i-3": {"status": "infra_error", "duration_s": 1.0, "patch_bytes": 0, "model": "m", "error": "x"},
            },
        )
        self.write_report(
            "run1",
            "m",
            [
                {"instance_id": "i-1", "resolved": True, "scored": True, "resolution": "RESOLVED_FULL", "patch_bytes": 500},
                {"instance_id": "i-2", "resolved": False, "scored": True, "resolution": "RESOLVED_NO", "patch_bytes": 0},
                {"instance_id": "i-3", "resolved": False, "scored": False, "status": "empty_patch"},
            ],
        )
        code, out = self.call_main(["--model", "m"])
        self.assertEqual(code, 0)
        self.assertIn("i-1", out)
        self.assertIn("32768", out)
        self.assertIn("sí", out)
        self.assertIn("no (RESOLVED_NO)", out)
        self.assertIn("no puntuada", out)
        self.assertIn("errores de infraestructura: 1", out)

    def test_missing_evaluation_still_prints_table(self):
        self.write_meta("m", {"i-1": {"status": "patched", "duration_s": 5.0, "patch_bytes": 10, "model": "m"}})
        code, out = self.call_main(["--model", "m"])
        self.assertEqual(code, 0)
        self.assertIn("falta evaluate.py", out)
        self.assertIn("| i-1 | patched | 10 B | 5.0 s | — | — |", out)

    def test_report_only_without_agent_meta(self):
        self.write_report(
            "oracle",
            "oracle-gold-patch",
            [{"instance_id": "i-1", "resolved": True, "scored": True, "patch_bytes": 464}],
        )
        code, out = self.call_main(["--model", "oracle-gold-patch"])
        self.assertEqual(code, 0)
        self.assertIn("sin datos de agente", out)
        self.assertIn("1/1", out)

    def test_no_data_at_all_exits_2(self):
        code, _ = self.call_main(["--model", "inexistente"])
        self.assertEqual(code, 2)


class MainArgvTests(TempDataMixin):
    """main() lee sys.argv; se parchea para probar los flags reales."""

    def call_main(self, argv: list[str]) -> tuple[int, str]:
        import sys

        buf = io.StringIO()
        old = sys.argv
        sys.argv = ["summarize.py"] + argv
        try:
            with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(io.StringIO()):
                code = summarize.main()
        finally:
            sys.argv = old
        return code, buf.getvalue()

    def test_md_flag_prints_only_table(self):
        self.write_meta("m", {"i-1": {"status": "patched", "duration_s": 1.0, "patch_bytes": 1, "model": "m"}})
        code, out = self.call_main(["--model", "m", "--md"])
        self.assertEqual(code, 0)
        self.assertTrue(out.lstrip().startswith("| Instancia"))
        self.assertNotIn("Modelo:", out)


if __name__ == "__main__":
    unittest.main()
