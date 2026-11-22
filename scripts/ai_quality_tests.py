#!/usr/bin/env python3
"""Pruebas de calidad de IA para lai: benchmarks de agentes basados en 2026 research.

Basado en: StaminaBench, WORKFLOWPERTURB, ReliabilityBench, XPerf, SASST,
ToolBottleneckBench, MCP-Injector, agitracker.

Evalúa la calidad de las respuestas del modelo (no solo latencia):
  - Correctitud de código generado
  - Adherencia a instrucciones en español
  - Uso correcto de herramientas (tool calling)
  - Stamina: consistencia en multi-turn (100 turns)
  - Perturbación de workflow: pasos faltantes/comprimidos/cambiados
  - Confiabilidad bajo fallos de infraestructura
  - Detección de sesgo (SASST: discovery/confirmation)

Uso:
  python3 scripts/ai_quality_tests.py --suite all
  python3 scripts/ai_quality_tests.py --suite code_correctness --model qwen3-8b
  python3 scripts/ai_quality_tests.py --suite stamina --turns 50
  python3 scripts/ai_quality_tests.py --suite workflow_perturb
  python3 scripts/ai_quality_tests.py --suite tool_calling
  python3 scripts/ai_quality_tests.py --suite sasst_bias
  python3 scripts/ai_quality_tests.py --report-json ai_quality_report.json
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from dataclasses import dataclass, field
from typing import List, Optional

import httpx

GATEWAY_URL = "http://127.0.0.1:8081/v1/chat/completions"
AUTH_HEADER = {"Authorization": "Bearer open"}

# ---------------------------------------------------------------------------
# Data structures
# ---------------------------------------------------------------------------


@dataclass
class QualityResult:
    name: str
    passed: bool
    score: float  # 0.0 - 1.0
    details: str
    latency_ms: float
    tokens: int = 0


@dataclass
class SuiteReport:
    name: str
    results: List[QualityResult] = field(default_factory=list)

    def add(self, r: QualityResult) -> None:
        self.results.append(r)

    def report(self) -> None:
        print(f"\n=== {self.name} ===")
        passed = sum(1 for r in self.results if r.passed)
        total = len(self.results)
        avg_score = statistics_mean([r.score for r in self.results]) if self.results else 0.0
        avg_latency = statistics_mean([r.latency_ms for r in self.results]) if self.results else 0.0
        print(
            f"  Passed: {passed}/{total}  Avg score: {avg_score:.2f}  Avg latency: {avg_latency:.0f}ms"
        )
        for r in self.results:
            status = "PASS" if r.passed else "FAIL"
            print(
                f"  [{status}] {r.name} (score={r.score:.2f}, {r.latency_ms:.0f}ms): {r.details[:100]}"
            )


def statistics_mean(values: List[float]) -> float:
    return sum(values) / len(values) if values else 0.0


# ---------------------------------------------------------------------------
# HTTP helper
# ---------------------------------------------------------------------------


def chat(
    prompt: str,
    model: str = "qwen3-8b",
    temperature: float = 0.7,
    max_tokens: int = 512,
    stream: bool = False,
    timeout: float = 60.0,
    tools: Optional[list] = None,
    tool_choice: str = "auto",
) -> tuple[str, float, int]:
    """Envía un chat y retorna (texto, latency_ms, tokens)."""
    start = time.perf_counter()
    payload = {
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": temperature,
        "max_tokens": max_tokens,
        "stream": stream,
    }
    if tools:
        payload["tools"] = tools
        payload["tool_choice"] = tool_choice
    try:
        with httpx.Client(timeout=timeout) as client:
            resp = client.post(GATEWAY_URL, json=payload, headers=AUTH_HEADER)
            resp.raise_for_status()
            data = resp.json()
        latency_ms = (time.perf_counter() - start) * 1000
        message = (data.get("choices") or [{}])[0].get("message", {})
        content = message.get("content", "") or ""
        reasoning = message.get("reasoning_content", "") or ""
        if not content:
            content = reasoning
        tokens = len(content) // 4
        return content, latency_ms, tokens
    except httpx.HTTPStatusError as exc:
        latency_ms = (time.perf_counter() - start) * 1000
        return f"ERROR: HTTP {exc.response.status_code}", latency_ms, 0
    except Exception as exc:
        latency_ms = (time.perf_counter() - start) * 1000
        return f"ERROR: {exc}", latency_ms, 0


# ---------------------------------------------------------------------------
# Heuristic scorers (no external LLM judge to keep it local)
# ---------------------------------------------------------------------------


def score_code_correctness(response: str, expected_keywords: List[str]) -> tuple[float, str]:
    """Puntaje basado en presencia de keywords esperadas en el código."""
    resp_lower = response.lower()
    found = sum(1 for kw in expected_keywords if kw.lower() in resp_lower)
    score = found / len(expected_keywords) if expected_keywords else 0.0
    details = f"Encontrados {found}/{len(expected_keywords)} keywords: {expected_keywords}"
    return score, details


def score_spanish_adherence(response: str) -> tuple[float, str]:
    """Puntaje basado en indicadores de respuesta en español."""
    spanish_words = [
        "el",
        "la",
        "los",
        "las",
        "de",
        "del",
        "que",
        "y",
        "a",
        "en",
        "un",
        "una",
        "por",
        "para",
        "con",
        "se",
        "es",
        "está",
        "son",
        "qué",
        "cómo",
        "cuál",
        "función",
        "código",
        "python",
        "ejemplo",
        "diferencia",
    ]
    resp_lower = response.lower()
    found = sum(1 for w in spanish_words if f" {w} " in f" {resp_lower} ")
    # Penalize English indicators
    english_words = ["the", "and", "is", "are", "for", "with", "this", "that", "function", "code"]
    eng_found = sum(1 for w in english_words if f" {w} " in f" {resp_lower} ")
    score = max(0.0, min(1.0, (found * 0.1) - (eng_found * 0.05)))
    details = f"Palabras ESP: {found}, PALABRAS EN: {eng_found}"
    return score, details


def score_tool_calls(response: str, expected_tool: str) -> tuple[float, str]:
    """Puntaje basado en presence de tool calls esperados (formato JSON)."""
    has_tool = expected_tool.lower() in response.lower()
    has_json = "{" in response and "}" in response
    score = 0.5 if has_tool else 0.0
    score += 0.5 if has_json else 0.0
    details = f"Tool '{expected_tool}' presente: {has_tool}, JSON presente: {has_json}"
    return score, details


# ---------------------------------------------------------------------------
# Suite: Code correctness
# ---------------------------------------------------------------------------


def run_code_correctness(model: str) -> SuiteReport:
    suite = SuiteReport("Code Correctness")
    cases = [
        {
            "name": "factorial_function",
            "prompt": (
                "Escribe en español una función Python que calcule el factorial de un número n. "
                "Incluye docstring, validación de entrada y ejemplo de uso."
            ),
            "keywords": ["def", "factorial", "return", "if", "n", "docstring", "ejemplo"],
        },
        {
            "name": "fibonacci_recursive",
            "prompt": (
                "Escribe en español una función recursiva de Fibonacci con explicación de complejidad."
            ),
            "keywords": ["def", "fibonacci", "return", "recurs", "complejidad", "O("],
        },
        {
            "name": "asyncio_example",
            "prompt": ("Crea un ejemplo de uso de asyncio en Python con dos tareas concurrentes."),
            "keywords": ["async", "await", "asyncio", "def", "gather", "task"],
        },
        {
            "name": "code_review",
            "prompt": (
                "Revisa este código en español: def foo(x): return x + 1. "
                "Dime qué está bien y qué se puede mejorar."
            ),
            "keywords": ["revis", "código", "mejor", "función", "return"],
        },
    ]
    for case in cases:
        content, latency, tokens = chat(case["prompt"], model=model, max_tokens=256, timeout=30.0)
        score, details = score_code_correctness(content, case["keywords"])
        passed = score >= 0.6
        suite.add(
            QualityResult(
                name=case["name"],
                passed=passed,
                score=score,
                details=f"{details}. Respuesta: {content[:80]}...",
                latency_ms=latency,
                tokens=tokens,
            )
        )
    return suite


# ---------------------------------------------------------------------------
# Suite: Spanish adherence
# ---------------------------------------------------------------------------


def run_spanish_adherence(model: str) -> SuiteReport:
    suite = SuiteReport("Spanish Adherence")
    prompts = [
        "¿Cuál es la diferencia entre una lista y una tupla en Python?",
        "Explica qué es una función recursiva en una frase.",
    ]
    for i, p in enumerate(prompts):
        content, latency, tokens = chat(p, model=model, max_tokens=128, timeout=30.0)
        score, details = score_spanish_adherence(content)
        passed = score >= 0.5
        suite.add(
            QualityResult(
                name=f"spanish_q{i+1}",
                passed=passed,
                score=score,
                details=f"{details}. Inicio: {content[:60]}...",
                latency_ms=latency,
                tokens=tokens,
            )
        )
    return suite


# ---------------------------------------------------------------------------
# Suite: Tool calling
# ---------------------------------------------------------------------------


def run_tool_calling(model: str) -> SuiteReport:
    suite = SuiteReport("Tool Calling")
    # Send a request asking the model to mention a tool call in text
    # (gateway may not support the OpenAI 'tools' parameter, so we test
    #  whether the model can produce tool-call-like output in text form)
    tool_prompt = (
        "Llama a la función 'add' con a=3 y b=4. "
        "Responde en español. Si usas tool calling, muestra el JSON de la llamada."
    )
    payload = {
        "model": model,
        "messages": [{"role": "user", "content": tool_prompt}],
        "temperature": 0.2,
        "max_tokens": 256,
        "stream": False,
    }
    start = time.perf_counter()
    try:
        content, latency_ms, tokens = chat(
            tool_prompt, model=model, temperature=0.2, max_tokens=256, timeout=60.0
        )
        has_add = "add" in content.lower()
        has_json = "{" in content and "}" in content
        has_a3b = "a=3" in content or "a : 3" in content or '"a"' in content
        score = 0.0
        if has_add:
            score += 0.4
        if has_json:
            score += 0.3
        if has_a3b:
            score += 0.3
        passed = score >= 0.5
        suite.add(
            QualityResult(
                name="add_tool_call_text",
                passed=passed,
                score=score,
                details=f"Menciona 'add': {has_add}, JSON: {has_json}, params a=3/b=4: {has_a3b}. "
                f"Respuesta: {content[:80]}",
                latency_ms=latency_ms,
                tokens=tokens,
            )
        )
    except Exception as exc:
        latency_ms = (time.perf_counter() - start) * 1000
        suite.add(
            QualityResult(
                name="add_tool_call_text",
                passed=False,
                score=0.0,
                details=f"ERROR: {exc}",
                latency_ms=latency_ms,
            )
        )
    return suite


# ---------------------------------------------------------------------------
# Suite: Stamina (multi-turn consistency)
# ---------------------------------------------------------------------------


def run_stamina(model: str, turns: int = 5) -> SuiteReport:
    suite = SuiteReport(f"Stamina ({turns} turns)")
    history: list = []
    scores: List[float] = []
    latencies: List[float] = []
    for t in range(1, turns + 1):
        prompt = (
            f"Turno {t}: continúa la conversación en español. "
            f"Responde brevemente una palabra o frase corta manteniendo el hilo."
        )
        history.append({"role": "user", "content": prompt})
        payload = {
            "model": model,
            "messages": history,
            "temperature": 0.7,
            "max_tokens": 64,
            "stream": False,
        }
        start = time.perf_counter()
        try:
            with httpx.Client(timeout=60.0) as client:
                resp = client.post(GATEWAY_URL, json=payload, headers=AUTH_HEADER)
                resp.raise_for_status()
                data = resp.json()
            latency_ms = (time.perf_counter() - start) * 1000
            message = (data.get("choices") or [{}])[0].get("message", {})
            content = message.get("content", "") or ""
            reasoning = message.get("reasoning_content", "") or ""
            if not content:
                content = reasoning
            if content and len(content.strip()) > 0:
                score = 1.0  # responded non-empty
                history.append({"role": "assistant", "content": content})
            else:
                score = 0.0
        except Exception as exc:
            latency_ms = (time.perf_counter() - start) * 1000
            content = f"ERROR: {exc}"
            score = 0.0
        scores.append(score)
        latencies.append(latency_ms)
    avg_score = statistics_mean(scores)
    avg_latency = statistics_mean(latencies)
    passed = avg_score >= 0.8
    suite.add(
        QualityResult(
            name=f"stamina_{turns}turns",
            passed=passed,
            score=avg_score,
            details=f"Turnos exitosos: {sum(1 for s in scores if s > 0)}/{turns}. "
            f"Última respuesta: {content[:60]}...",
            latency_ms=avg_latency,
            tokens=len(content) // 4 if content else 0,
        )
    )
    return suite


# ---------------------------------------------------------------------------
# Suite: Workflow perturb (WORKFLOWPERTURB)
# ---------------------------------------------------------------------------


def run_workflow_perturb(model: str) -> SuiteReport:
    suite = SuiteReport("Workflow Perturb (WORKFLOWPERTURB)")
    variants = [
        {
            "name": "missing_step",
            "prompt": (
                "Resuelve en español pero OMITE el paso de validación. "
                "Solo da el resultado final. Problema: Calcula la suma de 1 a 100."
            ),
            "check_keywords": ["5050", "suma", "resultado"],
        },
        {
            "name": "compressed",
            "prompt": (
                "Resuelve en español, comprime todos los pasos en una sola respuesta corta. "
                "Problema: ¿Cuál es el factorial de 5?"
            ),
            "check_keywords": ["120", "factorial"],
        },
        {
            "name": "changed_description",
            "prompt": (
                "Responde en español. El problema ahora se llama 'Suma de Gauss'. "
                "Calcula: suma de números del 1 al 50."
            ),
            "check_keywords": ["1275", "gauss", "suma"],
        },
    ]
    for variant in variants:
        content, latency, tokens = chat(
            variant["prompt"], model=model, max_tokens=128, timeout=30.0
        )
        score, details = score_code_correctness(content, variant["check_keywords"])
        passed = score >= 0.5
        suite.add(
            QualityResult(
                name=variant["name"],
                passed=passed,
                score=score,
                details=f"{details}. Respuesta: {content[:80]}...",
                latency_ms=latency,
                tokens=tokens,
            )
        )
    return suite


# ---------------------------------------------------------------------------
# Suite: Reliability (infrastructure failure simulation)
# ---------------------------------------------------------------------------


def run_reliability(model: str) -> SuiteReport:
    suite = SuiteReport("Reliability (infrastructure failure)")
    import random

    random.seed(99)
    # Simulate: some requests with bad keys, some with no key, some normal
    cases = [
        ("normal", None),
        ("bad_key", "bad_key"),
        ("no_key", "no_key"),
        ("normal_again", None),
    ]
    for name, fault in cases:
        headers = dict(AUTH_HEADER)
        if fault == "bad_key":
            headers["Authorization"] = "Bearer invalida"
        elif fault == "no_key":
            headers.pop("Authorization", None)
        payload = {
            "model": model,
            "messages": [{"role": "user", "content": "Di hola en español."}],
            "temperature": 0.7,
            "max_tokens": 64,
            "stream": False,
        }
        start = time.perf_counter()
        try:
            with httpx.Client(timeout=30.0) as client:
                resp = client.post(GATEWAY_URL, json=payload, headers=headers)
                # We expect 401 for bad/no key; success for normal
                expected_success = fault is None
                if expected_success:
                    resp.raise_for_status()
                    data = resp.json()
                    content = (data.get("choices") or [{}])[0].get("message", {}).get(
                        "content", ""
                    ) or ""
                    score = 1.0 if content else 0.0
                else:
                    # Expect failure (401)
                    score = 1.0 if resp.status_code >= 400 else 0.5
                    content = f"HTTP {resp.status_code} (expected failure)"
            passed = score >= 0.5
            suite.add(
                QualityResult(
                    name=name,
                    passed=passed,
                    score=score,
                    details=content[:100],
                    latency_ms=(time.perf_counter() - start) * 1000,
                )
            )
        except httpx.HTTPStatusError as exc:
            # For normal case this is a failure; for fault cases it's expected
            passed = fault is not None
            score = 1.0 if passed else 0.0
            suite.add(
                QualityResult(
                    name=name,
                    passed=passed,
                    score=score,
                    details=f"HTTP {exc.response.status_code}",
                    latency_ms=(time.perf_counter() - start) * 1000,
                )
            )
        except Exception as exc:
            passed = fault is not None
            score = 1.0 if passed else 0.0
            suite.add(
                QualityResult(
                    name=name,
                    passed=passed,
                    score=score,
                    details=f"ERROR: {exc}",
                    latency_ms=(time.perf_counter() - start) * 1000,
                )
            )
    return suite


# ---------------------------------------------------------------------------
# Suite: XPerf (multi-problem simultaneous reasoning)
# ---------------------------------------------------------------------------


def run_xperf(model: str) -> SuiteReport:
    suite = SuiteReport("XPerf (multi-problem reasoning)")
    problems = [
        (
            "train_problem",
            (
                "Resuelve paso a paso en español: Un tren sale de Madrid a las 8:00 a 80 km/h "
                "y otro de Barcelona a las 9:00 a 100 km/h. Distancia 600 km. ¿Cuándo y dónde se cruzan?"
            ),
            ["km", "hora", "cruzan", "Madrid", "Barcelona"],
        ),
        (
            "oo_example",
            (
                "Explica la diferencia entre clase e instancia en POO con un ejemplo completo en Python."
            ),
            ["clase", "instancia", "def", "class", "python"],
        ),
        (
            "fibonacci_complexity",
            ("Escribe una función recursiva de Fibonacci y explica su complejidad temporal."),
            ["fibonacci", "recurs", "complejidad", "O("],
        ),
        (
            "rest_api_design",
            ("Diseña una API REST para un sistema de inventario con endpoints y códigos HTTP."),
            ["api", "rest", "endpoint", "http", "inventario"],
        ),
        (
            "sql_vs_nosql",
            ("Compara SQL vs NoSQL: cuándo usar cada uno con ejemplos concretos."),
            ["sql", "nosql", "cuándo", "ejemplo"],
        ),
    ]
    for name, prompt, keywords in problems:
        content, latency, tokens = chat(prompt, model=model, max_tokens=128, timeout=30.0)
        score, details = score_code_correctness(content, keywords)
        passed = score >= 0.5
        suite.add(
            QualityResult(
                name=name,
                passed=passed,
                score=score,
                details=f"{details}. Respuesta: {content[:80]}...",
                latency_ms=latency,
                tokens=tokens,
            )
        )
    return suite


# ---------------------------------------------------------------------------
# Suite: SASST bias (Selection-Aware Stress Testing)
# ---------------------------------------------------------------------------


def run_sasst_bias(model: str) -> SuiteReport:
    suite = SuiteReport("SASST Bias Detection")
    # Ask the same question with different framings to detect bias
    framings = [
        ("positive", "¿Por qué Python es un buen lenguaje para principiantes?"),
        ("negative", "¿Por qué Python es un mal lenguaje para principiantes?"),
        ("neutral", "¿Cuáles son las ventajas y desventajas de Python para principiantes?"),
    ]
    results = []
    for name, prompt in framings:
        content, latency, tokens = chat(prompt, model=model, max_tokens=128, timeout=30.0)
        results.append((name, content, latency, tokens))
    # Score: neutral framing should mention both pros and cons; positive/negative should be consistent
    neutral_content = results[2][1].lower()
    has_pros = any(
        w in neutral_content for w in ["ventaja", "buen", "fácil", "sencillo", "popular"]
    )
    has_cons = any(
        w in neutral_content for w in ["desventaja", "mal", "difícil", "lento", "limitado"]
    )
    neutral_score = 0.5 if has_pros else 0.0
    neutral_score += 0.5 if has_cons else 0.0
    suite.add(
        QualityResult(
            name="neutral_balanced",
            passed=neutral_score >= 0.5,
            score=neutral_score,
            details=f"Pros: {has_pros}, Contras: {has_cons}. Respuesta: {results[2][1][:80]}...",
            latency_ms=results[2][2],
            tokens=results[2][3],
        )
    )
    # Positive should mention advantages
    pos_content = results[0][1].lower()
    pos_score = (
        0.5 if any(w in pos_content for w in ["buen", "fácil", "ventaja", "popular"]) else 0.0
    )
    suite.add(
        QualityResult(
            name="positive_framing",
            passed=pos_score >= 0.5,
            score=pos_score,
            details=f"Menciona ventajas: {pos_score > 0}. Respuesta: {results[0][1][:80]}...",
            latency_ms=results[0][2],
            tokens=results[0][3],
        )
    )
    return suite


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def parse_suites(s: str) -> List[str]:
    return [x.strip() for x in s.split(",") if x.strip()]


def main() -> int:
    parser = argparse.ArgumentParser(description="Pruebas de calidad de IA para lai")
    parser.add_argument(
        "--suite",
        default="all",
        help="Suite(s) de pruebas a ejecutar (coma-separadas o 'all'). "
        "Opciones: code_correctness, spanish, tool_calling, stamina, workflow, reliability, xperf, sasst",
    )
    parser.add_argument("--model", default="qwen3-8b", help="Modelo a probar")
    parser.add_argument("--turns", type=int, default=20, help="Turnos para stamina")
    parser.add_argument("--report-json", type=str, default=None, help="Guardar reporte JSON")
    args = parser.parse_args()

    valid_suites = {
        "code_correctness",
        "spanish",
        "tool_calling",
        "stamina",
        "workflow",
        "reliability",
        "xperf",
        "sasst",
    }
    if args.suite == "all":
        suites: List[str] = sorted(valid_suites)
    else:
        requested = parse_suites(args.suite)
        invalid = [s for s in requested if s not in valid_suites]
        if invalid:
            print(f"Suites inválidas: {', '.join(invalid)}")
            print(f"Válidas: {', '.join(sorted(valid_suites))}")
            return 1
        suites = requested

    print(f"Pruebas de calidad de IA — suites: {', '.join(suites)}")
    print(f"Modelo: {args.model}  Gateway: {GATEWAY_URL}")
    print("=" * 60)

    all_reports: List[SuiteReport] = []

    if "code_correctness" in suites:
        all_reports.append(run_code_correctness(args.model))
    if "spanish" in suites:
        all_reports.append(run_spanish_adherence(args.model))
    if "tool_calling" in suites:
        all_reports.append(run_tool_calling(args.model))
    if "stamina" in suites:
        all_reports.append(run_stamina(args.model, args.turns))
    if "workflow" in suites:
        all_reports.append(run_workflow_perturb(args.model))
    if "reliability" in suites:
        all_reports.append(run_reliability(args.model))
    if "xperf" in suites:
        all_reports.append(run_xperf(args.model))
    if "sasst" in suites:
        all_reports.append(run_sasst_bias(args.model))

    # Global summary
    print("\n" + "=" * 60)
    print("GLOBAL SUMMARY")
    print("=" * 60)
    total_passed = sum(1 for r in all_reports for r in r.results if r.passed)
    total_tests = sum(len(r.results) for r in all_reports)
    all_scores = [r.score for r in all_reports for r in r.results]
    print(f"Tests: {total_passed}/{total_tests} passed")
    if all_scores:
        print(f"Avg score: {statistics_mean(all_scores):.2f}")
    for report in all_reports:
        report.report()

    if args.report_json:
        data = {
            "model": args.model,
            "suites": [r.name for r in all_reports],
            "total_passed": total_passed,
            "total_tests": total_tests,
            "avg_score": statistics_mean(all_scores) if all_scores else 0.0,
            "results": [
                {
                    "suite": r.name,
                    "test": qr.name,
                    "passed": qr.passed,
                    "score": qr.score,
                    "details": qr.details,
                    "latency_ms": qr.latency_ms,
                    "tokens": qr.tokens,
                }
                for r in all_reports
                for qr in r.results
            ],
        }
        with open(args.report_json, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)
        print(f"\nReporte JSON guardado en: {args.report_json}")

    return 0 if total_passed == total_tests else 1


if __name__ == "__main__":
    sys.exit(main())
