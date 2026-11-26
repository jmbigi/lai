#!/usr/bin/env python3
"""
Check documentation consistency: versions, ports, context sizes across all docs.

Criterio (Regla 6 y Regla 7):
  * Los documentos **históricos** no se corrigen: se conservan tal como se
    escribieron y aquí solo se informan (ℹ️), no se cuentan como fallo.
  * `16384` no es un error por sí mismo: es la recomendación de contexto para
    SWE-bench (Regla 11). Se informa para revisarlo a mano, no se marca como
    fallo.
  * Sí es un fallo real usar `--reasoning-format none`, que está obsoleto.

Código de salida: 0 si no hay fallos, 1 si hay avisos contados como fallo.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent

# Patrones a revisar. El valor es (regex, descripción, severidad).
#   "ok"      -> correcto, solo informativo
#   "info"    -> revisar a mano, no rompe la build
#   "warning" -> fallo real, rompe la build
CHECKS = {
    "llama_port": (r"8080", "llama.cpp port", "ok"),
    "gateway_port": (r"8081", "gateway port", "ok"),
    "ollama_port": (r"11434", "ollama port", "ok"),
    "lai_serve_port": (r"8082", "lai serve port", "ok"),
    "context_32768": (r"32768", "context window", "ok"),
    "context_16384": (
        r"16384",
        "contexto 16384 (recomendación SWE-bench; verificar que no sea el valor por defecto)",
        "info",
    ),
    "version": (r"v?0\.0\.[23]", "version", "ok"),
    "reasoning_format_none": (
        r"--reasoning-format\s+none",
        "reasoning-format none (obsoleto: usar deepseek)",
        "warning",
    ),
    "reasoning_format_deepseek": (
        r"--reasoning-format\s+deepseek",
        "reasoning-format deepseek (actual)",
        "ok",
    ),
}

# Documentos que registran el pasado: no se reescriben (Regla 6).
HISTORICAL_FILES = {
    "LECCIONES_APRENDIDAS.md",
    "DOCUMENTACION_FINAL.md",
    "DOCUMENTACION_INTENTO.md",
    "ANALISIS_EXHAUSTIVO.md",
    "MEJORAS_IMPLEMENTADAS.md",
    "COMPARACION_2026.md",
    "COMPARACION_KIMICODE.md",
}

DOC_FILES = [
    "README.md",
    "ESTADO_ACTUAL.md",
    "CHANGELOG.md",
    "DOCUMENTACION_REGLAS.md",
    "DOCS/REVISION_2026.md",
    "DOCS/CHANGELOG.md",
    "DOCS/quality_comparison.md",
    "DOCS/INFERENCIA_LOCAL_BAJA_GAMA.md",
    "LECCIONES_APRENDIDAS.md",
    "DOCUMENTACION_FINAL.md",
    "DOCUMENTACION_INTENTO.md",
    "ANALISIS_EXHAUSTIVO.md",
    "MEJORAS_IMPLEMENTADAS.md",
]

MARKERS = {"ok": "✓", "info": "ℹ", "warning": "⚠"}


def scan_file(filepath: Path) -> list[tuple[str, str, str, int, str]]:
    """Devuelve (clave, descripción, severidad, línea, coincidencia)."""
    try:
        content = filepath.read_text(encoding="utf-8")
    except OSError:
        return []
    results = []
    for key, (pattern, desc, severity) in CHECKS.items():
        for match in list(re.finditer(pattern, content, re.IGNORECASE))[:3]:
            line_no = content[: match.start()].count("\n") + 1
            results.append((key, desc, severity, line_no, match.group()[:60]))
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--show-ok",
        action="store_true",
        help="Mostrar también las comprobaciones correctas (por defecto se ocultan)",
    )
    args = parser.parse_args()

    failures = 0
    info_count = 0

    for name in DOC_FILES:
        path = ROOT / name
        if not path.exists():
            print(f"SKIP: {name} (not found)")
            continue
        issues = scan_file(path)
        historical = name in HISTORICAL_FILES
        lines = []
        for key, desc, severity, line, match in issues:
            if historical and severity != "ok":
                severity = "info"
            if severity == "ok" and not args.show_ok:
                continue
            suffix = " [histórico: se conserva]" if historical and severity != "ok" else ""
            lines.append(f"  {MARKERS[severity]} line {line}: {desc} -> {match}{suffix}")
            if severity == "warning":
                failures += 1
            elif severity == "info":
                info_count += 1
        if lines:
            print(f"\n{name}:")
            print("\n".join(lines))

    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)
    print(f"  fallos (warning): {failures}")
    print(f"  avisos informativos: {info_count}")
    if failures:
        print("\nCorrige los fallos antes de publicar la documentación.")
        return 1
    print("Sin fallos. Los avisos informativos se revisan a mano (Regla 11).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
