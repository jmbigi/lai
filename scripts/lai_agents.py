"""Sistema de sub-agentes especializados (multi-agente) para lai.

Cada sub-agente tiene un system prompt especializado y un conjunto de
herramientas relevantes para su dominio. El MultiAgentOrchestrator puede
coordinar múltiples sub-agentes para resolver tareas complejas.

100% local, sin cloud, sin API keys. Regla 0 de lai.
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from typing import Optional

import httpx

from lai_tools import ToolRegistry

# ---------------------------------------------------------------------------
# System prompts especializados por dominio
# ---------------------------------------------------------------------------

SUBAGENT_PROMPTS: dict[str, str] = {
    'code': """Eres un Agente Especialista en Generación de Código.
Tu misión es escribir código limpio, funcional y bien documentado.
Siempre incluye: type hints, docstrings (estilo Google), manejo de errores,
y bloques if __name__ == "__main__". No uses markdown, solo código Python puro.
Prioriza la legibilidad y el cumplimiento de PEP 8.""",
    'review': """Eres un Agente Especialista en Revisión de Código (Code Review).
Tu misión es analizar código en busca de: bugs, vulnerabilidades de seguridad,
problemas de rendimiento, violaciones de estilo y oportunidades de refactorización.
Proporciona retroalimentación constructiva, específica y accionable.
Clasifica hallazgos por severidad: crítico, alto, medio, bajo.""",
    'test': """Eres un Agente Especialista en Generación de Pruebas.
Tu misión es escribir pruebas unitarias y de integración completas.
Usa pytest, cubre casos edge, pruebas de error y pruebas felices.
Incluye fixtures, mocks cuando sea necesario, y reportes de cobertura.""",
    'doc': """Eres un Agente Especialista en Documentación Técnica.
Tu misión es escribir documentación clara, completa y bien estructurada.
Incluye: docstrings, README, comentarios en el código, guías de usuario,
y documentación de API. Usa Markdown y Google-style docstrings.""",
    'fix': """Eres un Agente Especialista en Corrección de Errores (Debugging).
Tu misión es identificar y corregir bugs en código existente.
Analiza traces de error, reproduce el problema, propone una solución
y verifica que la corrección no introduce regresiones.""",
    'arch': """Eres un Agente Especialista en Arquitectura de Software.
Tu misión es diseñar estructuras de código modulares, escalables y
mantenibles. Propón patrones de diseño, definiendo interfaces claras,
separación de responsabilidades y bajo acoplamiento.""",
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
        model: str = 'qwen3-8b',
        base_url: str = 'http://localhost:8080/v1',
        api_key: Optional[str] = None,
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

    async def _chat(self, prompt: str, stream: bool = False) -> str:
        """Envía un mensaje al modelo local y devuelve la respuesta."""
        headers = {}
        if self.api_key:
            headers['Authorization'] = f'Bearer {self.api_key}'

        full_prompt = f'{self.system_prompt}\n\nUsuario: {prompt}\n\nAsistente:'

        async with httpx.AsyncClient(timeout=120, headers=headers) as client:
            payload = {
                'model': self.model,
                'messages': [{'role': 'user', 'content': full_prompt}],
                'max_tokens': 4096,
                'temperature': 0.7,
                'stream': stream,
            }
            resp = await client.post(
                f'{self.base_url}/chat/completions', json=payload, timeout=120
            )
            data = resp.json()
            message = data.get('choices', [{}])[0].get('message', {})
            content = message.get('content', '') or ''
            if not content:
                content = message.get('reasoning_content', '') or ''
            return content

    def run(self, task: str) -> SubAgentResult:
        """Ejecuta una tarea usando el sub-agente."""
        try:
            response = asyncio.run(self._chat(task))
            result = SubAgentResult(
                agent_name=self.name,
                task=task,
                output=response,
                success=True,
                iterations=1,
            )
            self.results.append(result)
            return result
        except Exception as e:
            result = SubAgentResult(
                agent_name=self.name,
                task=task,
                output=f'Error: {e}',
                success=False,
                iterations=0,
            )
            self.results.append(result)
            return result


class MultiAgentOrchestrator:
    """Orquesta múltiples sub-agentes especializados."""

    def __init__(
        self,
        tools: ToolRegistry,
        model: str = 'qwen3-8b',
        base_url: str = 'http://localhost:8080/v1',
        api_key: Optional[str] = None,
        allow_remote: bool = False,
    ):
        self.tools = tools
        self.model = model
        self.base_url = base_url
        self.api_key = api_key
        self.allow_remote = allow_remote
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

    def get_agent(self, name: str) -> Optional[SubAgent]:
        """Devuelve un sub-agente por nombre."""
        return self.agents.get(name)

    def run_agent(self, agent_name: str, task: str) -> SubAgentResult:
        """Ejecuta una tarea en un sub-agente específico."""
        agent = self.get_agent(agent_name)
        if agent is None:
            return SubAgentResult(
                agent_name=agent_name,
                task=task,
                output=f'Error: sub-agente "{agent_name}" no disponible. '
                f'Disponibles: {", ".join(self.list_agents())}',
                success=False,
            )
        return agent.run(task)

    def run_pipeline(self, tasks: list[dict[str, str]]) -> list[SubAgentResult]:
        """Ejecuta una pipeline de tareas en diferentes sub-agentes.

        Cada tarea es un dict con claves: 'agent' (nombre del sub-agente)
        y 'task' (descripción de la tarea).
        """
        results: list[SubAgentResult] = []
        for task_def in tasks:
            agent_name = task_def.get('agent', 'code')
            task_desc = task_def.get('task', '')
            result = self.run_agent(agent_name, task_desc)
            results.append(result)
        return results

    def run_collaborative(self, goal: str, max_iterations: int = 3) -> str:
        """Ejecuta un flujo colaborativo entre sub-agentes para lograr una meta.

        Flujo: code → review → test → doc (opcional).
        Cada sub-agente usa la salida del anterior como contexto.
        """
        pipeline = [
            {'agent': 'code', 'task': goal},
            {'agent': 'review', 'task': f"Revisa el código generado para: {goal}"},
            {'agent': 'test', 'task': f"Genera pruebas para el código de: {goal}"},
            {'agent': 'doc', 'task': f"Documenta el código generado para: {goal}"},
        ]

        results = self.run_pipeline(pipeline)

        output_parts = []
        for result in results:
            output_parts.append(f'\n═══ {result.agent_name.upper()} ═══')
            output_parts.append(result.output)
            output_parts.append('')

        return '\n'.join(output_parts)


# Instancia singleton para uso desde CLI
_orchestrator: Optional[MultiAgentOrchestrator] = None


def get_orchestrator(
    tools: Optional[ToolRegistry] = None,
    model: str = 'qwen3-8b',
    base_url: str = 'http://localhost:8080/v1',
    api_key: Optional[str] = None,
    allow_remote: bool = False,
) -> MultiAgentOrchestrator:
    """Devuelve el orchestrator singleton."""
    global _orchestrator
    if _orchestrator is None:
        if tools is None:
            tools = ToolRegistry()
        _orchestrator = MultiAgentOrchestrator(
            tools=tools,
            model=model,
            base_url=base_url,
            api_key=api_key,
            allow_remote=allow_remote,
        )
    return _orchestrator
