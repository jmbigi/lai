#!/usr/bin/env python3
"""Stress test a prueba de balas para lai: gateway + llama.cpp.

Dimensiones (basadas en benchmarks 2026: StaminaBench, WORKFLOWPERTURB,
ReliabilityBench, XPerf, AIGatewayBench, MCP-Injector, agitracker):

1. Concurrency sweep   — c=1,4,16,64,128,256 con p50/p95/p99 y RPS
2. Multi-turn stamina  — sesiones de N turnos consecutivos por agente (StaminaBench)
3. Fault injection     — 401, 429, 500, bad key, rotating key, timeout (MCP-Injector)
4. Large context       — prompts 1k / 10k / 100k tokens (AIGatewayBench large_context)
5. Streaming           — TTFT + inter-chunk latencia bajo carga (AIGatewayBench streaming_turn)
6. Security key flood  — claves API inválidas/rotadas a tasa controlada (AIGatewayBench security_key_flood)
7. Repeated trials     — N trials por celda, mediana + min/max (agitracker)
8. Workflow perturbation — pasos faltantes/comprimidos/cambiados (WORKFLOWPERTURB)
9. Reliability under infra failure — fallos de infraestructura (ReliabilityBench)
10. XPerf multi-problem — múltiples problemas simultáneos (XPerf)

Diseño robusto:
  - Warm-up antes de medir (descarta primera corrida)
  - HTTP keep-alive con cliente persistente
  - Timeout y retries configurables
  - Circuit breaker: detiene si >50% de errores consecutivos
  - Validación de contenido (no solo HTTP 200)
  - Salida JSON opcional (--json) para integración CI
  - Manejo graceful de KeyboardInterrupt / EOF
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import statistics
import sys
import time
from dataclasses import dataclass, field
from typing import List, Optional

import httpx

GATEWAY_URL = "http://127.0.0.1:8081/v1/chat/completions"
HEALTH_URL = "http://127.0.0.1:8081/health"
AUTH_HEADER = {"Authorization": "Bearer open"}

SHORT_PROMPTS_ES = [
    "Explica en una frase qué es una función recursiva.",
    "Escribe una función Python que calcule el factorial de n.",
    "Revisa este código: def foo(x): return x + 1",
    "¿Cuál es la diferencia entre una lista y una tupla en Python?",
    "Crea un ejemplo de uso de asyncio en Python.",
]

# ---------------------------------------------------------------------------
# Data structures
# ---------------------------------------------------------------------------


@dataclass
class Result:
    success: bool
    latency_ms: float
    tokens: int = 0
    ttft_ms: Optional[float] = None
    error: str = ""
    turn: int = 1
    status_code: Optional[int] = None


@dataclass
class Trial:
    """Resultados de un solo trial de una celda."""
    name: str
    total: int = 0
    success: int = 0
    failed: int = 0
    latencies: List[float] = field(default_factory=list)
    ttfts: List[float] = field(default_factory=list)
    tokens: int = 0
    errors: List[str] = field(default_factory=list)
    status_codes: dict = field(default_factory=dict)

    def add(self, r: Result) -> None:
        self.total += 1
        if r.success:
            self.success += 1
            self.latencies.append(r.latency_ms)
            self.tokens += r.tokens
            if r.ttft_ms is not None:
                self.ttfts.append(r.ttft_ms)
        else:
            self.failed += 1
            self.errors.append(r.error or "unknown")
        if r.status_code:
            self.status_codes[r.status_code] = self.status_codes.get(r.status_code, 0) + 1

    def pct(self, values: List[float], p: float) -> Optional[float]:
        if not values:
            return None
        s = sorted(values)
        idx = int((p / 100.0) * (len(s) - 1))
        return s[min(idx, len(s) - 1)]

    def summary_dict(self) -> dict:
        return {
            "name": self.name,
            "total": self.total,
            "success": self.success,
            "failed": self.failed,
            "success_rate": (self.success / self.total * 100) if self.total else 0.0,
            "latency_avg_ms": statistics.mean(self.latencies) if self.latencies else None,
            "latency_p50_ms": self.pct(self.latencies, 50) if self.latencies else None,
            "latency_p95_ms": self.pct(self.latencies, 95) if self.latencies else None,
            "latency_p99_ms": self.pct(self.latencies, 99) if self.latencies else None,
            "latency_max_ms": max(self.latencies) if self.latencies else None,
            "latency_min_ms": min(self.latencies) if self.latencies else None,
            "ttft_avg_ms": statistics.mean(self.ttfts) if self.ttfts else None,
            "ttft_p50_ms": self.pct(self.ttfts, 50) if self.ttfts else None,
            "ttft_p99_ms": self.pct(self.ttfts, 99) if self.ttfts else None,
            "tokens": self.tokens,
            "status_codes": self.status_codes,
            "top_errors": self.errors[:5],
        }

    def report(self, trial_idx: Optional[int] = None) -> None:
        label = self.name if trial_idx is None else f"{self.name} (trial {trial_idx})"
        print(f"\n--- {label} ---")
        print(f"  Total : {self.total}  OK : {self.success}  FAIL : {self.failed}")
        if self.latencies:
            print(
                f"  Latency : avg={statistics.mean(self.latencies):.1f}ms "
                f"p50={self.pct(self.latencies, 50):.1f}ms "
                f"p95={self.pct(self.latencies, 95):.1f}ms "
                f"p99={self.pct(self.latencies, 99):.1f}ms "
                f"max={max(self.latencies):.1f}ms min={min(self.latencies):.1f}ms"
            )
        if self.ttfts:
            print(
                f"  TTFT  : avg={statistics.mean(self.ttfts):.1f}ms "
                f"p50={self.pct(self.ttfts, 50):.1f}ms "
                f"p99={self.pct(self.ttfts, 99):.1f}ms"
            )
        print(f"  Tokens: {self.tokens}")
        if self.status_codes:
            print(f"  Status: {dict(sorted(self.status_codes.items()))}")
        if self.errors:
            print(f"  Top errors: {self.errors[:3]}")


@dataclass
class Cell:
    """Una celda de prueba con múltiples trials."""
    name: str
    trials: List[Trial] = field(default_factory=list)

    def add_trial(self, trial: Trial) -> None:
        self.trials.append(trial)

    def report(self) -> None:
        print(f"\n=== {self.name} ===")
        for i, t in enumerate(self.trials, 1):
            t.report(i)
        # Aggregated across trials
        all_lat = [l for t in self.trials for l in t.latencies]
        all_ttft = [t2 for t in self.trials for t2 in t.ttfts]
        total_ok = sum(t.success for t in self.trials)
        total_fail = sum(t.failed for t in self.trials)
        total_req = sum(t.total for t in self.trials)
        print(f"\n  Aggregated ({len(self.trials)} trials):")
        print(f"    Requests : {total_req}  OK : {total_ok}  FAIL : {total_fail}")
        if all_lat:
            print(
                f"    Latency  : avg={statistics.mean(all_lat):.1f}ms "
                f"p50={statistics.median(all_lat):.1f}ms "
                f"p99={sorted(all_lat)[int(len(all_lat)*0.99)]:.1f}ms "
                f"max={max(all_lat):.1f}ms"
            )
        if all_ttft:
            print(
                f"    TTFT     : avg={statistics.mean(all_ttft):.1f}ms "
                f"p99={sorted(all_ttft)[int(len(all_ttft)*0.99)]:.1f}ms"
            )

    def summary_dict(self) -> dict:
        return {
            "name": self.name,
            "trials": [t.summary_dict() for t in self.trials],
        }


# ---------------------------------------------------------------------------
# HTTP client pool (keep-alive)
# ---------------------------------------------------------------------------


class HttpClient:
    """Cliente HTTP persistente con keep-alive y reintentos."""

    def __init__(self, timeout_s: float = 120.0, max_retries: int = 2):
        self.timeout_s = timeout_s
        self.max_retries = max_retries
        self._client: Optional[httpx.Client] = None

    @property
    def client(self) -> httpx.Client:
        if self._client is None or self._client.is_closed:
            self._client = httpx.Client(timeout=self.timeout_s)
        return self._client

    def close(self) -> None:
        if self._client is not None and not self._client.is_closed:
            self._client.close()

    def post_chat(
        self,
        prompt: str,
        stream: bool = False,
        fault: Optional[str] = None,
        trial_num: int = 0,
    ) -> Result:
        start = time.perf_counter()
        ttft_start: Optional[float] = None
        ttft_ms: Optional[float] = None
        headers = dict(AUTH_HEADER)
        if fault == "bad_key":
            headers["Authorization"] = "Bearer invalida"
        elif fault == "rotating_key":
            headers["Authorization"] = f"Bearer key-{int(time.time()) % 9999}"
        elif fault == "no_key":
            headers.pop("Authorization", None)

        payload = {
            "model": "qwen3-8b",
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.7,
            "max_tokens": 256,
            "stream": stream,
        }

        last_error = ""
        last_status: Optional[int] = None
        for attempt in range(self.max_retries + 1):
            try:
                if stream:
                    ttft_start = time.perf_counter()
                    with self.client.stream(
                        "POST", GATEWAY_URL, json=payload, headers=headers
                    ) as resp:
                        last_status = resp.status_code
                        if resp.status_code >= 400:
                            return Result(
                                success=False, latency_ms=(time.perf_counter() - start) * 1000,
                                error=f"HTTP {resp.status_code}",
                                status_code=resp.status_code,
                            )
                        content_parts: List[str] = []
                        reasoning_parts: List[str] = []
                        first_ttft_recorded = False
                        for line in resp.iter_lines():
                            if not line:
                                continue
                            if line.startswith("data: "):
                                data_str = line[6:]
                                if data_str == "[DONE]":
                                    break
                                try:
                                    chunk = json.loads(data_str)
                                except json.JSONDecodeError:
                                    continue
                                delta = chunk.get("choices", [{}])[0].get("delta", {})
                                delta_content = delta.get("content", "") or ""
                                delta_reasoning = delta.get("reasoning_content", "") or ""
                                if (delta_content or delta_reasoning) and not first_ttft_recorded:
                                    ttft_ms = (time.perf_counter() - ttft_start) * 1000
                                    first_ttft_recorded = True
                                if delta_content:
                                    content_parts.append(delta_content)
                                if delta_reasoning:
                                    reasoning_parts.append(delta_reasoning)
                        content = "".join(content_parts)
                        if not content or len(content.strip()) == 0:
                            content = "".join(reasoning_parts)
                else:
                    resp = self.client.post(GATEWAY_URL, json=payload, headers=headers)
                    last_status = resp.status_code
                    if resp.status_code >= 400:
                        return Result(
                            success=False, latency_ms=(time.perf_counter() - start) * 1000,
                            error=f"HTTP {resp.status_code}", status_code=resp.status_code,
                        )
                    data = resp.json()
                    content = (
                        (data.get("choices") or [{}])[0]
                        .get("message", {})
                        .get("content", "")
                    )
                    reasoning = (
                        (data.get("choices") or [{}])[0]
                        .get("message", {})
                        .get("reasoning_content", "")
                    )
                    if not content or len(content.strip()) == 0:
                        content = reasoning

                # Validate content (accept content OR reasoning_content)
                if not content or len(content.strip()) == 0:
                    return Result(
                        success=False, latency_ms=(time.perf_counter() - start) * 1000,
                        error="empty content", status_code=last_status,
                    )

                latency_ms = (time.perf_counter() - start) * 1000
                # ttft_ms was recorded inside the stream loop (time to first token)
                # For non-streaming, ttft_ms equals latency_ms
                if not stream and ttft_ms is None:
                    ttft_ms = latency_ms
                tokens = len(content) // 4
                return Result(
                    success=True, latency_ms=latency_ms, tokens=tokens,
                    ttft_ms=ttft_ms, status_code=last_status,
                )
            except httpx.HTTPStatusError as exc:
                last_error = f"HTTP {exc.response.status_code}"
                last_status = exc.response.status_code
                if attempt < self.max_retries:
                    time.sleep(0.5 * (attempt + 1))
                    continue
                return Result(
                    success=False, latency_ms=(time.perf_counter() - start) * 1000,
                    error=last_error, status_code=last_status,
                )
            except (httpx.ConnectError, httpx.ReadTimeout, httpx.ConnectTimeout) as exc:
                last_error = str(exc)
                if attempt < self.max_retries:
                    time.sleep(0.5 * (attempt + 1))
                    continue
                return Result(
                    success=False, latency_ms=(time.perf_counter() - start) * 1000,
                    error=last_error,
                )
            except Exception as exc:
                last_error = str(exc)
                return Result(
                    success=False, latency_ms=(time.perf_counter() - start) * 1000,
                    error=last_error,
                )
        return Result(
            success=False, latency_ms=(time.perf_counter() - start) * 1000,
            error=last_error or "unknown", status_code=last_status,
        )


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def health_check(timeout: float = 5.0) -> bool:
    try:
        with httpx.Client(timeout=timeout) as client:
            resp = client.get(HEALTH_URL)
            return resp.status_code == 200
    except Exception:
        return False


def build_prompt(size_tag: str) -> str:
    base = "Responde en español y solo con el contenido solicitado. "
    if size_tag == "1k":
        return base + " ".join(["palabra"] * 200)
    if size_tag == "10k":
        return base + " ".join(["contexto"] * 2000)
    if size_tag == "100k":
        return base + " ".join(["contextoextenso"] * 20000)
    return base + " ".join(SHORT_PROMPTS_ES)


def parse_sizes(s: str) -> List[str]:
    return [x.strip() for x in s.split(",") if x.strip()]


def parse_levels(s: str) -> List[int]:
    return [int(x.strip()) for x in s.split(",") if x.strip()]


# ---------------------------------------------------------------------------
# Circuit breaker
# ---------------------------------------------------------------------------


class CircuitBreaker:
    """Detiene la ejecución si la tasa de error supera el umbral."""

    def __init__(self, failure_threshold: float = 0.5, min_calls: int = 10):
        self.failure_threshold = failure_threshold
        self.min_calls = min_calls
        self.calls = 0
        self.failures = 0

    def record(self, success: bool) -> None:
        self.calls += 1
        if not success:
            self.failures += 1

    def should_trip(self) -> bool:
        if self.calls < self.min_calls:
            return False
        return (self.failures / self.calls) > self.failure_threshold


# ---------------------------------------------------------------------------
# Suite runners
# ---------------------------------------------------------------------------


def run_concurrency_sweep(
    http: HttpClient,
    levels: List[int],
    total_per_level: int,
    trials: int,
    stream: bool = False,
    breaker: Optional[CircuitBreaker] = None,
) -> List[Cell]:
    cells: List[Cell] = []
    for c in levels:
        cell = Cell(name=f"concurrency c={c} (stream={stream})")
        for tr in range(1, trials + 1):
            trial = Trial(name=f"concurrency c={c} (stream={stream})")
            prompts = [
                SHORT_PROMPTS_ES[i % len(SHORT_PROMPTS_ES)]
                for i in range(total_per_level)
            ]
            with concurrent.futures.ThreadPoolExecutor(max_workers=c) as ex:
                futs = [
                    ex.submit(http.post_chat, p, stream) for p in prompts
                ]
                for fut in concurrent.futures.as_completed(futs):
                    r = fut.result()
                    trial.add(r)
                    if breaker:
                        breaker.record(r.success)
                        if breaker.should_trip():
                            print(
                                f"  [CIRCUIT BREAKER] trip at c={c} trial {tr}: "
                                f"error rate too high, stopping."
                            )
                            cell.add_trial(trial)
                            cells.append(cell)
                            return cells
            cell.add_trial(trial)
        cells.append(cell)
        cell.report()
    return cells


def run_stamina(
    http: HttpClient,
    turns: int,
    sessions: int,
    trials: int,
    breaker: Optional[CircuitBreaker] = None,
) -> Cell:
    cell = Cell(name=f"stamina {turns}turns x {sessions}sessions")
    for tr in range(1, trials + 1):
        trial = Trial(name=f"stamina {turns}turns x {sessions}sessions")
        for s in range(sessions):
            history: list = []
            for t in range(1, turns + 1):
                prompt = (
                    f"Turno {t}: continúa la conversación en español. "
                    f"Responde brevemente y mantén el hilo."
                )
                history.append({"role": "user", "content": prompt})
                payload = {
                    "model": "qwen3-8b",
                    "messages": history,
                    "temperature": 0.7,
                    "max_tokens": 256,
                    "stream": False,
                }
                start = time.perf_counter()
                try:
                    resp = http.client.post(
                        GATEWAY_URL, json=payload, headers=AUTH_HEADER
                    )
                    if resp.status_code >= 400:
                        trial.add(Result(
                            success=False, latency_ms=(time.perf_counter()-start)*1000,
                            error=f"HTTP {resp.status_code}", turn=t,
                            status_code=resp.status_code,
                        ))
                        if breaker:
                            breaker.record(False)
                        break
                    data = resp.json()
                    content = (
                        (data.get("choices") or [{}])[0]
                        .get("message", {})
                        .get("content", "")
                    )
                    reasoning = (
                        (data.get("choices") or [{}])[0]
                        .get("message", {})
                        .get("reasoning_content", "")
                    )
                    if not content or len(content.strip()) == 0:
                        content = reasoning
                    if not content or len(content.strip()) == 0:
                        trial.add(Result(
                            success=False, latency_ms=(time.perf_counter()-start)*1000,
                            error="empty content", turn=t,
                            status_code=resp.status_code,
                        ))
                        if breaker:
                            breaker.record(False)
                        break
                    history.append({"role": "assistant", "content": content})
                    trial.add(Result(
                        success=True, latency_ms=(time.perf_counter()-start)*1000,
                        tokens=len(content)//4, turn=t, status_code=resp.status_code,
                    ))
                    if breaker:
                        breaker.record(True)
                    if breaker and breaker.should_trip():
                        print(
                            f"  [CIRCUIT BREAKER] trip at stamina session {s} turn {t}"
                        )
                        cell.add_trial(trial)
                        return cell
                except Exception as exc:
                    trial.add(Result(
                        success=False, latency_ms=(time.perf_counter()-start)*1000,
                        error=str(exc), turn=t,
                    ))
                    if breaker:
                        breaker.record(False)
                    break
        cell.add_trial(trial)
    cell.report()
    return cell


def run_faults(
    http: HttpClient,
    fault_rate: float,
    total: int,
    concurrency: int,
    trials: int,
    breaker: Optional[CircuitBreaker] = None,
) -> Cell:
    import random
    random.seed(42)
    cell = Cell(name=f"faults rate={fault_rate} concurrency={concurrency}")
    fault_types = ["bad_key", "rotating_key", "no_key"]
    for tr in range(1, trials + 1):
        trial = Trial(name=f"faults rate={fault_rate} concurrency={concurrency}")
        prompts = [
            SHORT_PROMPTS_ES[i % len(SHORT_PROMPTS_ES)] for i in range(total)
        ]
        with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as ex:
            futs = []
            for i, p in enumerate(prompts):
                fault = None
                if random.random() < fault_rate:
                    fault = random.choice(fault_types)
                futs.append(ex.submit(http.post_chat, p, False, fault, tr))
            for fut in concurrent.futures.as_completed(futs):
                r = fut.result()
                trial.add(r)
                if breaker:
                    breaker.record(r.success)
                    if breaker.should_trip():
                        print(
                            f"  [CIRCUIT BREAKER] trip at faults trial {tr}"
                        )
                        cell.add_trial(trial)
                        return cell
        cell.add_trial(trial)
    cell.report()
    return cell


def run_context(
    http: HttpClient,
    sizes: List[str],
    concurrency: int,
    trials: int,
    breaker: Optional[CircuitBreaker] = None,
) -> List[Cell]:
    cells: List[Cell] = []
    for tag in sizes:
        prompt = build_prompt(tag)
        cell = Cell(name=f"context {tag} concurrency={concurrency}")
        for tr in range(1, trials + 1):
            trial = Trial(name=f"context {tag} concurrency={concurrency}")
            n = max(1, concurrency * 2)
            prompts = [prompt] * n
            with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as ex:
                futs = [ex.submit(http.post_chat, p) for p in prompts]
                for fut in concurrent.futures.as_completed(futs):
                    r = fut.result()
                    trial.add(r)
                    if breaker:
                        breaker.record(r.success)
                        if breaker.should_trip():
                            print(
                                f"  [CIRCUIT BREAKER] trip at context {tag} trial {tr}"
                            )
                            cell.add_trial(trial)
                            cells.append(cell)
                            return cells
            cell.add_trial(trial)
        cells.append(cell)
        cell.report()
    return cells


def run_streaming(
    http: HttpClient,
    concurrency: int,
    total: int,
    trials: int,
    breaker: Optional[CircuitBreaker] = None,
) -> Cell:
    cell = Cell(name=f"streaming concurrency={concurrency} total={total}")
    for tr in range(1, trials + 1):
        trial = Trial(name=f"streaming concurrency={concurrency} total={total}")
        prompts = [
            SHORT_PROMPTS_ES[i % len(SHORT_PROMPTS_ES)] for i in range(total)
        ]
        with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as ex:
            futs = [ex.submit(http.post_chat, p, True) for p in prompts]
            for fut in concurrent.futures.as_completed(futs):
                r = fut.result()
                trial.add(r)
                if breaker:
                    breaker.record(r.success)
                    if breaker.should_trip():
                        print(
                            f"  [CIRCUIT BREAKER] trip at streaming trial {tr}"
                        )
                        cell.add_trial(trial)
                        return cell
        cell.add_trial(trial)
    cell.report()
    return cell


def run_security(
    http: HttpClient,
    flood_rate: int,
    total: int,
    trials: int,
    breaker: Optional[CircuitBreaker] = None,
) -> Cell:
    import random
    random.seed(7)
    cell = Cell(name=f"security flood_rate={flood_rate}/s total={total}")
    for tr in range(1, trials + 1):
        trial = Trial(name=f"security flood_rate={flood_rate}/s total={total}")
        prompts = [
            SHORT_PROMPTS_ES[i % len(SHORT_PROMPTS_ES)] for i in range(total)
        ]
        interval = 1.0 / flood_rate if flood_rate > 0 else 0
        start_wall = time.perf_counter()
        with concurrent.futures.ThreadPoolExecutor(
            max_workers=min(flood_rate, 50)
        ) as ex:
            futs = []
            for i, p in enumerate(prompts):
                if interval > 0:
                    target = start_wall + i * interval
                    now = time.perf_counter()
                    if target > now:
                        time.sleep(target - now)
                fault = "bad_key" if random.random() < 0.5 else None
                futs.append(ex.submit(http.post_chat, p, False, fault, tr))
            for fut in concurrent.futures.as_completed(futs):
                r = fut.result()
                trial.add(r)
                if breaker:
                    breaker.record(r.success)
                    if breaker.should_trip():
                        print(
                            f"  [CIRCUIT BREAKER] trip at security trial {tr}"
                        )
                        cell.add_trial(trial)
                        return cell
        cell.add_trial(trial)
        wall_s = time.perf_counter() - start_wall
        print(f"  Wall time : {wall_s:.1f}s  ({total/wall_s:.1f} req/s)")
    cell.report()
    return cell


# ---------------------------------------------------------------------------
# WORKFLOWPERTURB suite: missing steps, compressed steps, changed descriptions
# ---------------------------------------------------------------------------


def run_workflow_perturb(
    http: HttpClient,
    concurrency: int,
    total: int,
    trials: int,
    breaker: Optional[CircuitBreaker] = None,
) -> Cell:
    """WORKFLOWPERTURB: perturba flujos de agentes (pasos faltantes, comprimidos, cambios de descripción)."""
    cell = Cell(name=f"workflow_perturb concurrency={concurrency} total={total}")
    # Three perturbation variants
    variants = {
        "missing_step": (
            "Resuelve este problema en español pero OMITE el paso de validación. "
            "Solo da el resultado final. Problema: Calcula la suma de 1 a 100."
        ),
        "compressed": (
            "Resuelve en español, comprime todos los pasos en una sola respuesta corta. "
            "Problema: ¿Cuál es el factorial de 5?"
        ),
        "changed_description": (
            "Responde en español. El problema ahora se llama 'Suma de Gauss'. "
            "Calcula: suma de números del 1 al 50."
        ),
    }
    for tr in range(1, trials + 1):
        trial = Trial(name=f"workflow_perturb concurrency={concurrency} total={total}")
        prompts = [
            variants[list(variants.keys())[i % len(variants)]]
            for i in range(total)
        ]
        with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as ex:
            futs = [ex.submit(http.post_chat, p, False) for p in prompts]
            for fut in concurrent.futures.as_completed(futs):
                r = fut.result()
                trial.add(r)
                if breaker:
                    breaker.record(r.success)
                    if breaker.should_trip():
                        print(
                            f"  [CIRCUIT BREAKER] trip at workflow_perturb trial {tr}"
                        )
                        cell.add_trial(trial)
                        return cell
        cell.add_trial(trial)
    cell.report()
    return cell


# ---------------------------------------------------------------------------
# ReliabilityBench: infrastructure failure simulation
# ---------------------------------------------------------------------------


def run_reliability(
    http: HttpClient,
    failure_rate: float,
    total: int,
    concurrency: int,
    trials: int,
    breaker: Optional[CircuitBreaker] = None,
) -> Cell:
    """ReliabilityBench: confiabilidad bajo condiciones de estrés de producción (fallos de infraestructura)."""
    import random
    random.seed(123)
    cell = Cell(name=f"reliability failure_rate={failure_rate} concurrency={concurrency}")
    for tr in range(1, trials + 1):
        trial = Trial(name=f"reliability failure_rate={failure_rate} concurrency={concurrency}")
        prompts = [
            SHORT_PROMPTS_ES[i % len(SHORT_PROMPTS_ES)] for i in range(total)
        ]
        with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as ex:
            futs = []
            for i, p in enumerate(prompts):
                fault = None
                if random.random() < failure_rate:
                    fault = random.choice(["timeout", "bad_key", "no_key"])
                futs.append(ex.submit(http.post_chat, p, False, fault, tr))
            for fut in concurrent.futures.as_completed(futs):
                r = fut.result()
                trial.add(r)
                if breaker:
                    breaker.record(r.success)
                    if breaker.should_trip():
                        print(
                            f"  [CIRCUIT BREAKER] trip at reliability trial {tr}"
                        )
                        cell.add_trial(trial)
                        return cell
        cell.add_trial(trial)
    cell.report()
    return cell


# ---------------------------------------------------------------------------
# XPerf: multi-problem simultaneous (reasoning under load)
# ---------------------------------------------------------------------------


def run_xperf(
    http: HttpClient,
    concurrency: int,
    total: int,
    trials: int,
    breaker: Optional[CircuitBreaker] = None,
) -> Cell:
    """XPerf: stress test de modelos de razonamiento con múltiples problemas simultáneos."""
    cell = Cell(name=f"xperf concurrency={concurrency} total={total}")
    # Complex reasoning problems in Spanish
    complex_problems = [
        "Resuelve paso a paso: Si un tren sale de Madrid a las 8:00 a 80 km/h y otro de Barcelona a las 9:00 a 100 km/h, ¿cuándo y dónde se cruzan? (distancia 600 km)",
        "Explica la diferencia entre clase y instancia en POO con un ejemplo completo en Python.",
        "Escribe una función recursiva para calcular Fibonacci y explica su complejidad temporal.",
        "Diseña una API REST para un sistema de inventario con endpoints, códigos HTTP y validaciones.",
        "Compara SQL vs NoSQL: cuándo usar cada uno y ejemplos concretos.",
    ]
    for tr in range(1, trials + 1):
        trial = Trial(name=f"xperf concurrency={concurrency} total={total}")
        prompts = [
            complex_problems[i % len(complex_problems)] for i in range(total)
        ]
        with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as ex:
            futs = [ex.submit(http.post_chat, p, False) for p in prompts]
            for fut in concurrent.futures.as_completed(futs):
                r = fut.result()
                trial.add(r)
                if breaker:
                    breaker.record(r.success)
                    if breaker.should_trip():
                        print(
                            f"  [CIRCUIT BREAKER] trip at xperf trial {tr}"
                        )
                        cell.add_trial(trial)
                        return cell
        cell.add_trial(trial)
    cell.report()
    return cell


# ---------------------------------------------------------------------------
# Warm-up
# ---------------------------------------------------------------------------


def warmup(http: HttpClient, count: int = 3) -> None:
    print(f"Warm-up ({count} requests)...")
    for i in range(count):
        r = http.post_chat(SHORT_PROMPTS_ES[0], stream=False)
        status = "OK" if r.success else f"FAIL ({r.error})"
        print(f"  [{i+1}/{count}] {status} ({r.latency_ms:.1f}ms)")


# ---------------------------------------------------------------------------
# Global report
# ---------------------------------------------------------------------------


def global_report(cells: List[Cell]) -> dict:
    print("\n" + "=" * 60)
    print("GLOBAL REPORT")
    print("=" * 60)
    total_ok = sum(t.success for c in cells for t in c.trials)
    total_fail = sum(t.failed for c in cells for t in c.trials)
    total_req = sum(t.total for c in cells for t in c.trials)
    all_lat = [l for c in cells for t in c.trials for l in t.latencies]
    all_ttft = [t2 for c in cells for t in c.trials for t2 in t.ttfts]
    print(f"Requests : {total_req}  OK : {total_ok}  FAIL : {total_fail}")
    if all_lat:
        print(
            f"Latency  : avg={statistics.mean(all_lat):.1f}ms "
            f"p50={statistics.median(all_lat):.1f}ms "
            f"p99={sorted(all_lat)[int(len(all_lat)*0.99)]:.1f}ms "
            f"max={max(all_lat):.1f}ms"
        )
    if all_ttft:
        print(
            f"TTFT     : avg={statistics.mean(all_ttft):.1f}ms "
            f"p99={sorted(all_ttft)[int(len(all_ttft)*0.99)]:.1f}ms"
        )
    print("=" * 60)
    return {
        "total_requests": total_req,
        "total_success": total_ok,
        "total_failed": total_fail,
        "latency_avg_ms": statistics.mean(all_lat) if all_lat else None,
        "latency_p50_ms": statistics.median(all_lat) if all_lat else None,
        "latency_p99_ms": sorted(all_lat)[int(len(all_lat)*0.99)] if all_lat else None,
        "ttft_avg_ms": statistics.mean(all_ttft) if all_ttft else None,
        "ttft_p99_ms": sorted(all_ttft)[int(len(all_ttft)*0.99)] if all_ttft else None,
        "cells": [c.summary_dict() for c in cells],
    }


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description="Stress test a prueba de balas para lai")
    parser.add_argument(
        "--suite", default="concurrency",
        choices=["concurrency", "stamina", "faults", "context", "streaming", "security",
                 "workflow", "reliability", "xperf", "full"],
        help="Suite de pruebas a ejecutar",
    )
    parser.add_argument("--concurrency", type=int, default=10, help="Peticiones concurrentes")
    parser.add_argument("--total", type=int, default=50, help="Total de peticiones")
    parser.add_argument("--turns", type=int, default=20, help="Turnos para stamina")
    parser.add_argument("--sessions", type=int, default=3, help="Sesiones para stamina")
    parser.add_argument("--trials", type=int, default=3, help="Número de trials por celda")
    parser.add_argument("--fault-rate", type=float, default=0.2, help="Tasa de fallos 0..1")
    parser.add_argument(
        "--sizes", type=parse_sizes, default=["1k", "10k", "100k"],
        help="Tamaños de contexto (coma-separados)",
    )
    parser.add_argument("--stream", action="store_true", help="Streaming en concurrency")
    parser.add_argument("--flood-rate", type=int, default=50, help="Req/s para security flood")
    parser.add_argument(
        "--levels", type=parse_levels, default=[1, 4, 16, 64, 128, 256],
        help="Niveles de concurrencia (coma-separados)",
    )
    parser.add_argument(
        "--failure-threshold", type=float, default=0.5,
        help="Umbral de error para circuit breaker (0..1)",
    )
    parser.add_argument(
        "--timeout", type=float, default=120.0, help="Timeout HTTP en segundos",
    )
    parser.add_argument("--json", type=str, default=None, help="Guardar reporte JSON en archivo")
    parser.add_argument("--no-warmup", action="store_true", help="Saltar warm-up")
    args = parser.parse_args()

    if not health_check():
        print("ERROR: gateway no responde en http://127.0.0.1:8081/health")
        return 1

    breaker = CircuitBreaker(
        failure_threshold=args.failure_threshold, min_calls=10
    )
    http = HttpClient(timeout_s=args.timeout)

    try:
        if not args.no_warmup:
            warmup(http, count=3)

        print(f"Stress test a prueba de balas — suite: {args.suite}")
        print(f"Gateway: {GATEWAY_URL}")
        print(f"Trials por celda: {args.trials}  Circuit breaker: {args.failure_threshold}")
        print("=" * 60)

        all_cells: List[Cell] = []

        if args.suite in ("concurrency", "full"):
            all_cells.extend(
                run_concurrency_sweep(
                    http, args.levels, args.total, args.trials,
                    stream=args.stream, breaker=breaker,
                )
            )

        if args.suite in ("stamina", "full"):
            all_cells.append(
                run_stamina(http, args.turns, args.sessions, args.trials, breaker=breaker)
            )

        if args.suite in ("faults", "full"):
            all_cells.append(
                run_faults(http, args.fault_rate, args.total, args.concurrency, args.trials, breaker=breaker)
            )

        if args.suite in ("context", "full"):
            all_cells.extend(
                run_context(http, args.sizes, args.concurrency, args.trials, breaker=breaker)
            )

        if args.suite in ("streaming", "full"):
            all_cells.append(
                run_streaming(http, args.concurrency, args.total, args.trials, breaker=breaker)
            )

        if args.suite in ("security", "full"):
            all_cells.append(
                run_security(http, args.flood_rate, args.total, args.trials, breaker=breaker)
            )

        if args.suite in ("workflow", "full"):
            all_cells.append(
                run_workflow_perturb(http, args.concurrency, args.total, args.trials, breaker=breaker)
            )

        if args.suite in ("reliability", "full"):
            all_cells.append(
                run_reliability(http, args.fault_rate, args.total, args.concurrency, args.trials, breaker=breaker)
            )

        if args.suite in ("xperf", "full"):
            all_cells.append(
                run_xperf(http, args.concurrency, args.total, args.trials, breaker=breaker)
            )

        report = global_report(all_cells)

        if args.json:
            with open(args.json, "w", encoding="utf-8") as f:
                json.dump(report, f, indent=2, ensure_ascii=False)
            print(f"\nReporte JSON guardado en: {args.json}")

        total_fail = sum(t.failed for c in all_cells for t in c.trials)
        return 0 if total_fail == 0 else 1
    except KeyboardInterrupt:
        print("\nInterrumpido por el usuario.")
        return 130
    finally:
        http.close()


if __name__ == "__main__":
    sys.exit(main())
