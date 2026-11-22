"""Sistema de sub-agentes especializados (multi-agente) para lai.

Cada sub-agente tiene un system prompt especializado y un conjunto de
herramientas relevantes para su dominio. El MultiAgentOrchestrator puede
coordinar múltiples sub-agentes para resolver tareas complejas.

100% local, sin cloud, sin API keys. Regla 0 de lai.
"""

from __future__ import annotations  # noqa: I001

import asyncio
from dataclasses import dataclass, field

import httpx

from lai_tools import ToolRegistry
from rag_local import get_rag

# ---------------------------------------------------------------------------
# System prompts especializados por dominio
# ---------------------------------------------------------------------------

SUBAGENT_PROMPTS: dict[str, str] = {
    "code": """Eres un Agente Especialista en Generación de Código.
Tu misión es escribir código limpio, funcional y bien documentado.
Siempre incluye: type hints, docstrings (estilo Google), manejo de errores,
y bloques if __name__ == "__main__". No uses markdown, solo código Python puro.
Prioriza la legibilidad y el cumplimiento de PEP 8.""",
    "review": """Eres un Agente Especialista en Revisión de Código (Code Review).
Tu misión es analizar código en busca de: bugs, vulnerabilidades de seguridad,
problemas de rendimiento, violaciones de estilo y oportunidades de refactorización.
Proporciona retroalimentación constructiva, específica y accionable.
Clasifica hallazgos por severidad: crítico, alto, medio, bajo.""",
    "test": """Eres un Agente Especialista en Generación de Pruebas.
Tu misión es escribir pruebas unitarias y de integración completas.
Usa pytest, cubre casos edge, pruebas de error y pruebas felices.
Incluye fixtures, mocks cuando sea necesario, y reportes de cobertura.""",
    "doc": """Eres un Agente Especialista en Documentación Técnica.
Tu misión es escribir documentación clara, completa y bien estructurada.
Incluye: docstrings, README, comentarios en el código, guías de usuario,
y documentación de API. Usa Markdown y Google-style docstrings.""",
    "fix": """Eres un Agente Especialista en Corrección de Errores (Debugging).
Tu misión es identificar y corregir bugs en código existente.
Analiza traces de error, reproduce el problema, propone una solución
y verifica que la corrección no introduce regresiones.""",
    "arch": """Eres un Agente Especialista en Arquitectura de Software.
Tu misión es diseñar estructuras de código modulares, escalables y
mantenibles. Propón patrones de diseño, definiendo interfaces claras,
separación de responsabilidades y bajo acoplamiento.""",
    "critic": """Eres un Agente Crítico Especialista en Evaluación de Código.
Tu misión es evaluar si un resultado cumple unos CRITERIOS DE ACEPTACIÓN
concretos y devolver un veredicto estructurado.

Para cada criterio, indica:
- CUMPLE o NO CUMPLE
- Evidencia concreta (línea, fragmento o comportamiento)
- Si no cumple, qué exactamente falta o está mal y cómo corregirlo

Responde SIEMPRE en este formato:

VEREDICTO: APROBADO | RECHAZADO | PARCIAL
PUNTUACION: 0-100
CRITERIOS:
- [ ] Criterio 1: CUMPLE/NO CUMPLE - evidencia
- [ ] Criterio 2: CUMPLE/NO CUMPLE - evidencia

FEEDBACK:
- Lista de problemas concretos y accionables
- Pasos específicos para corregir cada problema

Si el veredicto no es APROBADO, no alegues éxito: describe con precisión
qué debe cambiar para alcanzar APROBADO.""",
}


@dataclass
class SubAgentResult:
    """Resultado de la ejecución de un sub-agente."""

    agent_name: str
    task: str
    output: str
    success: bool
    iterations: int = 0
    tools_used: list[str] = field(default_factory=list)


