# ESTADO ACTUAL - Generación de chess.py

## Objetivo
Generar `chess.py` (juego de ajedrez completo en Python) usando LLM local a través de OpenCode + gateway + harness.

## Servicios (todos activos)
| Servicio | Puerto | Estado |
|----------|--------|--------|
| llama-server (Qwen3-8B-Q4_K_M) | 8080 | ✅ Activo (SIN reasoning_format deepseek) |
| Gateway local (filtro + compresión) | 8081 | ✅ Activo |
| Ollama (qwen3:8b) | 11434 | ✅ Activo |

## Problema principal (RESUELTO)
OpenCode (Kimi Code v2.0.32) **no funciona con modelos locales**. Siempre intenta conectar a NVIDIA cloud (Nemotron 3 Ultra) y falla con "Rate limit exceeded: free-models-per-day-high-balance".

**Causa raíz adicional (RESUELTA):** llama-server iniciado con `--reasoning-format deepseek` → Qwen3 se queda en modo thinking y no escribe código en `content`.

## Solución aplicada
1. **Reiniciar llama-server SIN `--reasoning-format deepseek`**
2. **Generar via gateway (puerto 8081) con prompt `/no_think` y `max_tokens=8000`, `temperature=0.1`**
3. **Limpiar y corregir código generado** (eliminar markdown, fix coordenadas, fix detección Unicode pieces, fix stalemate logic, fix parámetros faltantes)

## Resultado
✅ **chess.py GENERADO Y FUNCIONAL** (12,959 bytes)

### Verificación
```bash
cd /home/kubuntu/curso-ia/modelo-local/chess
echo -e "e2 e4\ne7 e5\ng1 f3\nb8 c6\nquit" | python3 chess.py
# → Juego se ejecuta correctamente con movimientos válidos
```

## Archivos relevantes
- `/home/kubuntu/curso-ia/modelo-local/chess/chess.py` → **12,959 bytes, COMPLETO**
- `/home/kubuntu/curso-ia/modelo-local/chess/README.md` → reglas del proyecto
- `/home/kubuntu/curso-ia/modelo-local/gateway.py` → proxy con filtro y compresión
- `/home/kubuntu/curso-ia/modelo-local/harness.py` → harness para sesiones