#!/usr/bin/env python3
"""
Generador de chess.py en chunks a través del gateway local.
Cada chunk es pequeño para evitar timeouts del gateway (120s).
"""

import json
import urllib.request
import time

GATEWAY_URL = "http://127.0.0.1:8081/v1/chat/completions"
MODEL = "qwen3-8b"

CHUNKS = [
    {
        "name": "board",
        "prompt": "Eres un programador Python. Responde SOLO con código Python válido, sin explicaciones, sin markdown, sin razonamiento. Escribe una clase Board para ajedrez con: __init__ que crea tablero 8x8 con piezas Unicode (♔♕♖♗♘♙♚♛♜♝♞♟), método display() que imprime el tablero con coordenadas, método get_piece(pos) y set_piece(pos, piece). Solo código Python, nada más.",
        "max_tokens": 1000
    },
    {
        "name": "movement",
        "prompt": "Eres un programador Python. Responde SOLO con código Python válido, sin explicaciones, sin markdown, sin razonamiento. Escribe una función is_valid_move(board, from_pos, to_pos, color) que valide movimientos de ajedrez para todas las piezas: peón, torre, caballo, alfil, reina, rey. Solo código Python, nada más.",
        "max_tokens": 1500
    },
    {
        "name": "check",
        "prompt": "Eres un programador Python. Responde SOLO con código Python válido, sin explicaciones, sin markdown, sin razonamiento. Escribe funciones is_in_check(board, color) y is_checkmate(board, color) para detectar jaque y jaque mate en ajedrez. Solo código Python, nada más.",
        "max_tokens": 1500
    },
    {
        "name": "game_loop",
        "prompt": "Eres un programador Python. Responde SOLO con código Python válido, sin explicaciones, sin markdown, sin razonamiento. Escribe la función main() con el bucle principal de un juego de ajedrez de texto: mostrar tablero, pedir movimiento en notación algebraica (ej: e2 e4), validar, ejecutar, alternar turnos, detectar jaque mate y tablas, comando quit. Solo código Python, nada más.",
        "max_tokens": 2000
    }
]

def ask_gateway(prompt, max_tokens):
    data = {
        "model": MODEL,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": max_tokens,
        "temperature": 0.1,
        "stream": False
    }
    req = urllib.request.Request(
        GATEWAY_URL,
        data=json.dumps(data).encode("utf-8"),
        headers={"Content-Type": "application/json"},
        method="POST"
    )
    with urllib.request.urlopen(req, timeout=110) as resp:
        result = json.loads(resp.read())
    content = result.get("choices", [{}])[0].get("message", {}).get("content", "")
    reasoning = result.get("choices", [{}])[0].get("message", {}).get("reasoning_content", "")
    return reasoning + "\n" + content if reasoning else content

if __name__ == "__main__":
    all_code = []
    for chunk in CHUNKS:
        print(f"Generando chunk: {chunk['name']}...")
        code = ask_gateway(chunk["prompt"], chunk["max_tokens"])
        all_code.append(f"# === {chunk['name'].upper()} ===\n{code}\n")
        time.sleep(2)
    
    with open("/home/kubuntu/curso-ia/modelo-local/chess/chess.py", "w") as f:
        f.write("\n".join(all_code))
    
    print(f"\nchess.py generado: {sum(len(c) for c in all_code)} caracteres totales")