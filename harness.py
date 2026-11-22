#!/usr/bin/env python3
"""
Harness Local - Gestor avanzado de sesiones, thinking, tools y auto-mode para modelos locales.

Proporciona:
- Gestión de sesiones (historial de conversación persistente)
- Manejo de thinking/reasoning_content de Qwen3 y modelos compatibles
- Tool calling (function calling) con validación
- Auto-mode para ejecución autónoma de tareas complejas
- Truncamiento inteligente de contexto con preservación de código
- Compresión semántica de contexto
- Estadísticas y métricas de uso
"""

import json
import os
import uuid
import re
import asyncio
import time
from typing import Dict, List, Optional, Any, Callable, Union
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum

# Configuración
HISTORY_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "sessions")
MAX_CONTEXT_TOKENS = 32768
MAX_OUTPUT_TOKENS = 8192
DEFAULT_TEMPERATURE = 0.2


class ToolType(Enum):
    FUNCTION = "function"
    BUILTIN = "builtin"


@dataclass
class Tool:
    """Definición de una herramienta disponible."""

    name: str
    description: str
    parameters: Dict[str, Any]
    tool_type: ToolType = ToolType.FUNCTION
    handler: Optional[Callable] = None
    required: List[str] = field(default_factory=list)


@dataclass
class ToolCall:
    """Llamada a una herramienta."""

    id: str
    name: str
    arguments: Dict[str, Any]
    result: Optional[Any] = None
    error: Optional[str] = None


@dataclass
class ThinkingBlock:
    """Bloque de razonamiento (thinking)."""

    content: str
    start_time: float
    end_time: Optional[float] = None
    tokens: int = 0


