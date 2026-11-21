#!/usr/bin/env python3
"""
Check documentation consistency: versions, ports, context sizes across all docs.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent

# Patterns to check
CHECKS = {
    "llama_port": (r"8080", "llama.cpp port"),
    "gateway_port": (r"8081", "gateway port"),
    "ollama_port": (r"11434", "ollama port"),
    "lai_serve_port": (r"8082", "lai serve port"),
    "context_32768": (r"32768", "context window"),
    "context_16384": (r"16384", "old context window (should be 32768)"),
    "version": (r"v?0\.0\.[23]", "version"),
    "reasoning_format_none": (r"--reasoning-format\s+none", "reasoning-format none (deprecated)"),
    "reasoning_format_deepseek": (r"--reasoning-format\s+deepseek", "reasoning-format deepseek (current)"),
}

DOC_FILES = [
    "README.md",
    "ESTADO_ACTUAL.md",
    "CHANGELOG.md",
    "DOCUMENTACION_REGLAS.md",
    "DOCS/REVISION_2026.md",
    "DOCS/CHANGELOG.md",
    "DOCS/quality_comparison.md",
    "LECCIONES_APRENDIDAS.md",
    "DOCUMENTACION_FINAL.md",
    "DOCUMENTACION_INTENTO.md",
    "ANALISIS_EXHAUSTIVO.md",
    "MEJORAS_IMPLEMENTADAS.md",
]

def scan_file(filepath):
    """Scan a file for patterns."""
    try:
        content = filepath.read_text(encoding="utf-8")
    except Exception:
        return []
    results = []
    for key, (pattern, desc) in CHECKS.items():
        matches = list(re.finditer(pattern, content, re.IGNORECASE))
        if matches:
            for m in matches[:3]:  # limit output
                line_no = content[:m.start()].count('\n') + 1
                results.append((key, desc, line_no, m.group()[:60]))
    return results

def main():
    all_issues = []
    for fname in DOC_FILES:
        fpath = ROOT / fname
        if not fpath.exists():
            print(f"SKIP: {fname} (not found)")
            continue
        issues = scan_file(fpath)
        if issues:
            print(f"\n{fname}:")
            for key, desc, line, match in issues:
                marker = "⚠" if key in ("context_16384", "reasoning_format_none") else "✓"
                print(f"  {marker} line {line}: {desc} -> {match}")
                all_issues.append((fname, key, desc, line, match))

    # Summary
    print("\n" + "="*60)
    print("SUMMARY")
    print("="*60)
    if not all_issues:
        print("All checks passed!")
        return 0

    # Count by type
    from collections import Counter
    cnt = Counter(key for _, key, _, _, _ in all_issues)
    for key, count in cnt.most_common():
        desc = CHECKS[key][1]
        print(f"  {desc}: {count} occurrences")

    return 0


if __name__ == "__main__":
    sys.exit(main())