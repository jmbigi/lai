#!/usr/bin/env python3
"""Servidor MCP que expone las herramientas de ToolRegistry de lai.py.

Expone las herramientas de lai.ToolRegistry vía protocolo MCP (JSON-RPC 2.0
sobre stdio), permitiendo que cualquier cliente MCP (Claude Code, LM Studio,
Open WebUI, etc.) use las herramientas de lai localmente, sin cloud ni API keys.

Uso:
  python3 scripts/mcp_server.py
  # Configurar en cliente MCP (ej. claude mcp add --transport stdio):
  #   command: python3
  #   args: [scripts/mcp_server.py]
  #   cwd: /home/kubuntu/curso-ia/modelo-local
"""

from __future__ import annotations

import json
import os
import sys
from typing import Any, Dict, List, Optional

# Añadir el directorio raíz al path para importar lai
ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT_DIR)

import lai  # noqa: E402

PROTOCOL_VERSION = "2026-07-28"

SERVER_INFO = {
    "name": "lai-mcp-server",
    "version": "0.0.1",
    "description": "Exposes lai.py ToolRegistry tools via MCP stdio",
}


class McpServer:
    """Servidor MCP mínimo sobre stdio."""

    def __init__(self) -> None:
        self.registry = lai.ToolRegistry()
        raw_schema: List[Dict] = self.registry.get_tools_schema()
        self.tools_schema: List[Dict] = [self._to_mcp_tool(t) for t in raw_schema]
        self.initialized = False

    @staticmethod
    def _to_mcp_tool(tool: Dict) -> Dict:
        """Convierte una herramienta de formato OpenAI (lai.py) a formato MCP."""
        func = tool.get("function", {})
        return {
            "name": func.get("name", ""),
            "description": func.get("description", ""),
            "inputSchema": func.get("parameters", {"type": "object", "properties": {}}),
        }

    # ------------------------------------------------------------------
    # JSON-RPC helpers
    # ------------------------------------------------------------------
    def _response(self, id_: Any, result: Any) -> Dict:
        return {"jsonrpc": "2.0", "id": id_, "result": result}

    def _error(self, id_: Any, code: int, message: str) -> Dict:
        return {"jsonrpc": "2.0", "id": id_, "error": {"code": code, "message": message}}

    def _send(self, message: Dict) -> None:
        sys.stdout.write(json.dumps(message, ensure_ascii=False) + "\n")
        sys.stdout.flush()

    # ------------------------------------------------------------------
    # MCP methods
    # ------------------------------------------------------------------
    def handle_initialize(self, params: Dict, id_: Any) -> Dict:
        self.initialized = True
        return self._response(
            id_,
            {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {"listChanged": False},
                },
                "serverInfo": SERVER_INFO,
            },
        )

    def handle_tools_list(self, params: Dict, id_: Any) -> Dict:
        return self._response(id_, {"tools": self.tools_schema})

    def handle_tools_call(self, params: Dict, id_: Any) -> Dict:
        name = params.get("name", "")
        arguments = params.get("arguments", {}) or {}

        if name not in self.registry.tools:
            return self._error(id_, -32601, f"Tool not found: {name}")

        try:
            tool_fn = self.registry.tools[name]
            result = tool_fn(**arguments)
            return self._response(
                id_,
                {
                    "content": [{"type": "text", "text": str(result)}],
                    "isError": False,
                },
            )
        except TypeError as exc:
            return self._error(id_, -32602, f"Invalid arguments: {str(exc)}")
        except Exception as exc:
            return self._error(id_, -32603, f"Tool error: {str(exc)}")

    # ------------------------------------------------------------------
    # Main loop
    # ------------------------------------------------------------------
    def run(self) -> None:
        print("lai-mcp-server ready", file=sys.stderr)
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue
            try:
                request = json.loads(line)
            except json.JSONDecodeError:
                continue

            id_ = request.get("id")
            method = request.get("method", "")
            params = request.get("params", {}) or {}

            if method == "initialize":
                self._send(self.handle_initialize(params, id_))
            elif method == "initialized":
                # Notification, no response needed
                continue
            elif method == "tools/list":
                self._send(self.handle_tools_list(params, id_))
            elif method == "tools/call":
                self._send(self.handle_tools_call(params, id_))
            else:
                self._send(self._error(id_, -32601, f"Method not found: {method}"))


def main() -> None:
    server = McpServer()
    server.run()


if __name__ == "__main__":
    main()
