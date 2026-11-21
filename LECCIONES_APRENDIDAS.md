# Lecciones Aprendidas - Proyecto Gateway IA Local

## Resumen del Proyecto

Este proyecto implementa una infraestructura completa para ejecutar LLMs locales (Qwen3-8B) con una arquitectura robusta que incluye:
- **llama.cpp server** (puerto 8080) - Servidor de inferencia optimizado
- **Gateway Go v3.1** (puerto 8081) - Proxy inteligente con circuit breaker, load balancing, caché semántica
- **Harness Python** - Gestor de sesiones, thinking, tools, auto-mode
- **lai.py** - CLI completo para interacción con el modelo local

---

## Cambios Realizados y Lecciones Aprendidas

### 1. Configuración de llama.cpp (Puerto 8080)

#### Configuración Óptima para RTX 3070 8GB + Qwen3-8B-Q4_K_M:

```bash
./llama-server -m /home/kubuntu/models/Qwen3-8B-Q4_K_M.gguf \
  --host 0.0.0.0 --port 8080 -c 32768 -ngl 99 \
  --flash-attn on --cont-batching \
  --alias qwen3-8b \
  --reasoning-format none \
  -ctk f8_e4m3 -ctv f8_e4m3 \
  --spec-type ngram-simple --spec-ngram-simple-size-n 12 --spec-ngram-simple-size-m 48 \
  -b 4096 -ub 512
```

**Lecciones clave:**
- **CRÍTICO**: `--reasoning-format none` es OBLIGATORIO. Con `--reasoning-format deepseek`, Qwen3 se queda en modo thinking infinito y nunca escribe en `content`.
- **FP8 KV Cache** (`-ctk f8_e4m3 -ctv f8_e4m3`): Ahorra ~50% VRAM en KV cache sin pérdida de calidad.
- **Context Checkpoints** (`--ctx-checkpoints 8`): Permite contextos largos sin OOM.
- **Speculative Decoding ngram** (`--spec-type ngram-simple`): 2-3x velocidad sin VRAM extra.
- **Flash Attention + Continuous Batching**: Obligatorios para rendimiento.
- **Context 32K** (`-c 32768`): Necesario para tareas complejas.
- **GPU Layers**: `-ngl 99` (todas las capas en GPU) para RTX 3070 8GB.

### 2. Gateway Go v3.1 - Arquitectura de Producción

#### Características Implementadas:
- **Circuit Breaker** (Closed/Open/HalfOpen) con thresholds configurables
- **Load Balancing Ponderado** con awareness de circuit breaker
- **Retry con Exponential Backoff** en fallback chain
- **Semantic Cache** con cosine similarity (threshold 0.95)
- **LRU Cache** (5000 entries, TTL 1h)
- **Prometheus Metrics** (`/metrics`)
- **Health/Ready endpoints**
- **Request/Response Compression** (gzip)

#### Circuit Breaker States:
```
Closed (Normal) → [5 failures] → Open → [30s timeout] → HalfOpen [3 successes] → Closed
```

#### Load Balancing Score:
```
score = weight × statePenalty × loadPenalty
statePenalty: Closed=1.0, HalfOpen=0.5, Open=0 (skip)
loadPenalty = 1.0 / (1.0 + requests/100)
```

### 3. lai.py - CLI Completo

#### Comandos Implementados:
```bash
lai chat "prompt" [--stream/--no-stream] [--thinking/--no-thinking] [--temp N] [--max-tokens N]
lai chat -i                              # REPL interactivo
lai generate "prompt"                    # Generación simple
lai code "descripción" archivo.py [--auto] [--yolo]
lai think "problema"                     # Modo thinking
lai auto "objetivo"                      # Modo autónomo con tools
lai run script.py                        # Ejecuta script
lai serve --port 8080                    # Servidor API
lai status                               # Estado servicios
lai models                               # Lista modelos
lai tools                                # Lista tools
lai auto "objetivo"                      # Modo autónomo con tools
lai run script.py --yolo                 # Ejecución sin confirmaciones
```

#### Opciones globales:
```bash
-m, --model          Modelo a usar (default: qwen3-8b)
-u, --base-url       URL base API (default: http://localhost:8081/v1)
-t, --temp           Temperatura (default: 0.7)
--max-tokens         Max tokens (default: 4096)
--stream / --no-stream
--thinking / --no-thinking
--tools / --no-tools
--auto               # Modo autónomo
--yolo               # Sin confirmaciones
-v, -vv              Verbose
-q, --quiet          # Modo silencioso
```

### 3.1 Fixes Críticos en lai.py

