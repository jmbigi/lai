"""AutoMode - Agente autónomo con llamada nativa de herramientas (llama.cpp).

Mejora de arquitectura e inteligencia sobre la versión inline de lai.py:

- Plan-and-Execute: el planner descompone la meta en subtareas con criterios
  de aceptación claros (tareas pequeñas, criterios verificables).
- ReAct (Reason + Act): bucle explícito Thought → Action → Observation con
  traza de razonamiento visible.
- Reflection: bucle generate → critique → revise usando un crítico separado
  (sub-agente "critic") que evita el sesgo de autoevaluación.
- Crítico separado: el agente "review"/"critic" de lai_agents evalúa cada
  salida contra una rubrica concreta, no el propio generador.
- Best-version fallback: si el agente no converge antes del límite, se
  devuelve la mejor versión puntuada por el crítico, no la última.
- Mejor manejo de errores con reintentos inteligentes.
- Mejor tracking de progreso, estado, tokens y coste.

100% local, sin cloud, sin API keys. Regla 0 de lai.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Dict, List, Optional

if TYPE_CHECKING:
    from lai import LAI
    from lai_tools import ToolRegistry
    from scripts.lai_agents import MultiAgentOrchestrator


@dataclass
class PlanStep:
    """Paso de un plan de ejecución con criterios de aceptación."""

    id: str
    description: str
    criteria: List[str]
    status: str = "pending"  # pending, in_progress, done, failed
    result: Optional[str] = None
    score: Optional[float] = None
    feedback: Optional[str] = None


@dataclass
class ReActTrace:
    """Un paso del bucle ReAct (Reason + Act)."""

    thought: str
    action: Optional[str] = None
    action_input: Optional[Dict[str, Any]] = None
    observation: Optional[str] = None


@dataclass
class CritiqueResult:
    """Resultado de la evaluación del crítico separado."""

    approved: bool
    score: float
    verdict: str  # APROBADO / RECHAZADO / PARCIAL
    feedback: str
    raw: str


class AutoMode:
    """Agente autónomo con bucle de llamada nativa de herramientas.

    Características de inteligencia:
    - Usa llama.cpp native function calling (no heurísticas de keyword).
    - Plan-and-Execute: descompone la meta en subtareas con criterios claros.
    - ReAct: razonamiento explícito entre cada acción de herramienta.
    - Reflection: crítico separado evalúa y pide revisión hasta aprobar.
    - Best-version fallback: devuelve la mejor versión si no converge.
    - Gestión de turnos y coste con límites configurables.
    """

    PLANNER_PROMPT = (
        "/no_think\n"
        "Eres un planificador de tareas para un agente de programación autónomo.\n"
        "OBJETIVO: Descomponer la meta del usuario en subtareas pequeñas y "
        "verificables, cada una con criterios de aceptación concretos.\n"
        "\n"
        "REGLAS:\n"
        "1. Cada subtarea debe ser pequeña (una acción o un archivo).\n"
        "2. Los criterios de aceptación deben ser verificables (compila, pasa "
        "tests, existe el archivo, etc.).\n"
        "3. Devuelve SOLO una lista numerada con este formato:\n"
        "   1. <descripción> | Criterios: <c1>; <c2>\n"
        "4. No escribas código, solo planifica.\n"
        "5. Todo el código debe generarse con modelos locales (Regla 4).\n"
    )

    EXECUTOR_PROMPT = (
        "/no_think\n"
        "Eres un ejecutor de tareas autónomo con acceso a herramientas.\n"
        "OBJETIVO: Cumplir la subtarea asignada usando las herramientas "
        "disponibles.\n"
        "\n"
        "MÉTODO ReAct (Reason + Act):\n"
        "Para cada paso, piensa primero (Thought), luego decide una acción\n"
        "(Action con nombre de herramienta y argumentos), espera la observación\n"
        "y repite hasta resolver la subtarea.\n"
        "\n"
        "REGLAS DE ORO:\n"
        "1. Inspecciona los archivos antes de editarlos (read_file, grep, "
        "list_dir).\n"
        "2. Usa write_file con el CONTENIDO COMPLETO del archivo, nunca "
        "fragmentos.\n"
        "3. Los archivos Python se verifican sintácticamente automáticamente.\n"
        "4. Si una herramienta falla, analiza el error y reintenta con "
        "corrección.\n"
        "5. Cuando la subtarea esté completa, responde sin llamar a ninguna "
        "herramienta.\n"
        "6. Todo el código debe generarse usando modelos locales (Regla 4).\n"
    )

    CRITIQUE_PROMPT_HINT = (
        "Usa únicamente evidencia concreta (líneas, fragmentos, salida de "
        "herramientas) para fundamentar tu veredicto."
    )

    def __init__(
        self,
        goal: str,
        tools: ToolRegistry,
        lai_instance: LAI,
        max_iterations: int = 10,
        max_turns: int = 20,
        track_cost: bool = True,
        reflective: bool = True,
        max_reflection_rounds: int = 3,
        orchestrator: Optional[MultiAgentOrchestrator] = None,
    ):
        self.goal = goal
        self.tools = tools
        self.lai = lai_instance
        self.max_iterations = max_iterations
        self.max_turns = max_turns
        self.track_cost = track_cost
        self.reflective = reflective
        self.max_reflection_rounds = max_reflection_rounds
        self.orchestrator = orchestrator

        self.iteration = 0
        self.turns_used = 0
        self.total_prompt_tokens = 0
        self.total_completion_tokens = 0
        self.messages: List[Dict] = [
            {"role": "system", "content": self.EXECUTOR_PROMPT},
            {"role": "user", "content": goal},
        ]
        self.completed = False
        self.error_history: List[str] = []
        self.plan: List[PlanStep] = []
        self.transcript: List[str] = []
        self.best_outputs: Dict[str, tuple[float, str]] = {}
        # Mejor resultado global (puntuación, contenido)
        self.best_global: Optional[tuple[float, str]] = None

    # ------------------------------------------------------------------
    # Orquestación principal
    # ------------------------------------------------------------------

    def run(self) -> str:
        """Ejecuta el agente autónomo con planificación, ReAct y reflexión."""
        self.transcript = []
        self._log("╔═══════════════════════════════════════════════════════════════╗")
        self._log("║  lai AutoMode - Agente autónomo (Plan + ReAct + Reflection)   ║")
        self._log("╚═══════════════════════════════════════════════════════════════╝")
        self._log(f"\nMeta: {self.goal}\n")

        if not self._is_local_endpoint():
            self._log("Error: AutoMode requiere un endpoint de modelo local (Regla 4).")
            return "\n".join(self.transcript)

        if self.orchestrator is None:
            from scripts.lai_agents import get_orchestrator

            self.orchestrator = get_orchestrator(
                tools=self.tools,
                model=getattr(self.lai, "model", "qwen3-8b"),
                base_url=getattr(self.lai, "base_url", "http://localhost:8080/v1"),
                api_key=getattr(self.lai, "api_key", None),
                allow_remote=getattr(self.lai, "allow_remote", False),
            )

        # 1. Plan-and-Execute: descomponer la meta en subtareas
        self._log("\n=== FASE 1: Planificación (Plan-and-Execute) ===")
        self.plan = self._plan_goal()
        if not self.plan:
            self._log("No se pudo generar un plan. Ejecutando en modo directo (una sola tarea).")
            self.plan = [
                PlanStep(
                    id="step-1",
                    description=self.goal,
                    criteria=["La tarea del usuario queda resuelta"],
                )
            ]
        self._log_plan()

        # 2. Ejecutar cada paso del plan con ReAct + Reflection
        for step in self.plan:
            if self.turns_used >= self.max_turns:
                self._log(f"\nMáximo de turnos ({self.max_turns}) alcanzado. Deteniendo plan.")
                break

            step.status = "in_progress"
            self._log(f"\n--- Paso {step.id}: {step.description} ---")
            self._log(f"Criterios de aceptación: {step.criteria}")

            # 2a. Ejecutar con bucle ReAct
            step_result = self._execute_step_react(step)

            # 2b. Reflection: crítico separado evalúa y pide revisión
            best_result = step_result
            best_score = -1.0
            if self.reflective and self.orchestrator is not None:
                for round_num in range(1, self.max_reflection_rounds + 1):
                    self._log(f"\n[Reflection] Ronda {round_num} de crítica")
                    critique = self._critique_step(step, step_result)
                    step.score = critique.score
                    step.feedback = critique.feedback

                    if critique.approved:
                        self._log(f"[Reflection] APROBADO (puntuación {critique.score})")
                        best_result = step_result
                        best_score = critique.score
                        break

                    self._log(
                        f"[Reflection] RECHAZADO/PARCIAL "
                        f"(puntuación {critique.score}): {critique.feedback[:200]}"
                    )

                    # Guardar como candidata a best version
                    if critique.score > best_score:
                        best_score = critique.score
                        best_result = step_result

                    # 2c. Revisar: re-ejecutar con el feedback del crítico
                    self._log("[Reflection] Re-ejecutando con feedback del crítico...")
                    step_result = self._revise_step(step, step_result, critique.feedback)
            else:
                step.score = None

            # 2d. Best-version fallback: guardar la mejor versión del paso
            if best_score > 0:
                self.best_outputs[step.id] = (best_score, best_result)
            step.result = best_result
            step.status = "done"
            self._log(f"[Paso {step.id}] Completado. Resultado:\n{best_result[:500]}")

        # 3. Integrar resultados y generar respuesta final
        self._log("\n=== FASE 3: Integración y respuesta final ===")
        final_output = self._integrate_results()

        # 4. Best-version fallback global: si nada se completó bien,
        # devolver la mejor versión puntuada
        if not self.completed and self.best_global is not None:
            self._log(
                "\n[Best-version fallback] No se alcanzó completitud. "
                "Devolviendo la mejor versión puntuada."
            )
            return self.best_global[1]

        self._log_cost_summary()
        return final_output

    # ------------------------------------------------------------------
    # Fase 1: Planificación
    # ------------------------------------------------------------------

    def _plan_goal(self) -> List[PlanStep]:
        """Usa el modelo para descomponer la meta en pasos con criterios."""
        messages = [
            {"role": "system", "content": self.PLANNER_PROMPT},
            {"role": "user", "content": f"Meta: {self.goal}"},
        ]
        response = self._chat_plain(messages)
        if response.get("error"):
            self._log(f"Error en planificación: {response['error']}")
            return []

        content = response.get("content") or ""
        return self._parse_plan(content)

    def _parse_plan(self, content: str) -> List[PlanStep]:
        """Parsea la salida del planner en una lista de PlanStep."""
        steps: List[PlanStep] = []
        # Patrón: "1. desc | Criterios: c1; c2" o variantes
        pattern = re.compile(r"^\s*\d+\.\s*(.+?)\s*\|\s*Criterios:\s*(.+?)\s*$", re.MULTILINE)
        for idx, m in enumerate(pattern.finditer(content), 1):
            description = m.group(1).strip()
            criteria_raw = m.group(2).strip()
            criteria = [c.strip() for c in criteria_raw.split(";") if c.strip()]
            steps.append(
                PlanStep(
                    id=f"step-{idx}",
                    description=description,
                    criteria=criteria,
                )
            )

        # Fallback: si no coincidió el patrón, intentar líneas simples
        if not steps:
            for idx, line in enumerate(content.splitlines(), 1):
                line = line.strip()
                if not line or line[0].isdigit() is False:
                    continue
                # Quitar numeración inicial
                desc = re.sub(r"^\d+\.\s*", "", line).strip()
                if desc:
                    steps.append(
                        PlanStep(
                            id=f"step-{idx}",
                            description=desc,
                            criteria=["La subtarea queda resuelta"],
                        )
                    )
        return steps

    def _log_plan(self) -> None:
        """Registra el plan generado en el transcript."""
        for step in self.plan:
            self._log(
                f"  {step.id}. {step.description}"
                + (f" | Criterios: {'; '.join(step.criteria)}" if step.criteria else "")
            )

    # ------------------------------------------------------------------
    # Fase 2: Ejecución (ReAct + Reflection)
    # ------------------------------------------------------------------

    def _execute_step_react(self, step: PlanStep) -> str:
        """Ejecuta una subtarea usando el bucle ReAct (Reason + Act)."""
        messages: List[Dict] = [
            {"role": "system", "content": self.EXECUTOR_PROMPT},
            {
                "role": "user",
                "content": (
                    f"Subtarea: {step.description}\n"
                    f"Criterios de aceptación: {'; '.join(step.criteria)}\n"
                    "Resuelve la subtarea usando el método ReAct "
                    "(Thought → Action → Observation). "
                    "Cuando termine, responde sin llamar a herramientas."
                ),
            },
        ]

        react_traces: List[ReActTrace] = []
        final_content = ""

        for turn in range(1, self.max_turns + 1):
            if self.turns_used >= self.max_turns:
                self._log(f"  [ReAct] Máximo de turnos alcanzado en paso {step.id}")
                break

            self.turns_used += 1
            response = self._chat_with_tools(messages)
            if response.get("error"):
                self._log(f"  [ReAct] Error: {response['error']}")
                break

            self._track_usage(response)

            content = response.get("content") or ""
            tool_calls = response.get("tool_calls") or []

            # Registrar pensamiento (ReAct: Thought)
            if content.strip():
                trace = ReActTrace(thought=content.strip())
                react_traces.append(trace)
                self._log(f"  [ReAct] Thought: {content.strip()[:200]}")
                final_content = content.strip()

            if not tool_calls:
                # No hay más acciones: el ejecutor considera terminado
                self._log("  [ReAct] Sin más acciones. Contenido final del paso.")
                break

            # Ejecutar cada herramienta (ReAct: Action → Observation)
            for call in tool_calls:
                if self.turns_used >= self.max_turns:
                    break
                function = call.get("function") or {}
                name = function.get("name", "")
                raw_args = function.get("arguments", "{}")
                try:
                    arguments = json.loads(raw_args) if isinstance(raw_args, str) else raw_args
                except json.JSONDecodeError:
                    arguments = {}
                if not isinstance(arguments, dict):
                    arguments = {}

                trace.action = name
                trace.action_input = arguments

                func = self.tools.tools.get(name)
                if func is None:
                    result = f"Herramienta desconocida: {name}"
                else:
                    try:
                        result = func(**arguments)
                    except TypeError as exc:
                        result = f"Argumentos inválidos para {name}: {exc}"
                    except Exception as exc:
                        result = f"Tool {name} falló: {exc}"
                        self.error_history.append(f"{name}: {exc}")

                if not isinstance(result, str):
                    result = json.dumps(result, ensure_ascii=False)
                result = result[:6000]
                trace.observation = result
                self._log(f"  [ReAct] Action: {name}({str(arguments)[:120]})")
                self._log(f"  [ReAct] Observation: {result[:200]}")

                messages.append(
                    {
                        "role": "tool",
                        "tool_call_id": call.get("id") or f"call_{turn}",
                        "name": name,
                        "content": result,
                    }
                )

        return final_content or "(sin resultado)"

    def _critique_step(self, step: PlanStep, output: str) -> CritiqueResult:
        """Hace que el crítico separado evalúe la salida del paso."""
        assert self.orchestrator is not None
        context = (
            f"Meta global: {self.goal}\n"
            f"Subtarea: {step.description}\n"
            f"Criterios de aceptación: {'; '.join(step.criteria)}"
        )
        try:
            result = self.orchestrator.critique(
                output=output,
                criteria=step.criteria,
                context=context,
            )
        except Exception as exc:
            self._log(f"  [Crítico] Error invocando crítico: {exc}")
            return CritiqueResult(
                approved=False,
                score=0.0,
                verdict="RECHAZADO",
                feedback=f"Error del crítico: {exc}",
                raw=str(exc),
            )

        raw = result.output or ""
        return self._parse_critique(raw)

    @staticmethod
    def _parse_critique(raw: str) -> CritiqueResult:
        """Parsea el veredicto estructurado del crítico."""
        verdict = "RECHAZADO"
        approved = False
        score = 0.0
        feedback = raw

        m_verdict = re.search(r"VEREDICTO:\s*(APROBADO|RECHAZADO|PARCIAL)", raw, re.IGNORECASE)
        if m_verdict:
            verdict = m_verdict.group(1).upper()
            approved = verdict == "APROBADO"

        m_score = re.search(r"PUNTUACION:\s*(\d+(?:\.\d+)?)", raw, re.IGNORECASE)
        if m_score:
            score = float(m_score.group(1))

        # Extraer solo la sección FEEDBACK si existe
        m_feedback = re.search(r"FEEDBACK:\s*(.+)", raw, re.IGNORECASE | re.DOTALL)
        if m_feedback:
            feedback = m_feedback.group(1).strip()

        return CritiqueResult(
            approved=approved,
            score=score,
            verdict=verdict,
            feedback=feedback,
            raw=raw,
        )

    def _revise_step(self, step: PlanStep, previous_output: str, feedback: str) -> str:
        """Re-ejecuta la subtarea incorporando el feedback del crítico."""
        messages: List[Dict] = [
            {"role": "system", "content": self.EXECUTOR_PROMPT},
            {
                "role": "user",
                "content": (
                    f"Subtarea: {step.description}\n"
                    f"Criterios de aceptación: {'; '.join(step.criteria)}\n"
                    f"INTENTO ANTERIOR:\n---\n{previous_output}\n---\n"
                    f"FEEDBACK DEL CRÍTICO (debes corregir estos puntos):\n"
                    f"---\n{feedback}\n---\n"
                    "Vuelve a resolver la subtarea aplicando el feedback. "
                    "Usa el método ReAct. Cuando termine, responde sin "
                    "llamar a herramientas."
                ),
            },
        ]

        final_content = ""

        for turn in range(1, self.max_turns + 1):
            if self.turns_used >= self.max_turns:
                break
            self.turns_used += 1

            response = self._chat_with_tools(messages)
            if response.get("error"):
                self._log(f"  [Revisión] Error: {response['error']}")
                break

            self._track_usage(response)

            content = response.get("content") or ""
            tool_calls = response.get("tool_calls") or []

            if content.strip():
                self._log(f"  [Revisión] Thought: {content.strip()[:200]}")
                final_content = content.strip()

            if not tool_calls:
                break

            for call in tool_calls:
                if self.turns_used >= self.max_turns:
                    break
                function = call.get("function") or {}
                name = function.get("name", "")
                raw_args = function.get("arguments", "{}")
                try:
                    arguments = json.loads(raw_args) if isinstance(raw_args, str) else raw_args
                except json.JSONDecodeError:
                    arguments = {}
                if not isinstance(arguments, dict):
                    arguments = {}

                func = self.tools.tools.get(name)
                if func is None:
                    result = f"Herramienta desconocida: {name}"
                else:
                    try:
                        result = func(**arguments)
                    except TypeError as exc:
                        result = f"Argumentos inválidos para {name}: {exc}"
                    except Exception as exc:
                        result = f"Tool {name} falló: {exc}"
                        self.error_history.append(f"{name}: {exc}")

                if not isinstance(result, str):
                    result = json.dumps(result, ensure_ascii=False)
                result = result[:6000]
                self._log(f"  [Revisión] Action: {name} → {result[:200]}")

                messages.append(
                    {
                        "role": "tool",
                        "tool_call_id": call.get("id") or f"rev_call_{turn}",
                        "name": name,
                        "content": result,
                    }
                )

        return final_content or previous_output

    # ------------------------------------------------------------------
    # Fase 3: Integración
    # ------------------------------------------------------------------

    def _integrate_results(self) -> str:
        """Combina los resultados de todos los pasos en una respuesta final."""
        parts: List[str] = []
        parts.append("\nResultado de la ejecución autónoma:")
        parts.append("=" * 60)

        any_done = False
        best_score_overall = -1.0
        best_content_overall = ""

        for step in self.plan:
            any_done = True
            parts.append(f"\n## {step.id}: {step.description}")
            parts.append(f"Estado: {step.status}")
            if step.criteria:
                parts.append(f"Criterios: {'; '.join(step.criteria)}")
            if step.score is not None:
                parts.append(f"Puntuación del crítico: {step.score}")
            if step.feedback:
                parts.append(f"Feedback: {step.feedback[:300]}")
            if step.result:
                parts.append(f"Resultado:\n{step.result}")
            if step.score is not None and step.score > best_score_overall:
                best_score_overall = step.score
                best_content_overall = step.result or ""

        if not any_done:
            parts.append("\nNo se ejecutó ningún paso del plan.")

        # Registrar mejor versión global para fallback
        if best_score_overall > 0:
            self.best_global = (best_score_overall, best_content_overall)

        # Marcar como completada si todos los pasos están done
        self.completed = all(s.status == "done" for s in self.plan) if self.plan else False

        return "\n".join(parts)

    # ------------------------------------------------------------------
    # Utilidades
    # ------------------------------------------------------------------

    def _log(self, message: str) -> None:
        """Añade una línea al transcript."""
        self.transcript.append(message)

    def _chat_with_tools(self, schema: List[Dict]) -> Dict[str, Any]:
        return self.lai.chat_messages(
            self.messages,
            tools=schema,
            tool_choice="auto",
            stream=False,
            thinking=False,
            temp=0.1,
            max_tokens=2048,
        )

    def _chat_plain(self, messages: List[Dict]) -> Dict[str, Any]:
        """Chat sin herramientas (para planner y crítico)."""
        return self.lai.chat_messages(
            messages,
            tools=None,
            tool_choice="none",
            stream=False,
            thinking=False,
            temp=0.3,
            max_tokens=2048,
        )

    def _track_usage(self, response: Dict[str, Any]) -> None:
        usage = response.get("usage")
        if self.track_cost and usage:
            self.total_prompt_tokens += usage.get("prompt_tokens", 0)
            self.total_completion_tokens += usage.get("completion_tokens", 0)

    def _is_local_endpoint(self) -> bool:
        from lai import is_local_url

        return is_local_url(self.lai.base_url)

    def _log_cost_summary(self) -> None:
        if not self.track_cost:
            return
        self._log("\n--- Resumen de Coste ---")
        self._log(f"Prompt tokens: {self.total_prompt_tokens}")
        self._log(f"Completion tokens: {self.total_completion_tokens}")
        self._log(f"Turnos usados: {self.turns_used}/{self.max_turns}")
        self._log(f"Iteraciones: {self.iteration}/{self.max_iterations}")
        self._log(f"Pasos del plan: {len(self.plan)}")
        self._log(f"Completada: {self.completed}")
        if self.error_history:
            self._log(f"Errores: {len(self.error_history)}")

    # ------------------------------------------------------------------
    # Retrocompatibilidad con la API anterior
    # ------------------------------------------------------------------

    def _evaluate_completion(self, content: str) -> Dict[str, Any]:
        """Evalúa si el contenido indica que la tarea está completa.

        Mantener por compatibilidad con código existente que pueda llamar
        a este método directamente. Ahora delega en heurísticas simples
        porque la evaluación real la hace el crítico separado.
        """
        completion_indicators = [
            "completado",
            "completada",
            "finalizado",
            "finalizada",
            "hecho",
            "listo",
            "terminado",
            "task complete",
            "done",
            "finished",
        ]
        content_lower = content.lower()
        for indicator in completion_indicators:
            if indicator in content_lower:
                return {"complete": True, "reason": f"Indicador: {indicator}"}
        return {"complete": False, "reason": "Sin indicador de completitud"}
