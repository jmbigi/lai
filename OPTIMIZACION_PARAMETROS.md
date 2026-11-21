# OPTIMIZACIÓN DE PARÁMETROS - Generación de chess.py

> **ESTADO: ✅ APLICADO Y VERIFICADO** - Los cambios se aplicaron exitosamente y chess.py fue generado (12,959 bytes, funcional).

## 1. PARÁMETROS ACTUALES Y PROBLEMAS

### llama-server (puerto 8080)
Comando actual:
```
./llama-server -m /home/kubuntu/models/Qwen3-8B-Q4_K_M.gguf \
  --host 0.0.0.0 --port 8080 -c 16384 -ngl 60 \
  --flash-attn on --cont-batching --alias qwen3-8b \
  --spec-type ngram-simple \
  --spec-ngram-simple-size-n 12 \
  --spec-ngram-simple-size-m 48 \
  --reasoning-format deepseek
```

| Parámetro | Valor actual | Problema | Valor recomendado |
|-----------|-------------|----------|-------------------|
| `--reasoning-format` | `deepseek` | ❌ Qwen3 se queda en thinking, no escribe en `content` | **Eliminar** o usar `default` |
| `-c` (contexto) | 16384 | OK para el modelo | 16384 (mantener) |
| `-ngl` (GPU layers) | 60 | OK para RTX 3070 8GB | 60 (mantener) |
| `--flash-attn` | on | OK | on (mantener) |
| `--cont-batching` | - | OK | Mantener |
| `--spec-type` | ngram-simple | OK para acelerar | Mantener |

**Problema crítico:** `--reasoning-format deepseek` hace que Qwen3 genere todo el razonamiento en `reasoning_content` y nada en `content`. Debe eliminarse.

---

### Gateway local (puerto 8081)
Archivo: `/home/kubuntu/curso-ia/modelo-local/gateway.py`

| Parámetro | Valor actual | Problema | Valor recomendado |
|-----------|-------------|----------|-------------------|
| `MAX_CONTEXT_TOKENS` | 4096 | ❌ Muy bajo para generar código completo | 8192 o 12288 |
| `timeout` (urlopen) | 120s | ❌ Insuficiente para 3000+ tokens a 15 tok/s | 300s |
| `temperature` por defecto | 0.7 | ❌ Demasiado alta para código | 0.2 |
| `max_tokens` por defecto | 2048 | ❌ Muy bajo para chess.py | 8000-12000 |

---

### OpenCode config
Archivo: `/home/kubuntu/.opencode/config.json`

| Parámetro | Valor actual | Problema | Valor recomendado |
|-----------|-------------|----------|-------------------|
| `model` | `qwen3-8b` | OK | `qwen3-8b` |
| `baseURL` | `http://localhost:8080/v1` | ✅ Correcto | Mantener |
| `maxTokens` | 8192 | OK | 16384 |
| `gpuLayers` | 99 | Informativo | Informativo |

---

## 2. CAMBIOS REQUERIDOS

### Cambio 1: Reiniciar llama-server SIN `--reasoning-format deepseek`

Este es el cambio más importante. Sin `--reasoning-format deepseek`, Qwen3 generará el código directamente en el campo `content` de la respuesta OpenAI-compatible.

Nuevo comando:
```
./llama-server -m /home/kubuntu/models/Qwen3-8B-Q4_K_M.gguf \
  --host 0.0.0.0 --port 8080 -c 16384 -ngl 60 \
  --flash-attn on --cont-batching --alias qwen3-8b \
  --spec-type ngram-simple \
  --spec-ngram-simple-size-n 12 \
  --spec-ngram-simple-size-m 48
```

### Cambio 2: Ajustar parámetros del gateway

En `gateway.py`:
- `MAX_CONTEXT_TOKENS`: 4096 → 8192
- `timeout` en `urllib.request.urlopen`: 120 → 300 segundos
- `temperature` por defecto: 0.7 → 0.2
- `max_tokens` por defecto: 2048 → 8000

### Cambio 3: Parámetros de la petición curl

Para generar chess.py:
- `max_tokens`: 8000-12000
- `temperature`: 0.1-0.2 (código necesita determinismo)
- `stream`: false

---

## 3. ESTIMACIÓN DE TOKENS PARA chess.py

Un archivo chess.py completo con todas las funcionalidades requiere aproximadamente:
- 300-500 líneas de código
- ~6,000-10,000 caracteres
- ~1,500-2,500 tokens de salida

Con `max_tokens: 8000` y `--reasoning-format` eliminado, debería ser suficiente.

---

## 4. VERIFICACIÓN POST-OPTIMIZACIÓN (COMPLETADA ✅)

Después de aplicar los cambios, se verificó:
1. ✅ `curl http://127.0.0.1:8080/health` → `{"status":"ok"}`
2. ✅ `curl http://127.0.0.1:8081/health` → `{"status":"ok"}`
3. ✅ Envío de petición con `max_tokens: 8000` y `temperature: 0.1` + prompt `/no_think`
4. ✅ Verificar que `content` contenga código Python real (def, class, import)
5. ✅ Escribir el resultado en `/home/kubuntu/curso-ia/modelo-local/chess/chess.py` (12,959 bytes)
6. ✅ Ejecutar `python3 chess.py` → **Juego funciona correctamente**

---

## 5. ARCHIVOS RELACIONADOS

- `/home/kubuntu/curso-ia/modelo-local/gateway.py` - Gateway con parámetros ajustados
- `/home/kubuntu/.opencode/config.json` - Config de OpenCode
- `/home/kubuntu/curso-ia/modelo-local/chess/chess.py` - **Archivo objetivo: 12,959 bytes, COMPLETO**
- `/home/kubuntu/curso-ia/modelo-local/generate_chess_chunks.py` - Script de generación por chunks