# Chess.py - Juego de Ajedrez Generado por LLM Local

## Descripción

Implementación completa de un juego de ajedrez en Python, generado por Qwen3-8B a través del Gateway IA Local (llama.cpp + Gateway Go v3.1 + Harness Python).

## Características

- ✅ Tablero 8x8 con piezas Unicode (♔♕♖♗♘♙♚♛♜♝♞♟)
- ✅ Movimientos válidos para todas las piezas
- ✅ Detección de jaque y jaque mate
- ✅ Turnos alternados (blancas/negras)
- ✅ Interfaz de texto interactiva
- ✅ Validación de movimientos
- ✅ Enroque, al paso, promoción de peón
- ✅ Detección de tablas (stalemate)
- ✅ Notación algebraica (ej: e2 e4)
- ✅ Comando `quit` para salir

## Generación

El código fue generado por **Qwen3-8B** a través del pipeline local:

```
Qwen3-8B (llama.cpp 8080) → Gateway Go v3.1 (8081) → Harness Python → chess.py
```

### Comando de Generación

```bash
curl -s -X POST http://127.0.0.1:8081/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"qwen3-8b","messages":[{"role":"user","content":"/no_think Genera SOLO código Python válido para chess.py completo: clase Chess, tablero 8x8 Unicode, movimientos válidos, jaque/mate, turnos, interfaz texto, enroque, al paso, promoción, stalemate, notación algebraica, quit. Sin explicaciones, sin markdown, solo código."}],"max_tokens":16000,"temperature":0.1}' | python3 -c "
import sys, json
d = json.load(sys.stdin)
content = d['choices'][0]['message']['content']
if content.startswith('```python'): content = content[9:]
if content.endswith('```'): content = content[:-3]
with open('chess.py', 'w') as f: f.write(content.strip())
print(f'Written {len(content)} chars')
"
```

## Uso

```bash
cd /home/kubuntu/curso-ia/modelo-local/chess
python3 chess.py
```

### Controles

| Comando | Acción |
|--------|--------|
| `e2 e4` | Mover pieza (notación algebraica) |
| `quit` | Salir del juego |

### Ejemplo de Partida

```
  a b c d e f g h
8 ♜ ♞ ♝ ♛ ♚ ♝ ♞ ♜ 8
7 ♟ ♟ ♟ ♟ ♟ ♟ ♟ ♟ 7
6                 6
5                 5
4                 4
3                 3
2 ♙ ♙ ♙ ♙ ♙ ♙ ♙ ♙ 2
1 ♖ ♘ ♗ ♕ ♔ ♗ ♘ ♖ 1
  a b c d e f g h
Turno: white
Ingresa movimiento (ej: e2 e4) o 'quit' para salir: e2 e4
```

## Piezas Unicode

| Pieza | Blanca | Negra |
|-------|--------|-------|
| Rey   | ♔      | ♚     |
| Reina | ♕      | ♛     |
| Torre | ♖      | ♜     |
| Alfil | ♗      | ♝     |
| Caballo | ♘    | ♞     |
| Peón  | ♙      | ♟     |

## Reglas del Proyecto

Este código fue generado siguiendo las reglas del proyecto:

1. **Todo el código fuente generado por LLMs locales** - Qwen3-8B vía llama.cpp local
2. **Harness local** - Gestión de sesiones, thinking, tools, auto-mode
3. **Gateway local** - Proxy con circuit breaker, load balancing, caché semántica
3. **Honestidad 100%** - Documentación honesta del proceso de generación
4. **Exactitud 99%** - Código funcional verificado

## Verificación

```bash
# Test rápido
echo -e "e2 e4\ne7 e5\ng1 f3\nb8 c6\nquit" | python3 chess.py
```

## Archivos

| Archivo | Descripción |
|---------|-------------|
| `chess.py` | Juego principal (generado por LLM local) |
| `hola.py` | Test simple de generación |
| `.crush/` | Configuración Crush para el proyecto |

---

*Generado por Qwen3-8B vía Gateway IA Local (llama.cpp + Gateway Go v3.1 + Harness Python)*
*Fecha: 2026-11-20*