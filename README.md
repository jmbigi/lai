# Gateway IA Local - Proyecto Chess

## Descripción

Gateway IA Local es una infraestructura completa para ejecutar LLMs locales (Qwen3-8B) a través de una API compatible con OpenAI. El proyecto incluye:

- **llama.cpp server** (puerto 8080) - Servidor de inferencia optimizado con FP8 KV cache, speculative decoding, context checkpoints
- **Gateway Go v3.1** (puerto 8081) - Proxy inteligente con circuit breaker, load balancing, caché semántica, compresión, filtrado de ruido
- **Harness Python** - Gestor de sesiones, thinking extraction, tool calling, auto-mode
- **OpenAPI 3.1 Spec** - Especificación completa compatible con OpenAI API 2.3.0

## Misión y Principio Fundacional (Regla 0)

**lai existe para impulsar el uso de IAs locales, abiertas y soberanas.**

- **Open source first:** Modelos, herramientas y licencias abiertas.
- **Sin dependencia de claves/API keys:** El usuario no necesita cuentas en la nube, suscripciones ni tokens externos.
- **Sin dependencia de cuentas en la nube:** Inferencia, almacenamiento y procesamiento ocurren localmente por defecto.
- **Software gratuito y de calidad:** Modelos abiertos (Qwen, Llama, Gemma) servidos localmente con llama.cpp/Ollama.
- **Privacidad por diseño:** Código y datos del usuario nunca salen de su máquina salvo decisión explícita.

> **Regla 0:** *Cualquier feature, arquitectura o dependencia que introduzca dependencia obligatoria de cloud, API keys, cuentas externas o software propietario será rechazada salvo aprobación explícita y documentada del operador.*

## Reglas del Proyecto

1. **Seguir las reglas de https://github.com/jmbigi/thinker/**
2. **Seguir las reglas de https://github.com/jmbigi/better-ai/**
3. **Seguir las reglas de https://github.com/jmbigi/better-project/**
4. **Todo el código fuente debe ser generado por LLMs locales, sin excepciones**
5. **Las IAs, LLMs y modelos cloud sólo pueden configurar y/o generar scripts genéricos, no deben reparar ni generar código específico**
6. **Honestidad 100%**
7. **Exactitud 99%**
8. **El proyecto chess debe ser generado con LLMs y harness locales**

## Arquitectura

```
┌─────────────┐     ┌──────────────────┐     ┌────────────────────┐
│   Cliente   │────▶│  Gateway Go 8081 │────▶│  llama.cpp 8080    │
│ (Crush/curl)│     │  Circuit Breaker │     │  FP8 KV Cache      │
│             │     │  Load Balancing  │     │  Speculative Dec.  │
└─────────────┘     │  Semantic Cache  │     │  Context Checkpt.  │
                    │  Compression     │     │  Speculative Dec.  │
                    └──────────────────┘     └────────────────────┘
                              │
                              ▼
                      ┌────────────────────┐
                      │   Ollama 11434     │
                      │   (Fallback)       │
                      └────────────────────┘
```

## Inicio Rápido

```bash
# 1. Iniciar llama.cpp server
cd /home/kubuntu/llama.cpp-bin/llama-b12567
export LD_LIBRARY_PATH=.:$LD_LIBRARY_PATH
nohup ./llama-server -m /home/kubuntu/models/Qwen3-8B-Q4_K_M.gguf \
  --host 0.0.0.0 --port 8080 -c 32768 -ngl 99 \
  --flash-attn on --cont-batching \
  -ctk f8_e4m3 -ctv f8_e4m3 \
  --ctx-checkpoints 8 --checkpoint-min-step 4096 \
  --spec-type ngram-simple --spec-ngram-simple-size-n 12 --spec-ngram-simple-size-m 48 \
  -b 4096 -ub 512 --alias qwen3-8b --reasoning-format none \
  > /tmp/llama.log 2>&1 &

# 2. Iniciar Gateway Go
cd /home/kubuntu/curso-ia/modelo-local/gateway-go
nohup ./gateway > /tmp/gateway.log 2>&1 &

# 3. Ollama (fallback)
ollama serve &

# Verificar
sleep 5
curl -s http://127.0.0.1:8080/health && echo "llama.cpp OK"
curl -s http://127.0.0.1:8081/health && echo "Gateway OK"
curl -s http://127.0.0.1:11434/api/tags && echo "Ollama OK"
```

## Endpoints Principales

