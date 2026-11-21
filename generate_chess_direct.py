#!/usr/bin/env python3
"""
Genera chess.py en chunks directamente contra llama-server (puerto 8080).
Evita el timeout del gateway.
"""

import json
import urllib.request
import time

CHUNKS = [
    ("board", "/no_think\nSolo código Python. Clase Chess con __init__: tablero 8x8 con piezas Unicode ♔♕♖♗♘♙♚♛♜♝♞♟, método display() imprime tablero con coords a-h 1-8, get_piece(pos), set_piece(pos, piece). Nada más.", 1500),
    ("movement", "/no_think\nSolo código Python. Método is_valid_move(self, start, end): valida movimientos peón, torre, caballo, alfil, reina, rey. Métodos auxiliares: is_valid_pawn_move, is_valid_rook_move, is_valid_knight_move, is_valid_bishop_move, is_valid_queen_move, is_valid_king_move. Usa self.get_piece(). Nada más.", 2000),
    ("check", "/no_think\nSolo código Python. Métodos is_in_check(self, color), is_checkmate(self), is_stalemate(self), is_attacked(self, start, end, color). Detecta jaque, jaque mate, tablas. Nada más.", 2000),
    ("special", "/no_think\nSolo código Python. Métodos: enroque (castle), al paso (en_passant), promoción peón (promote_pawn). Maneja castle_rights, enpassant. Nada más.", 1500),
    ("game_loop", "/no_think\nSolo código Python. Método run(self): bucle principal, muestra tablero, pide movimiento algebraico (ej e2 e4), valida, ejecuta, alterna turnos, detecta fin de juego, comando quit. algebraic_notation(pos) convierte coords. Nada más.", 2000),
]

def ask_llama(prompt, max_tokens):
    data = json.dumps({
        "model": "qwen3-8b",
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": max_tokens,
        "temperature": 0.1,
        "stream": False
    }).encode()
    req = urllib.request.Request(
        "http://127.0.0.1:8080/v1/chat/completions",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST"
    )
    with urllib.request.urlopen(req, timeout=180) as resp:
        result = json.loads(resp.read())
    content = result["choices"][0]["message"]["content"]
    # Strip markdown fences
    if content.startswith("```python"):
        content = content[9:]
    if content.endswith("```"):
        content = content[:-3]
    return content.strip()

if __name__ == "__main__":
    all_code = []
    for name, prompt, max_tokens in CHUNKS:
        print(f"Generating {name}...")
        code = ask_llama(prompt, max_tokens)
        all_code.append(f"# === {name.upper()} ===\n{code}\n")
        time.sleep(1)

    with open("/home/kubuntu/curso-ia/modelo-local/chess/chess.py", "w") as f:
        f.write("\n".join(all_code))
    print(f"chess.py generated: {sum(len(c) for c in all_code)} chars total")