class SubAgent:
    """Sub-agente especializado en un dominio específico."""

    def __init__(  # noqa: PLR0913
        self,
        name: str,
        system_prompt: str,
        tools: ToolRegistry,
        model: str = "qwen3-8b",
        base_url: str = "http://localhost:8080/v1",
        api_key: str | None = None,
        allow_remote: bool = False,
        max_iterations: int = 3,
    ):
        self.name = name
        self.system_prompt = system_prompt
        self.tools = tools
        self.model = model
        self.base_url = base_url
        self.api_key = api_key
        self.allow_remote = allow_remote
        self.max_iterations = max_iterations
        self.results: list[SubAgentResult] = []

    async def _chat(self, prompt: str, stream: bool = False, context: str = "") -> str:
        """Envía un mensaje al modelo local y devuelve la respuesta."""
        headers = {}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"

        system_prompt = self.system_prompt
        if context:
            system_prompt = f"{system_prompt}\n\n{context}"

        full_prompt = f"{system_prompt}\n\nUsuario: {prompt}\n\nAsistente:"

        async with httpx.AsyncClient(timeout=120, headers=headers) as client:
            payload = {
                "model": self.model,
                "messages": [{"role": "user", "content": full_prompt}],
                "max_tokens": 4096,
                "temperature": 0.7,
                "stream": stream,
            }
            resp = await client.post(f"{self.base_url}/chat/completions", json=payload, timeout=120)
            data = resp.json()
            message = data.get("choices", [{}])[0].get("message", {})
            content = message.get("content", "") or ""
            if not content:
                content = message.get("reasoning_content", "") or ""
            return content

    def run(self, task: str, context: str = "") -> SubAgentResult:
        """Ejecuta una tarea usando el sub-agente.

        ``context`` se inyecta solo en esta ejecución (no muta el prompt
        permanente del agente).
        """
        try:
            response = asyncio.run(self._chat(task, context=context))
        except Exception as e:
            result = SubAgentResult(
                agent_name=self.name,
                task=task,
                output=f"Error: {e}",
                success=False,
                iterations=0,
            )
        else:
            result = SubAgentResult(
                agent_name=self.name,
                task=task,
                output=response,
                success=True,
                iterations=1,
            )
        self.results.append(result)
        return result


class SharedMemory:
    """Memoria vectorial compartida entre sub-agentes.

    Permite que los sub-agentes almacenen y recuperen conocimiento
    común usando búsqueda semántica (RAG local).
    """

    def __init__(self, collection_name: str = "shared_agent_memory"):
        self.collection = collection_name
        # RAG es opcional (requiere sentence-transformers). Si no está
        # disponible, los sub-agentes siguen funcionando sin memoria.
        try:
            self.rag = get_rag()
        except Exception:
            self.rag = None

    @property
    def available(self) -> bool:
        """True si la memoria vectorial está operativa."""
        return self.rag is not None

    def store(self, text: str, metadata: dict | None = None) -> str:
        """Almacena un fragmento de conocimiento en la memoria compartida."""
        if self.rag is None:
            return ""
        meta = metadata or {}
        ids = self.rag.add_documents(
            collection=self.collection,
            documents=[text],
            metadatas=[meta],
        )
        return ids[0] if ids else ""

    def search(self, query: str, top_k: int = 5) -> list[dict]:
        """Busca conocimiento relevante en la memoria compartida."""
        if self.rag is None:
            return []
        return self.rag.search(
            query=query,
            collection=self.collection,
            top_k=top_k,
            threshold=0.3,
        )

    def get_context(self, query: str, top_k: int = 3) -> str:
        """Devuelve contexto de memoria como texto formateado para prompts."""
        results = self.search(query, top_k=top_k)
        if not results:
            return ""
        parts = ["Memoria compartida relevante:"]
        for r in results:
            parts.append(f'- [{r["score"]:.2f}] {r["text"][:300]}')
        return "\n".join(parts)