| Endpoint | Método | Descripción |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/ready` | GET | Readiness probe |
| `/v1/models` | GET | Lista modelos |
| `/v1/chat/completions` | POST | Chat completion (stream + non-stream) |
| `/v1/chat/completions/stream` | POST | Streaming SSE |
| `/v1/chat/completions/compress` | POST | Comprimir prompt |
| `/v1/context/analyze` | POST | Analizar contexto |
| `/v1/batch` | POST | Batch processing |
| `/v1/embeddings` | POST | Embeddings |
| `/v1/cache/stats` | GET | Estadísticas caché |
| `/v1/cache/clear` | POST | Limpiar caché |
| `/metrics` | GET | Prometheus metrics |
| `/v1/metrics` | GET | JSON metrics |

## Configuración Crush

```json
{
  "providers": {
    "llamacpp": {
      "type": "completions",
      "base_url": "http://localhost:8080/v1",
      "models": [{
        "id": "qwen3-8b",
        "name": "Qwen3-8B Directo",
        "context_window": 32768,
        "default_max_tokens": 8192,
        "tool_call": true
      }]
    },
    "gateway": {
      "type": "completions",
      "base_url": "http://localhost:8081/v1",
      "models": [{
        "id": "qwen3-8b",
        "name": "Qwen3-8B via Gateway",
        "context_window": 16384,
        "default_max_tokens": 8192,
        "tool_call": true
      }]
    }
  }
}
```

## Uso con Crush

```bash
# Directo a llama.cpp (RECOMENDADO)
export PATH=$PATH:~/go/bin
crush run -m llamacpp/qwen3-8b --quiet "/no_think Genera código Python..."

# Via gateway (si funciona)
crush run -m gateway/qwen3-8b --quiet "..."
```

## Harness Python

```python
import harness
import asyncio

h = harness.Harness()
h.enable_auto_mode(max_iterations=5)

# Chat simple
h.add_message("user", "Hola")
context = h.get_context()

# Tools
result = asyncio.run(h.call_tool('run_command', {'command': 'ls -la'}))
print(result.result)

# Auto-mode
result = asyncio.run(h.run_auto("Crear un archivo hello.py", "En /home/user/proyecto"))
```

## Tools Disponibles

| Tool | Descripción |
|------|-------------|
| `read_file` | Leer archivos |
| `write_file` | Escribir archivos |
| `list_dir` | Listar directorios |
| `run_command` | Ejecutar comandos shell |
| `grep` | Buscar patrones en archivos |
| `python_eval` | Evaluar código Python |

## Optimizaciones llama.cpp

| Optimización | Flag | Impacto |
|-------------|------|---------|
| FP8 KV Cache | `-ctk f8_e4m3 -ctv f8_e4m3` | -50% VRAM KV cache |
| Context Checkpoints | `--ctx-checkpoints 8` | Contexto largo sin OOM |
| Flash Attention | `--flash-attn on` | -30% memoria, +20% velocidad |
| Continuous Batching | `--cont-batching` | +40% throughput |
| Speculative Decoding | `--spec-type ngram-simple` | 2-3x tokens/seg |
| GPU Layers | `-ngl 99` | Todas capas en GPU |
| Batch Sizes | `-b 4096 -ub 512` | Optimal para RTX 3070 |

## Comandos Útiles

```bash
# Health checks
curl -s http://127.0.0.1:8080/health && echo "llama.cpp OK"
curl -s http://127.0.0.1:8081/health && echo "Gateway OK"
curl -s http://127.0.0.1:11434/api/tags && echo "Ollama OK"

# Métricas
curl -s http://127.0.0.1:8081/metrics  # Prometheus
curl -s http://127.0.0.1:8081/v1/metrics  # JSON

# Caché
curl -s http://127.0.0.1:8081/v1/cache/stats
curl -X POST http://127.0.0.1:8081/v1/cache/clear

# Test rápido
curl -s -X POST http://127.0.0.1:8081/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"qwen3-8b","messages":[{"role":"user","content":"/no_think def hola(): print(1)"}],"max_tokens":50}'
```

## Documentación

| Archivo | Descripción |
|---------|-------------|
| `DOCUMENTACION_GATEWAY_LOCAL.md` | Documentación completa del gateway |
| `MEJORAS_IMPLEMENTADAS.md` | Detalle de mejoras implementadas |
| `DOCUMENTACION_FINAL.md` | Documentación final del proyecto |
| `OPTIMIZACION_PARAMETROS.md` | Parámetros de optimización |
| `DOCUMENTACION_REGLAS.md` | Reglas del proyecto |
| `openapi.yaml` | Especificación OpenAPI 3.1 |

## Reglas del Proyecto

1. Seguir las reglas de https://github.com/jmbigi/thinker/
2. Seguir las reglas de https://github.com/jmbigi/better-ai/
3. Seguir las reglas de https://github.com/jmbigi/better-project/
4. **Todo el código fuente debe ser generado por LLMs locales, sin excepciones**
5. Las IAs, LLMs y modelos cloud sólo pueden configurar y/o generar scripts genéricos, no deben reparar ni generar código específico
6. Honestidad 100%
7. Exactitud 99%
8. El proyecto chess debe ser generado con LLMs y harness locales