class ToolRegistry:
    """Registro de herramientas disponibles."""

    def __init__(self):
        self.tools: Dict[str, Tool] = {}
        self._register_builtin_tools()

    def _register_builtin_tools(self):
        """Registra herramientas integradas."""
        # Tool: leer archivo
        self.register(
            Tool(
                name="read_file",
                description="Lee el contenido de un archivo",
                parameters={
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Ruta del archivo a leer"}
                    },
                    "required": ["path"],
                },
                tool_type=ToolType.BUILTIN,
                handler=self._builtin_read_file,
                required=["path"],
            )
        )

        # Tool: escribir archivo
        self.register(
            Tool(
                name="write_file",
                description="Escribe contenido a un archivo",
                parameters={
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Ruta del archivo"},
                        "content": {"type": "string", "description": "Contenido a escribir"},
                    },
                    "required": ["path", "content"],
                },
                tool_type=ToolType.BUILTIN,
                handler=self._builtin_write_file,
                required=["path", "content"],
            )
        )

        # Tool: listar directorio
        self.register(
            Tool(
                name="list_dir",
                description="Lista el contenido de un directorio",
                parameters={
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Ruta del directorio"}
                    },
                    "required": ["path"],
                },
                tool_type=ToolType.BUILTIN,
                handler=self._builtin_list_dir,
                required=["path"],
            )
        )

        # Tool: ejecutar comando
        self.register(
            Tool(
                name="run_command",
                description="Ejecuta un comando en la terminal",
                parameters={
                    "type": "object",
                    "properties": {
                        "command": {"type": "string", "description": "Comando a ejecutar"},
                        "cwd": {
                            "type": "string",
                            "description": "Directorio de trabajo (opcional)",
                        },
                        "timeout": {
                            "type": "integer",
                            "description": "Timeout en segundos (default: 30)",
                        },
                    },
                    "required": ["command"],
                },
                tool_type=ToolType.BUILTIN,
                handler=self._builtin_run_command,
                required=["command"],
            )
        )

        # Tool: búsqueda en archivos
        self.register(
            Tool(
                name="grep",
                description="Busca un patrón en archivos",
                parameters={
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string", "description": "Patrón a buscar (regex)"},
                        "path": {"type": "string", "description": "Directorio donde buscar"},
                        "include": {
                            "type": "string",
                            "description": "Patrón de archivos a incluir (opcional)",
                        },
                    },
                    "required": ["pattern", "path"],
                },
                tool_type=ToolType.BUILTIN,
                handler=self._builtin_grep,
                required=["pattern", "path"],
            )
        )

        # Tool: python eval
        self.register(
            Tool(
                name="python_eval",
                description="Evalúa código Python y retorna el resultado",
                parameters={
                    "type": "object",
                    "properties": {
                        "code": {"type": "string", "description": "Código Python a evaluar"}
                    },
                    "required": ["code"],
                },
                tool_type=ToolType.BUILTIN,
                handler=self._builtin_python_eval,
                required=["code"],
            )
        )

    def register(self, tool: Tool):
        self.tools[tool.name] = tool

    def get_tool(self, name: str) -> Optional[Tool]:
        return self.tools.get(name)

    def get_all_schemas(self) -> List[Dict[str, Any]]:
        """Retorna esquemas en formato OpenAI function calling."""
        schemas = []
        for tool in self.tools.values():
            schemas.append(
                {
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    },
                }
            )
        return schemas

    async def execute(self, tool_call: ToolCall) -> ToolCall:
        tool = self.get_tool(tool_call.name)
        if not tool:
            tool_call.error = f"Tool '{tool_call.name}' not found"
            return tool_call

        try:
            if tool.handler:
                if asyncio.iscoroutinefunction(tool.handler):
                    result = await tool.handler(**tool_call.arguments)
                else:
                    result = tool.handler(**tool_call.arguments)
                tool_call.result = result
            else:
                tool_call.error = "No handler for tool"
        except Exception as e:
            tool_call.error = str(e)

        return tool_call

    # Builtin handlers
    def _builtin_read_file(self, path: str) -> str:
        try:
            with open(path, "r", encoding="utf-8") as f:
                return f.read()
        except Exception as e:
            return f"Error: {e}"

    def _builtin_write_file(self, path: str, content: str) -> str:
        try:
            os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
            with open(path, "w", encoding="utf-8") as f:
                f.write(content)
            return f"Written {len(content)} bytes to {path}"
        except Exception as e:
            return f"Error: {e}"

    def _builtin_list_dir(self, path: str) -> str:
        try:
            items = os.listdir(path)
            return json.dumps(items, indent=2)
        except Exception as e:
            return f"Error: {e}"

    def _builtin_run_command(self, command: str, cwd: str = None, timeout: int = 30) -> str:
        try:
            result = subprocess.run(
                command, shell=True, cwd=cwd, timeout=timeout, capture_output=True, text=True
            )
            output = result.stdout
            if result.stderr:
                output += f"\nSTDERR: {result.stderr}"
            if result.returncode != 0:
                output += f"\nExit code: {result.returncode}"
            return output
        except subprocess.TimeoutExpired:
            return f"Error: Command timed out after {timeout}s"
        except Exception as e:
            return f"Error: {e}"

    def _builtin_grep(self, pattern: str, path: str, include: str = "") -> str:
        try:
            cmd = ["grep", "-r", "-n", pattern, path]
            if include:
                cmd.extend(["--include", include])
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
            return result.stdout if result.stdout else "No matches found"
        except Exception as e:
            return f"Error: {e}"

    def _builtin_python_eval(self, code: str) -> str:
        try:
            # Create a safe namespace
            namespace = {"__builtins__": {}}
            result = eval(code, namespace)
            return str(result)
        except Exception as e:
            # Try exec for statements
            try:
                namespace = {"__builtins__": {}}
                exec(code, namespace)
                return "Executed successfully"
            except Exception as e2:
                return f"Error: {e2}"


import subprocess


class ThinkingManager:
    """Gestiona el modo thinking/reasoning de modelos como Qwen3."""

    def __init__(self, enabled: bool = True):
        self.enabled = enabled
        self.thinking_blocks: List[ThinkingBlock] = []

    def extract_reasoning(self, content: str) -> tuple[str, str]:
        """Extrae el contenido de thinking de la respuesta del modelo."""
        if not self.enabled:
            return "", content

        # Patrones para diferentes formatos de thinking
        patterns = [
            r"<thinking>\n(.*?)\n</thinking>",  # Qwen3 standard
            r"<think>\n(.*?)\n</think>",  # DeepSeek style
            r"\\|begin_thinking\\|(.*?)\\|end_thinking\\|",  # Some models
        ]

        for pattern in patterns:
            matches = list(re.finditer(pattern, content, re.DOTALL))
            if matches:
                # Tomar el último bloque de thinking (el más completo)
                match = matches[-1]
                thinking = match.group(1).strip()
                clean_content = re.sub(pattern, "", content, flags=re.DOTALL).strip()

                # Registrar bloque de thinking
                block = ThinkingBlock(
                    content=thinking,
                    start_time=time.time() - 1,  # Aproximado
                    end_time=time.time(),
                    tokens=len(thinking) // 4,
                )
                self.thinking_blocks.append(block)

                return thinking, clean_content

        return "", content

    def format_thinking_for_kimi(self, thinking: str, content: str) -> str:
        """Formatea thinking y contenido para compatibilidad con Kimi Code."""
        if self.enabled and thinking:
            return f"<thinking>\n{thinking}\n</thinking>\n\n{content}"
        return content

    def get_thinking_summary(self) -> Dict[str, Any]:
        """Retorna resumen de bloques de thinking."""
        total_tokens = sum(b.tokens for b in self.thinking_blocks)
        return {
            "blocks": len(self.thinking_blocks),
            "total_tokens": total_tokens,
            "blocks_detail": [
                {"tokens": b.tokens, "preview": b.content[:100]} for b in self.thinking_blocks
            ],
        }

    def clear(self):
        self.thinking_blocks.clear()