**Problemas corregidos:**
1. **Dual `_chat` methods** - Eliminado duplicado
2. **Syntax Error** - `except:` sin `Exception` → `except Exception:`
3. **Streaming buffer** - Buffer correctamente inicializado
4. **Thinking mode** - Prefijo `<thinking>\n...\n</thinking>\n\n` funciona correctamente
5. **Streaming response** - Buffer correcto en streaming mode
6. **Non-streaming fallback** - Manejo correcto de respuesta no-streaming

### 4. Harness Python (`harness.py`)

#### Componentes:
- **SessionManager**: Persistencia JSON de sesiones
- **ContextManager**: Truncamiento inteligente (preserva último user message)
- **ThinkingManager**: Extracción de `<thinking>`, `问题...回答`, `|begin_thinking|`
- **ToolRegistry**: 7 built-in tools (read, write, list, run, grep, python, patch)
- **AutoMode**: Loop autónomo con max_iterations
- **ThinkingManager**: Extrae `<thinking>`, `问题...回答`, `|begin_thinking|`

#### Tools Built-in:
| Tool | Descripción |
|------|-------------|
| read_file | Leer archivo |
| write_file | Escribir archivo (crea dirs) |
| list_dir | Listar directorio |
| run_command | Ejecutar shell command |
| grep | Buscar patrón en archivos |
| python_eval | Evaluar código Python |
| apply_patch | Aplicar unified diff |

### 5. Gateway Go v3.1 (puerto 8081)

#### Características:
- **Circuit Breaker**: Closed/Open/HalfOpen con thresholds configurables
- **Load Balancing**: Weighted + Circuit Breaker awareness
- **Semantic Cache**: Cosine similarity (threshold 0.95)
- **LRU Cache**: 5000 entries, TTL 1h
- **Speculative Decoding**: ngram-simple (2-3x speedup)
- **Retry + Exponential Backoff**: En fallback chain
- **Prometheus Metrics**: `/metrics`
- **Health/Ready endpoints**: `/health`, `/ready`

### 5. llama.cpp Server Optimizado (Puerto 8080)

```bash
./llama-server -m /home/kubuntu/models/Qwen3-8B-Q4_K_M.gguf \
  --host 0.0.0.0 --port 8080 -c 32768 -ngl 99 \
  --flash-attn on --cont-batching \
  --alias qwen3-8b \
  --reasoning-format none \
  -ctk f8_e4m3 -ctv f8_e4m3 \
  --ctx-checkpoints 8 --checkpoint-min-step 4096 \
  --spec-type ngram-simple --spec-ngram-simple-size-n 12 --spec-ngram-simple-size-m 48 \
  -b 4096 -ub 512
```

**Parámetros clave:**
| Flag | Valor | Por qué |
|------|-------|---------|
| `-c 32768` | Contexto 32K | Tareas complejas |
| `-ngl 99` | Todas capas GPU | RTX 3070 8GB soporta Q4_K_M completo |
| `--flash-attn on` | Flash Attention | -30% memoria, +20% velocidad |
| `--cont-batching` | Continuous batching | +40% throughput |
| `-ctk f8_e4m3 -ctv f8_e4m3` | FP8 KV Cache | -50% VRAM KV cache |
| `--ctx-checkpoints 8` | Context checkpoints | Contexto largo sin OOM |
| `--spec-type ngram-simple` | Speculative decoding | 2-3x speedup |
| `--reasoning-format none` | **CRÍTICO** | Evita thinking infinito |

---

## Problemas Encontrados y Soluciones

### 1. Qwen3 Thinking Infinito
**Problema**: Qwen3 se queda en `<thinking>` infinito, nunca escribe en `content`.
**Causa**: `--reasoning-format deepseek` en llama.cpp
**Solución**: `--reasoning-format none` (o omitir el flag)

### 2. Streaming Response Buffer
**Problema**: `content_buffer` no definido en streaming mode.
**Fix**: Inicializar `content_buffer = []` antes del loop.

### 3. Streaming Response Buffer
**Problema**: `content_buffer` no definido en non-streaming.
**Fix**: Inicializar `content_buffer = []` antes del loop.

### 4. Duplicate `_chat` Methods
**Problema**: Dos definiciones de `_chat` en LAI class.
**Fix**: Eliminar duplicado, mantener solo la versión con streaming correcto.

### 5. Streaming Buffer Variable
**Problema**: `content_buffer` no definido en non-streaming path.
**Fix**: Inicializar `content_buffer = []` antes del if/else.