class MultiAgentOrchestrator:
    """Orquesta múltiples sub-agentes especializados."""

    def __init__(
        self,
        tools: ToolRegistry,
        model: str = "qwen3-8b",
        base_url: str = "http://localhost:8080/v1",
        api_key: str | None = None,
        allow_remote: bool = False,
    ):
        self.tools = tools
        self.model = model
        self.base_url = base_url
        self.api_key = api_key
        self.allow_remote = allow_remote
        self.shared_memory = SharedMemory()
        self.agents: dict[str, SubAgent] = {}
        self._initialize_agents()

    def _initialize_agents(self) -> None:
        """Crea los sub-agentes especializados por defecto."""
        for name, prompt in SUBAGENT_PROMPTS.items():
            self.agents[name] = SubAgent(
                name=name,
                system_prompt=prompt,
                tools=self.tools,
                model=self.model,
                base_url=self.base_url,
                api_key=self.api_key,
                allow_remote=self.allow_remote,
            )

    def list_agents(self) -> list[str]:
        """Devuelve los nombres de los sub-agentes disponibles."""
        return list(self.agents.keys())

    def get_agent(self, name: str) -> SubAgent | None:
        """Devuelve un sub-agente por nombre."""
        return self.agents.get(name)

    def run_agent(self, agent_name: str, task: str) -> SubAgentResult:
        """Ejecuta una tarea en un sub-agente específico.

        Inyecta contexto de memoria compartida antes de ejecutar.
        Almacena el resultado en la memoria compartida si tuvo éxito.
        """
        agent = self.get_agent(agent_name)
        if agent is None:
            return SubAgentResult(
                agent_name=agent_name,
                task=task,
                output=f'Error: sub-agente "{agent_name}" no disponible. '
                f'Disponibles: {", ".join(self.list_agents())}',
                success=False,
            )

        # Inyectar contexto de memoria compartida en esta ejecución
        memory_context = self.shared_memory.get_context(task, top_k=3)
        result = agent.run(task, context=memory_context)

        # Almacenar resultado exitoso en memoria compartida
        if result.success and result.output:
            self.shared_memory.store(
                text=f"[{agent_name}] {task}\nResultado: {result.output[:1000]}",
                metadata={"agent": agent_name, "task": task, "success": True},
            )

        return result

    def run_pipeline(self, tasks: list[dict[str, str]]) -> list[SubAgentResult]:
        """Ejecuta una pipeline de tareas en diferentes sub-agentes.

        Cada tarea es un dict con claves: 'agent' (nombre del sub-agente)
        y 'task' (descripción de la tarea).
        """
        results: list[SubAgentResult] = []
        for task_def in tasks:
            agent_name = task_def.get("agent", "code")
            task_desc = task_def.get("task", "")
            result = self.run_agent(agent_name, task_desc)
            results.append(result)
        return results

    def critique(
        self,
        output: str,
        criteria: list[str],
        context: str = "",
    ) -> SubAgentResult:
        """Evalúa una salida contra criterios de aceptación usando el agente crítico.

        El crítico es un sub-agente independiente (no el mismo que generó
        el código), lo que evita el sesgo de autoevaluación.

        Devuelve un SubAgentResult cuyo output contiene el veredicto
        estructurado (VEREDICTO, PUNTUACION, CRITERIOS, FEEDBACK).
        """
        criteria_text = "\n".join(f"- {c}" for c in criteria)
        task = (
            "EVALUA la siguiente salida contra estos CRITERIOS DE ACEPTACIÓN:\n\n"
            f"{criteria_text}\n\n"
            f"SALIDA A EVALUAR:\n---\n{output}\n---\n"
        )
        if context:
            task = f"CONTEXTO: {context}\n\n{task}"

        return self.run_agent("critic", task)

    def run_collaborative(self, goal: str) -> str:
        """Ejecuta un flujo colaborativo entre sub-agentes para lograr una meta.

        Flujo: code → review → test → doc (opcional).
        Cada sub-agente usa la salida del anterior como contexto y
        la memoria vectorial compartida persiste entre iteraciones.
        """
        pipeline = [
            {"agent": "code", "task": goal},
            {"agent": "review", "task": f"Revisa el código generado para: {goal}"},
            {"agent": "test", "task": f"Genera pruebas para el código de: {goal}"},
            {"agent": "doc", "task": f"Documenta el código generado para: {goal}"},
        ]

        results = self.run_pipeline(pipeline)

        output_parts = []
        for result in results:
            output_parts.append(f"\n═══ {result.agent_name.upper()} ═══")
            output_parts.append(result.output)
            output_parts.append("")

        return "\n".join(output_parts)


# Instancia singleton para uso desde CLI


def get_orchestrator(
    tools: ToolRegistry | None = None,
    model: str = "qwen3-8b",
    base_url: str = "http://localhost:8080/v1",
    api_key: str | None = None,
    allow_remote: bool = False,
) -> MultiAgentOrchestrator:
    """Devuelve el orchestrator singleton."""
    if get_orchestrator._instance is None:
        if tools is None:
            tools = ToolRegistry()
        get_orchestrator._instance = MultiAgentOrchestrator(
            tools=tools,
            model=model,
            base_url=base_url,
            api_key=api_key,
            allow_remote=allow_remote,
        )
    return get_orchestrator._instance  # type: ignore[return-value]


get_orchestrator._instance = None
