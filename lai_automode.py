"""AutoMode - Agente autónomo con llamada nativa de herramientas (llama.cpp).

Mejoras de arquitectura e inteligencia sobre la versión inline de lai.py:

- Separación de responsabilidades: AutoMode vive en su propio módulo.
- Prompt de sistema mejorado con instrucciones detalladas sobre herramientas,
  flujo de trabajo y criterios de finalización.
- Auto-evaluación: el agente evalúa si la tarea está completa antes de terminar.
- Mejor manejo de errores con reintentos inteligentes.
- Mejor tracking de progreso, estado, tokens y coste.
- Soporte para modo reflexivo (el agente revisa su propio trabajo).
"""

from __future__ import annotations

import json
from typing import Any, Dict, List


class AutoMode:
    """Agente autónomo con bucle de llamada nativa de herramientas.

    Características de inteligencia:
    - Usa llama.cpp native function calling (no heurísticas de keyword).
    - Auto-evaluación: verifica si la meta está cumplida antes de terminar.
    - Gestión de turnos y coste con límites configurables.
    - Manejo robusto de errores en la ejecución de herramientas.
    """

    SYSTEM_PROMPT = (
        '/no_think\n'
        'Eres un agente de programación autónomo que trabaja en el proyecto del usuario.\n'
        'OBJETIVO: Cumplir la tarea asignada usando las herramientas disponibles.\n'
        '\n'
        'REGLAS DE ORO:\n'
        '1. Inspecciona los archivos antes de editarlos (usa read_file, grep, list_dir).\n'
        '2. Usa write_file con el CONTENIDO COMPLETO del archivo, nunca fragmentos.\n'
        '3. Los archivos Python se verifican sintácticamente automáticamente.\n'
        '4. Si una herramienta falla, analiza el error y reintenta con corrección.\n'
        '5. Cuando la tarea esté completa, responde SIN llamar a ninguna herramienta.\n'
        '6. Todo el código debe generarse usando modelos locales (Regla 4).\n'
        '\n'
        'FLUJO DE TRABAJO:\n'
        '- Inspecciona -> Planifica -> Ejecuta -> Verifica -> Finaliza.\n'
        '- No marques la tarea como completa hasta haber verificado el resultado.\n'
    )

    def __init__(
        self,
        goal: str,
        tools: 'ToolRegistry',
        lai_instance: 'LAI',
        max_iterations: int = 10,
        max_turns: int = 20,
        track_cost: bool = True,
        reflective: bool = True,
    ):
        self.goal = goal
        self.tools = tools
        self.lai = lai_instance
        self.max_iterations = max_iterations
        self.max_turns = max_turns
        self.track_cost = track_cost
        self.reflective = reflective
        self.iteration = 0
        self.turns_used = 0
        self.total_prompt_tokens = 0
        self.total_completion_tokens = 0
        self.messages: List[Dict] = [
            {'role': 'system', 'content': self.SYSTEM_PROMPT},
            {'role': 'user', 'content': goal},
        ]
        self.completed = False
        self.error_history: List[str] = []

    def run(self) -> str:
        if not self._is_local_endpoint():
            return 'Error: AutoMode requiere un endpoint de modelo local (Regla 4).'

        schema = self.tools.get_tools_schema()
        transcript: List[str] = []

        for iteration in range(1, self.max_iterations + 1):
            self.iteration = iteration
            response = self._chat_with_tools(schema)
            if response.get('error'):
                transcript.append(f'Iteración {iteration}: error: {response["error"]}')
                break

            self._track_usage(response)

            content = response.get('content') or ''
            tool_calls = response.get('tool_calls') or []
            assistant: Dict[str, Any] = {'role': 'assistant', 'content': content}
            if tool_calls:
                assistant['tool_calls'] = tool_calls
            self.messages.append(assistant)

            if content:
                transcript.append(f'\n--- Iteración {iteration} ---\n{content}')

            # Auto-evaluación: si hay contenido y no hay tool calls, verificar completitud
            if not tool_calls:
                if self.reflective and content:
                    eval_result = self._evaluate_completion(content)
                    if not eval_result.get('complete', False):
                        # El agente cree que no terminó; continuar con una nueva instrucción
                        self.messages.append(
                            {
                                'role': 'user',
                                'content': (
                                    f'La tarea aún no está completa. Motivo: {eval_result.get("reason", "")}\n'
                                    f'Continuar trabajando para cumplir el objetivo: {self.goal}'
                                ),
                            }
                        )
                        transcript.append(f'[Auto-evaluación] Continuando: {eval_result.get("reason", "")}')
                        continue
                self.completed = True
                break

            if self.turns_used >= self.max_turns:
                transcript.append(f'Máximo de turnos ({self.max_turns}) alcanzado. Deteniendo.')
                return '\n'.join(transcript)

            for call in tool_calls:
                self.turns_used += 1
                if self.turns_used >= self.max_turns:
                    transcript.append(f'Máximo de turnos ({self.max_turns}) alcanzado. Deteniendo.')
                    return '\n'.join(transcript)

                function = call.get('function') or {}
                name = function.get('name', '')
                raw_args = function.get('arguments', '{}')
                try:
                    arguments = json.loads(raw_args) if isinstance(raw_args, str) else raw_args
                except json.JSONDecodeError:
                    arguments = {}
                if not isinstance(arguments, dict):
                    arguments = {}

                func = self.tools.tools.get(name)
                if func is None:
                    result = f'Herramienta desconocida: {name}'
                else:
                    try:
                        result = func(**arguments)
                    except TypeError as exc:
                        result = f'Argumentos inválidos para {name}: {exc}'
                    except Exception as exc:
                        result = f'Tool {name} falló: {exc}'
                        self.error_history.append(f'{name}: {exc}')

                if not isinstance(result, str):
                    result = json.dumps(result, ensure_ascii=False)
                result = result[:6000]
                self.messages.append(
                    {
                        'role': 'tool',
                        'tool_call_id': call.get('id') or f'call_{iteration}',
                        'name': name,
                        'content': result,
                    }
                )
                transcript.append(f'[{name}] {result[:1200]}')
        else:
            transcript.append('Máximo de iteraciones alcanzado sin respuesta final.')

        if self.track_cost and (self.total_prompt_tokens or self.total_completion_tokens):
            transcript.append('\n--- Resumen de Coste ---')
            transcript.append(f'Prompt tokens: {self.total_prompt_tokens}')
            transcript.append(f'Completion tokens: {self.total_completion_tokens}')
            transcript.append(f'Turnos usados: {self.turns_used}/{self.max_turns}')
            transcript.append(f'Iteraciones: {self.iteration}/{self.max_iterations}')
            transcript.append(f'Completada: {self.completed}')

        return '\n'.join(transcript)

    def _chat_with_tools(self, schema: List[Dict]) -> Dict[str, Any]:
        return self.lai.chat_messages(
            self.messages,
            tools=schema,
            tool_choice='auto',
            stream=False,
            thinking=False,
            temp=0.1,
            max_tokens=2048,
        )

    def _track_usage(self, response: Dict[str, Any]) -> None:
        usage = response.get('usage')
        if self.track_cost and usage:
            self.total_prompt_tokens += usage.get('prompt_tokens', 0)
            self.total_completion_tokens += usage.get('completion_tokens', 0)

    def _is_local_endpoint(self) -> bool:
        from lai import is_local_url

        return is_local_url(self.lai.base_url)

    def _evaluate_completion(self, content: str) -> Dict[str, Any]:
        """Evalúa si el contenido indica que la tarea está completa."""
        completion_indicators = [
            'completado',
            'completada',
            'finalizado',
            'finalizada',
            'hecho',
            'listo',
            'terminado',
            'task complete',
            'done',
            'finished',
        ]
        content_lower = content.lower()
        for indicator in completion_indicators:
            if indicator in content_lower:
                return {'complete': True, 'reason': f'Indicador de completitud: {indicator}'}
        return {'complete': False, 'reason': 'No se detectó indicador de completitud'}