### 6. Chat Command Lambda
**Problema**: `lambda a: chat_cmd(LAI(), a)` pasa 2 args a función de 1 param.
**Fix**: `chat_cmd(args)` recibe solo `args`, crea `LAI()` internamente.

### 6. Subparser Lambdas
**Problema**: Lambdas crean nueva instancia `LAI()` cada vez.
**Fix**: Cambiar `lambda a: chat_cmd(LAI(), a)` → `lambda a: chat_cmd(a)` y que `chat_cmd` cree `LAI()` internamente.

---

## Configuración Óptima Actual (v0.0.2)

### llama.cpp (8080):
```bash
./llama-server -m /home/kubuntu/models/Qwen3-8B-Q4_K_M.gguf \
  --host 0.0.0.0 --port 8080 -c 32768 -ngl 99 \
  --flash-attn on --cont-batching --alias qwen3-8b \
  --reasoning-format none \
  -ctk f8_e4m3 -ctv f8_e4m3 \
  --ctx-checkpoints 8 --checkpoint-min-step 4096 \
  --spec-type ngram-simple --spec-ngram-simple-size-n 12 --spec-ngram-simple-size-m 48 \
  -b 4096 -ub 512
```

### Gateway Go (8081):
```bash
cd /home/kubuntu/curso-ia/modelo-local/gateway-go && ./gateway
```

### Gateway Python Fallback (8081):
```bash
cd /home/kubuntu/curso-ia/modelo-local/gateway-go && python3 gateway.py
```

### Ollama (fallback):
```bash
ollama serve  # Puerto 11434
```

### lai.py CLI:
```bash
cd /home/kubuntu/curso-ia/modelo-local
python3 lai.py chat "hola mundo" --max-tokens 100
python3 lai.py code "calculator class" calc.py --yolo
python3 lai.py auto "create REST API with FastAPI" --yolo
lai status
```

---

## Performance Actual (v0.0.2)

| Métrica | Valor |
|---------|-------|
| Throughput | ~71 tok/s |
| VRAM Used | ~6 GB / 8 GB |
| Latencia p50 | ~14ms/token |
| Context Window | 32K tokens |
| Speculative Speedup | ~2-3x |

---

## Próximos Pasos (Roadmap)

### Fase 1: Confiabilidad
- [ ] Circuit breaker metrics export
- [ ] Retry con exponential backoff mejorado
- [ ] Fallback chain completo (llama.cpp → Ollama → Cloud)
- [ ] Request/response compression (gzip)

### Fase 2: Caché Inteligente
- [ ] Semantic cache con embeddings reales (sentence-transformers)
- [ ] Cache warming on startup
- [ ] Cache analytics dashboard

### Fase 3: Routing Avanzado
- [ ] Weighted load balancing mejorado
- [ ] Conditional routing (model selection)
- [ ] Request prioritization

### Fase 4: Enterprise
- [ ] Virtual keys / RBAC
- [ ] Guardrails (PII, content filter)
- [ ] Audit logging
- [ ] Multi-tenancy

---

## Comandos de Verificación Rápida

```bash
# Health checks
curl -s http://127.0.0.1:8080/health   # llama.cpp
curl -s http://127.0.0.1:8081/health   # Gateway Go
curl -s http://127.0.0.1:11434/api/tags # Ollama

# Test chat
curl -s -X POST http://127.0.0.1:8081/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"qwen3-8b","messages":[{"role":"user","content":"hola"}],"max_tokens":50}' | jq -r .choices[0].message.content

# lai CLI
python3 lai.py chat "hola" --max-tokens 50
python3 lai.py code "calculator class" calc.py --yolo
python3 lai.py auto "create REST API with FastAPI" --yolo
```

---

## Archivos Clave del Proyecto

| Archivo | Descripción |
|---------|-------------|
| `lai.py` | CLI principal (593 líneas) |
| `harness.py` | Harness Python (742 líneas) |
| `gateway-go/` | Gateway Go v3.1 |
| `gateway.py` | Gateway Python fallback |
| `harness.py` | Harness completo con tools |
| `chess.py` | Ajedrez generado por Qwen3-8B |
| `racing.py` | Juego carreras terminal |
| `tui_app.py` | TUI con curses |
| `project_generator_tui.py` | Generador de proyectos TUI |
| `gateway-go/` | Gateway Go v3.1 (source) |
| `gateway-go/gateway` | Binario compilado |
| `gateway-go/config.yaml` | Configuración gateway |
| `CHANGELOG.md` | Historial de cambios |
| `VERSION` | v0.0.2 |

---

*Última actualización: 2026-11-20 | Versión: v0.0.2*