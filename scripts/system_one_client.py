#!/usr/bin/env python3
"""Cliente System One para modelos de decisión locales.

Permite ejecutar juicios tipados (choice/noul/score) sobre modelos locales
que implementen el protocolo TypeSafe /v1/systemone (Ollama, llama.cpp,
EdgeJev, von, etc.).

Ejemplos de uso:
  # Noul (sí/no con probabilidad)
  python3 scripts/system_one_client.py --type noul \
    --state "Ticket to Ride es un juego de mesa para 2-5 jugadores" \
    --question "¿Es bueno para jugadores casuales?" \
    --endpoint http://localhost:11434/v1/systemone \
    --model nimble

  # Choice (seleccionar una opción)
  python3 scripts/system_one_client.py --type choice \
    --state "El código tiene un error de sintaxis en la línea 42" \
    --question "¿Cuál es la acción más adecuada?" \
    --options "revisar_sintaxis" "ignorar" "reescribir_modulo" \
    --endpoint http://localhost:11434/v1/systemone

  # Score (puntuación en escala ordinal)
  python3 scripts/system_one_client.py --type score \
    --state "Función de 200 líneas con 10 parámetros y 5 niveles de anidación" \
    --question "¿Cuál es la complejidad ciclomática?" \
    --scale 1-10 \
    --endpoint http://localhost:11434/v1/systemone
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any, Dict, List, Optional

import httpx


class SystemOneClient:
    """Cliente para modelos de decisión System One."""

    def __init__(self, endpoint: str = "http://localhost:11434/v1/systemone", model: str = "nimble") -> None:
        self.endpoint = endpoint.rstrip("/")
        self.model = model

    def _build_questions(
        self,
        qtype: str,
        question: str,
        options: Optional[List[str]] = None,
        scale: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Construye el diccionario de preguntas para la request."""
        if qtype == "choice":
            if not options:
                raise ValueError("choice requiere --options")
            return {
                question: {
                    "type": "choice",
                    "instructions": question,
                    "options": options,
                }
            }
        elif qtype == "noul":
            return {
                question: {
                    "type": "noul",
                    "instructions": question,
                }
            }
        elif qtype == "score":
            if not scale:
                scale = "1-10"
            return {
                question: {
                    "type": "score",
                    "instructions": question,
                    "scale": scale,
                }
            }
        else:
            raise ValueError(f"Tipo desconocido: {qtype}")

    def decide(
        self,
        state: str,
        qtype: str,
        question: str,
        options: Optional[List[str]] = None,
        scale: Optional[str] = None,
        timeout: float = 30.0,
    ) -> Dict[str, Any]:
        """Envía una request de decisión al servidor System One."""
        questions = self._build_questions(qtype, question, options, scale)

        payload = {
            "model": self.model,
            "state": state,
            "questions": questions,
        }

        try:
            with httpx.Client(timeout=timeout) as client:
                resp = client.post(
                    self.endpoint,
                    json=payload,
                    headers={"Content-Type": "application/json"},
                )
                resp.raise_for_status()
                data = resp.json()
        except httpx.HTTPStatusError as exc:
            return {
                "error": f"HTTP {exc.response.status_code}",
                "detail": exc.response.text[:500],
            }
        except Exception as exc:
            return {"error": str(exc)}

        return data

    def choice(
        self,
        state: str,
        question: str,
        options: List[str],
        timeout: float = 30.0,
    ) -> Dict[str, Any]:
        """Ejecuta una pregunta de tipo choice."""
        return self.decide(state, "choice", question, options=options, timeout=timeout)

    def noul(
        self,
        state: str,
        question: str,
        timeout: float = 30.0,
    ) -> Dict[str, Any]:
        """Ejecuta una pregunta de tipo noul (sí/no con probabilidad)."""
        return self.decide(state, "noul", question, timeout=timeout)

    def score(
        self,
        state: str,
        question: str,
        scale: str = "1-10",
        timeout: float = 30.0,
    ) -> Dict[str, Any]:
        """Ejecuta una pregunta de tipo score."""
        return self.decide(state, "score", question, scale=scale, timeout=timeout)


def _format_result(result: Dict[str, Any]) -> str:
    """Formatea el resultado para salida por consola."""
    if "error" in result:
        return f"ERROR: {result['error']} | {result.get('detail', '')}"

    lines: List[str] = []
    answers = result.get("answers", {})
    if not answers:
        # Formato alternativo: las respuestas están directamente en result
        answers = {k: v for k, v in result.items() if k not in ("model", "state")}

    for qid, answer in answers.items():
        if isinstance(answer, dict):
            atype = answer.get("type", "unknown")
            value = answer.get("value", answer)
            confidence = answer.get("confidence", answer.get("probability", "N/A"))
            lines.append(f"  {qid} [{atype}]: {value} (confianza: {confidence})")
        else:
            lines.append(f"  {qid}: {answer}")

    return "\n".join(lines) if lines else str(result)


def main() -> int:
    parser = argparse.ArgumentParser(description="Cliente System One para modelos de decisión locales")
    parser.add_argument("--endpoint", default="http://localhost:11434/v1/systemone", help="Endpoint del servidor System One")
    parser.add_argument("--model", default="nimble", help="Nombre del modelo (nimble, jev, von, etc.)")
    parser.add_argument("--type", required=True, choices=["choice", "noul", "score"], help="Tipo de decisión")
    parser.add_argument("--state", required=True, help="Estado/contexto para la decisión")
    parser.add_argument("--question", required=True, help="Pregunta a responder")
    parser.add_argument("--options", nargs="+", help="Opciones para type=choice")
    parser.add_argument("--scale", default="1-10", help="Escala para type=score (ej. 1-10)")
    parser.add_argument("--timeout", type=float, default=30.0, help="Timeout en segundos")
    parser.add_argument("--json", action="store_true", help="Salida en formato JSON")
    args = parser.parse_args()

    client = SystemOneClient(endpoint=args.endpoint, model=args.model)
    result = client.decide(
        state=args.state,
        qtype=args.type,
        question=args.question,
        options=args.options,
        scale=args.scale,
        timeout=args.timeout,
    )

    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(_format_result(result))

    return 0 if "error" not in result else 1


if __name__ == "__main__":
    sys.exit(main())
