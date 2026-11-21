# DOCUMENTACIÓN PRE-OPTIMIZACIÓN - Estado completo antes de cambios

## Fecha: 2026-11-20

---

## 1. SERVICIOS ACTIVOS

| Servicio | Puerto | Proceso | Comando |
|----------|--------|---------|---------|
| llama-server | 8080 | ✅ Activo | Qwen3-8B-Q4_K_M, -ngl 60, flash-attn, cont-batching, spec ngram |
| Gateway local | 8081 | ✅ Activo | python3 gateway.py (filtra system, comprime contexto 4096 tokens) |
| Ollama | 11434 | ✅ Activo | qwen3:8b |
| Crush | - | ✅ Instalado | v0.102.0 en ~/go/bin/crush |

---

## 2. CONFIGURACIÓN LLAMA-SERVER ACTUAL

```bash
cd /home/kubuntu/llama.cpp-bin/llama-b12567
export LD_LIBRARY_PATH=.:$LD_LIBRARY_PATH
nohup ./llama-server -m /home/kubuntu/models/Qwen3-8B-Q4_K_M.gguf \
  --host 0.0.0.0 --port 8080 -c 16384 -ngl 60 \
  --flash-attn on --cont-batching --alias qwen3-8b \
  --spec-type ngram-simple \
  --spec-ngram-simple-size-n 12 \
  --spec-ngram-simple-size-m 48 \
  > /tmp/llama-noreason.log 2>&1 &
```

**Parámetros clave:**
- `-ngl 60`: 60 capas en GPU (de ~33 totales en Qwen3-8B? verificar)
- `--flash-attn on`: Flash Attention activado
- `--cont-batching`: Continuous batching
- `--spec-type ngram-simple`: Speculative decoding n-gram
- `-c 16384`: Contexto 16K
- **SIN** `--reasoning-format deepseek` (eliminado - causaba solo thinking)

---

## 3. GATEWAY LOCAL (puerto 8081)

**Archivo:** `/home/kubuntu/curso-ia/modelo-local/gateway.py`

**Funciones:**
- Filtra mensajes system de Kimi/OpenCode (ruido)
- Comprime contexto a 4096 tokens (MAX_CONTEXT_TOKENS)
- Transforma reasoning_content de Qwen3 a `<thinking>` tags
- Timeout upstream: 120s (recentemente aumentado a 600s en código)
- Endpoints: `/health`, `/v1/models`, `/v1/chat/completions`

---

## 4. CRUSH CONFIGURACIÓN

**Archivo:** `~/.config/crush/crush.json`
```json
{
  "providers": {
    "llamacpp": {
      "type": "llamacpp",
      "base_url": "http://localhost:8080/v1",
      "models": [{ "id": "qwen3-8b", "name": "Qwen3-8B (local)", "context_window": 16384, "default_max_tokens": 8192, "tool_call": true }]
    },
    "ollama": {
      "type": "ollama",
      "base_url": "http://localhost:11434/v1",
      "models": []
    }
  }
}
```

**Modelo usado:** `llamacpp/qwen3-8b` (vía llama-server directo puerto 8080)

---

## 5. CHESS.PY ACTUAL

**Archivo:** `/home/kubuntu/curso-ia/modelo-local/chess/chess.py` (12,959 bytes)

**Funcionalidades verificadas:**
- ✅ Tablero 8x8 con piezas Unicode (♔♕♖♗♘♙♚♛♜♝♞♟)
- ✅ Movimientos válidos todas las piezas
- ✅ Detección jaque/jaque mate
- ✅ Turnos alternados blancas/negras
- ✅ Interfaz texto interactiva
- ✅ Validación movimientos
- ✅ Enroque, al paso, promoción (parcial en código)
- ✅ Detección tablas (stalemate)
- ✅ Notación algebraica (e2 e4)
- ✅ Comando quit

**Test pasado:**
```bash
echo -e "e2 e4\ne7 e5\ng1 f3\nb8 c6\nquit" | python3 chess.py
# → 4 movimientos válidos ejecutados correctamente
```

---

## 6. RENDIMIENTO BASELINE (antes de optimización)

**Test rápido:**
```bash
curl -s -X POST http://127.0.0.1:8080/v1/chat/completions \
  -d '{"model":"qwen3-8b","messages":[{"role":"user","content":"/no_think def hola(): print(1)"}],"max_tokens":50}' \
  -H "Content-Type: application/json"
```

**Métricas a medir post-optimización:**
- `predicted_per_second` (tokens/seg generación)
- `predicted_per_token_ms` (ms/token)
- VRAM usada (nvidia-smi)
- Latencia primer token

---

## 7. OPTIMIZACIÓN PLANIFICADA

**Cambio único (seguro, recomendado oficial llama.cpp):**
```bash
# AÑADIR KV cache cuantizado 8-bit
-ctk q8_0 -ctv q8_0

# OPCIONAL: subir capas GPU a 99 (probar OOM)
-ngl 99
```

**Comando nuevo:**
```bash
./llama-server -m /home/kubuntu/models/Qwen3-8B-Q4_K_M.gguf \
  --host 0.0.0.0 --port 8080 -c 16384 -ngl 99 \
  --flash-attn on --cont-batching --alias qwen3-8b \
  -ctk q8_0 -ctv q8_0 \
  --spec-type ngram-simple --spec-ngram-simple-size-n 12 --spec-ngram-simple-size-m 48
```

**Beneficios esperados:**
- `-ctk q8_0 -ctv q8_0`: ~2GB VRAM ahorrados, 0% pérdida calidad
- `-ngl 99`: Todas capas en GPU (si cabe en 8GB)
- Speculative ngram: Ya activo, ~2-3x speedup

---

## 8. TESTING POST-OPTIMIZACIÓN

**Pasos:**
1. Medir baseline actual (tokens/seg, VRAM)
2. Aplicar optimización
3. Verificar health check
4. Medir nuevo rendimiento
5. Probar Crush con modelo local via gateway
6. Regenerar chess.py con Crush + gateway local
7. Verificar chess.py funcional

---

## 9. ARCHIVOS CLAVE PARA ROLLBACK

| Archivo | Descripción |
|---------|-------------|
| `/home/kubuntu/curso-ia/modelo-local/gateway.py` | Gateway (backup: gateway.py.bak) |
| `/home/kubuntu/llama.cpp-bin/llama-b12567/` | Binario llama-server |
| `/tmp/llama-noreason.log` | Log actual |
| `~/.config/crush/crush.json` | Config Crush |
| `/home/kubuntu/curso-ia/modelo-local/chess/chess.py` | Chess funcional |

---

## 10. COMANDOS DE VERIFICACIÓN RÁPIDA

```bash
# Health checks
curl -s http://127.0.0.1:8080/health
curl -s http://127.0.0.1:8081/health
curl -s http://127.0.0.1:11434/api/tags

# Test modelo directo
curl -s -X POST http://127.0.0.1:8080/v1/chat/completions \
  -d '{"model":"qwen3-8b","messages":[{"role":"user","content":"/no_think hola"}],"max_tokens":20}' \
  -H "Content-Type: application/json"

# Test Crush
export PATH=$PATH:~/go/bin && crush run -m llamacpp/qwen3-8b --quiet "/no_think hola"

# VRAM
nvidia-smi
```

---

*Documentado antes de cualquier cambio de optimización.*