class ContextManager:
    """Gestiona el contexto de la conversación con truncamiento inteligente."""

    def __init__(self, max_tokens: int = MAX_CONTEXT_TOKENS):
        self.max_tokens = max_tokens

    def estimate_tokens(self, text: str) -> int:
        """Estima el número de tokens en un texto (aproximación: 1 token ≈ 3.5 chars para código)."""
        return max(1, len(text) // 4)

    def compress_content(self, text: str, preserve_code: bool = True) -> str:
        """Comprime contenido preservando bloques de código."""
        if not text:
            return text

        if preserve_code:
            code_blocks = []

            def save_code(m):
                code_blocks.append(m.group(0))
                return f"__CODE_BLOCK_{len(code_blocks)-1}__"

            text = re.sub(r"```[\s\S]*?```", save_code, text)
            text = re.sub(r"`[^`]+`", save_code, text)

        text = re.sub(r"[ \t]+", " ", text)
        text = re.sub(r"\n{3,}", "\n\n", text)
        text = "\n".join(line.strip() for line in text.split("\n"))
        text = text.strip()

        if preserve_code and code_blocks:
            for i, block in enumerate(code_blocks):
                text = text.replace(f"__CODE_BLOCK_{i}__", block)

        return text

    def filter_system_noise(self, messages: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
        """Filtra mensajes de sistema que son ruido para modelos locales."""
        noise_keywords = [
            "TaskOutput",
            "TaskStop",
            "TodoList",
            "UpdateGoal",
            "WaitFor",
            "auto permission mode",
            "Never Ask",
            "Plan mode",
            "system reminder",
            "Kimi K3.1 Pro",
            "session_",
            "Permission mode:",
            "Everything runs",
            "only plans in Plan mode",
            "Switched to",
            "thinking on",
            "thinking off",
            "Use this tool",
            "function",
            "parameters",
            "description",
            "crush",
            "Crush",
            "charm.land",
            "catwalk",
            "fang",
            "bubbletea",
            "lipgloss",
            "glamour",
            "tone",
            "ultraviolet",
            "x/ansi",
            "opencode",
            "OpenCode",
            "Nemotron",
            "nvidia",
        ]

        def is_noise(content: str) -> bool:
            if not content:
                return False
            content_lower = content.lower()
            return any(kw.lower() in content_lower for kw in noise_keywords)

        filtered = []
        for msg in messages:
            role = msg.get("role", "")
            content = msg.get("content", "")

            if role == "system":
                if len(content) < 500 and not any(
                    kw.lower() in content.lower() for kw in noise_keywords
                ):
                    filtered.append(msg)
                continue

            if is_noise(content):
                continue

            filtered.append(msg)
        return filtered

    def truncate_messages(
        self, messages: List[Dict[str, Any]], max_tokens: Optional[int] = None
    ) -> List[Dict[str, Any]]:
        """Trunca mensajes inteligentemente preservando el último mensaje del usuario."""
        if max_tokens is None:
            max_tokens = self.max_tokens

        if not messages:
            return messages

        # Filtrar ruido
        filtered = self.filter_system_noise(messages)
        if not filtered:
            return filtered

        # Comprimir cada mensaje
        for msg in filtered:
            content = msg.get("content", "")
            msg["content"] = self.compress_content(content)

        # Calcular tokens totales
        total_tokens = sum(self.estimate_tokens(msg.get("content", "")) for msg in filtered)
        if total_tokens <= max_tokens:
            return filtered

        # Truncar desde lo más antiguo, preservando último mensaje de usuario
        result = []
        remaining = max_tokens

        # Encontrar último mensaje de usuario
        last_user_idx = -1
        for i, msg in enumerate(reversed(filtered)):
            if msg.get("role") == "user":
                last_user_idx = len(filtered) - 1 - i
                break

        for i, msg in enumerate(reversed(filtered)):
            orig_idx = len(filtered) - 1 - i
            content = msg.get("content", "")
            msg_tokens = self.estimate_tokens(content)
            is_last_user = orig_idx == last_user_idx

            if is_last_user or msg_tokens <= remaining:
                result.insert(0, msg)
                remaining -= msg_tokens
            else:
                break

        return result

    def format_messages_for_model(self, messages: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
        """Formatea mensajes para el modelo local."""
        formatted = []
        for msg in messages:
            role = msg.get("role", "user")
            content = msg.get("content", "")
            formatted.append({"role": role, "content": content})
        return formatted


class AutoMode:
    """Modo automático para ejecución autónoma de tareas complejas."""

    def __init__(self, harness: "Harness", max_iterations: int = 10):
        self.harness = harness
        self.max_iterations = max_iterations
        self.iteration = 0
        self.tool_registry = ToolRegistry()
        self.task_history: List[Dict[str, Any]] = []

    async def run(self, objective: str, context: str = "") -> Dict[str, Any]:
        """Ejecuta una tarea de forma autónoma."""
        self.iteration = 0
        self.task_history = []

        # Prompt inicial del sistema para auto-mode
        system_prompt = self._build_system_prompt()

        # Inicializar conversación
        messages = [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": f"Objective: {objective}\n\nContext: {context}"},
        ]

        while self.iteration < self.max_iterations:
            self.iteration += 1

            # Obtener respuesta del modelo
            response = await self._get_model_response(messages)

            # Procesar tool calls
            tool_calls = self._extract_tool_calls(response)

            if not tool_calls:
                # No hay tool calls, verificar si la tarea está completa
                if self._is_task_complete(response):
                    return {
                        "status": "completed",
                        "result": response,
                        "iterations": self.iteration,
                        "history": self.task_history,
                    }

                # Pedir continuación
                messages.append(
                    {"role": "user", "content": "Continue with the task. What's the next step?"}
                )
                continue

            # Ejecutar tool calls
            for tc in tool_calls:
                result = await self.tool_registry.execute(tc)
                self.task_history.append(
                    {
                        "iteration": self.iteration,
                        "tool": tc.name,
                        "arguments": tc.arguments,
                        "result": result.result,
                        "error": result.error,
                    }
                )

                # Añadir resultado a mensajes
                messages.append(
                    {
                        "role": "tool",
                        "tool_call_id": tc.id,
                        "content": json.dumps(result.result) if result.result else result.error,
                    }
                )

        return {
            "status": "max_iterations_reached",
            "iterations": self.max_iterations,
            "history": self.task_history,
        }

    def _build_system_prompt(self) -> str:
        return """You are an autonomous AI agent that can use tools to complete complex tasks.
You have access to the following tools:
- read_file: Read file contents
- write_file: Write content to files
- list_dir: List directory contents
- run_command: Execute shell commands
- grep: Search for patterns in files
- python_eval: Evaluate Python code

When you need to use a tool, respond with a tool call in the proper format.
Think step by step and use tools to accomplish the objective.
Be thorough and verify your work."""

    async def _get_model_response(self, messages: List[Dict]) -> str:
        # This would call the actual model through the gateway
        # For now, return a placeholder
        return ""

    def _extract_tool_calls(self, response: str) -> List[ToolCall]:
        # Parse tool calls from response
        # This would parse the model's tool call format
        return []

    def _is_task_complete(self, response: str) -> bool:
        # Check if the response indicates task completion
        completion_indicators = ["task complete", "done", "finished", "completed"]
        return any(indicator in response.lower() for indicator in completion_indicators)


class SessionManager:
    """Gestiona sesiones de conversación con historial persistente."""

    def __init__(self, history_dir: str = HISTORY_DIR):
        self.history_dir = history_dir
        os.makedirs(history_dir, exist_ok=True)

    def create_session(self, session_id: Optional[str] = None) -> str:
        if session_id is None:
            session_id = str(uuid.uuid4())
        session_path = os.path.join(self.history_dir, f"{session_id}.json")
        if not os.path.exists(session_path):
            self._save_session(session_id, [])
        return session_id

    def get_session(self, session_id: str) -> List[Dict[str, Any]]:
        session_path = os.path.join(self.history_dir, f"{session_id}.json")
        if os.path.exists(session_path):
            with open(session_path, "r", encoding="utf-8") as f:
                return json.load(f)
        return []

    def save_session(self, session_id: str, messages: List[Dict[str, Any]]):
        self._save_session(session_id, messages)

    def _save_session(self, session_id: str, messages: List[Dict[str, Any]]):
        session_path = os.path.join(self.history_dir, f"{session_id}.json")
        with open(session_path, "w", encoding="utf-8") as f:
            json.dump(messages, f, indent=2, ensure_ascii=False)

    def list_sessions(self) -> List[str]:
        if not os.path.exists(self.history_dir):
            return []
        return [f.replace(".json", "") for f in os.listdir(self.history_dir) if f.endswith(".json")]


class Harness:
    """Harness principal que combina todos los componentes."""

    def __init__(
        self,
        session_id: Optional[str] = None,
        max_context_tokens: int = MAX_CONTEXT_TOKENS,
        thinking_enabled: bool = True,
        auto_mode: bool = False,
    ):
        self.session_manager = SessionManager()
        self.context_manager = ContextManager(max_context_tokens)
        self.thinking_manager = ThinkingManager(thinking_enabled)
        self.tool_registry = ToolRegistry()
        self.auto_mode = AutoMode(self) if auto_mode else None

        if session_id:
            self.session_id = session_id
        else:
            self.session_id = self.session_manager.create_session()

        self.messages = self.session_manager.get_session(self.session_id)

        # Añadir mensaje de sistema si es nueva sesión
        if not self.messages:
            self.messages.append(
                {
                    "role": "system",
                    "content": "You are a helpful AI assistant with access to tools. "
                    "You can use tools to read/write files, run commands, search files, etc.",
                }
            )

    def add_message(self, role: str, content: str):
        self.messages.append({"role": role, "content": content})
        self.session_manager.save_session(self.session_id, self.messages)

    def get_context(self) -> List[Dict[str, Any]]:
        truncated = self.context_manager.truncate_messages(self.messages)
        return self.context_manager.format_messages_for_model(truncated)

    def process_response(self, response: Dict[str, Any]) -> Dict[str, Any]:
        choices = response.get("choices", [])
        if not choices:
            return response

        message = choices[0].get("message", {})
        content = message.get("content", "")

        thinking, clean_content = self.thinking_manager.extract_reasoning(content)

        if thinking:
            message["reasoning_content"] = thinking
            message["content"] = clean_content

        choices[0]["message"] = message
        return response

    def save_response(self, response: Dict[str, Any]):
        choices = response.get("choices", [])
        if choices:
            message = choices[0].get("message", {})
            self.add_message(
                message.get("role", "assistant"),
                message.get("content", ""),
            )

    async def run_auto(self, objective: str, context: str = "") -> Dict[str, Any]:
        """Ejecuta una tarea en modo automático."""
        if not self.auto_mode:
            self.auto_mode = AutoMode(self)
        return await self.auto_mode.run(objective, context)

    async def call_tool(self, name: str, arguments: Dict[str, Any]) -> ToolCall:
        """Llama a una herramienta directamente."""
        tool_call = ToolCall(id=str(uuid.uuid4()), name=name, arguments=arguments)
        return await self.tool_registry.execute(tool_call)

    def get_available_tools(self) -> List[Dict[str, Any]]:
        return self.tool_registry.get_all_schemas()

    def get_thinking_summary(self) -> Dict[str, Any]:
        return self.thinking_manager.get_thinking_summary()

    def list_sessions(self) -> List[str]:
        return self.session_manager.list_sessions()

    def switch_session(self, session_id: str):
        self.session_id = session_id
        self.messages = self.session_manager.get_session(session_id)

    def get_stats(self) -> Dict[str, Any]:
        total_tokens = sum(
            self.context_manager.estimate_tokens(m.get("content", "")) for m in self.messages
        )
        thinking_summary = self.thinking_manager.get_thinking_summary()
        return {
            "session_id": self.session_id,
            "message_count": len(self.messages),
            "estimated_tokens": total_tokens,
            "max_context_tokens": self.context_manager.max_tokens,
            "thinking": thinking_summary,
            "auto_mode": self.auto_mode is not None,
        }

    def clear_thinking(self):
        self.thinking_manager.clear()

    def enable_auto_mode(self, max_iterations: int = 10):
        self.auto_mode = AutoMode(self, max_iterations)

    def disable_auto_mode(self):
        self.auto_mode = None


if __name__ == "__main__":
    import subprocess

    # Demo básico
    harness = Harness()
    harness.add_message("user", "Hola, ¿cómo estás?")
    print(json.dumps(harness.get_stats(), indent=2, ensure_ascii=False))

    # Demo de tools
    print("\nAvailable tools:")
    for tool in harness.get_available_tools():
        print(f"  - {tool['function']['name']}: {tool['function']['description']}")
