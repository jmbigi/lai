# ANÁLISIS EXHAUSTIVO - Intentos de Generación de chess.py

## 1. OBJETIVO
Generar `chess.py` (juego de ajedrez completo en Python, un solo archivo) usando OpenCode con modelo local a través del gateway local con compresión de contexto, siguiendo las reglas del README.md (código generado por LLMs locales, Regla 4).

**Estado del entregable:** `/home/kubuntu/curso-ia/modelo-local/chess/chess.py` = **0 bytes, vacío**. No se ha escrito ni una línea de código Python ejecutable.

---

## 2. INFRAESTRUCTURA (TODA OPERATIVA)

| Componente | Puerto | Estado | Detalle |
|------------|--------|--------|---------|
| llama-server | 8080 | ✅ Activo | Qwen3-8B-Q4_K_M, 60 GPU layers, 16384 ctx, flash-attn |
| Gateway local | 8081 | ✅ Activo | Filtra system messages de Kimi, comprime contexto a 4096 tokens |
| Ollama | 11434 | ✅ Activo | qwen3:8b |
| OpenCode service | - | ⚠️ Activo | `opencode serve --service` se reinicia automáticamente |

Todos los servicios responden a health checks. La infraestructura local está lista.

---

## 3. INTENTOS REALIZADOS (TODOS FALLIDOS PARA GENERAR CÓDIGO)

### Intento 1: OpenCode con gateway local (varias ejecuciones)

**Comandos ejecutados:**
- `opencode --prompt "..." --auto`
- `opencode --standalone --prompt "..." --auto`

**Resultado:** Fallo sistemático con error:
```
Rate limit exceeded: free-models-per-day-high-balance
```

**Causa raíz:** El cliente OpenCode (Kimi Code v2.0.32) ignora la configuración de `~/.opencode/config.json` donde se especificó:
- `provider: openai-compatible`
- `model: qwen3-8b`
- `baseURL: http://localhost:8081/v1`
- `apiKey: dummy`

OpenCode se conecta directamente a **NVIDIA cloud** (Nemotron 3 Ultra) en lugar del gateway local en puerto 8081. No existe variable de entorno ni bandera de línea de comandos para forzar modo local-only en esta versión.

**Evidencia en los logs de salida:** Se veía "Nemotron 3 Ultra (free) · 1m 26s" y "Build · Nemotron 3 Ultra (free)" en la interfaz de terminal de OpenCode, confirmando conexión a cloud, no a localhost:8081.

**Conclusión:** OpenCode está roto para modelos locales en este entorno. No genera código a través del gateway.

---

### Intento 2: llama-server directo vía curl (varias ejecuciones)

**Comandos ejecutados:**
- `curl -X POST http://127.0.0.1:8080/v1/chat/completions` con `max_tokens: 3000, 8000, 10000, 15000`

**Resultado:** HTTP 200, pero **100% del contenido en `reasoning_content`, 0 en `content`**.

| max_tokens | reasoning_content | content | ¿Código Python? |
|------------|-------------------|---------|-----------------|
| 3000 | ~2296 tokens | 0 | ❌ No |
| 8000 | ~30,815 chars | 0 | ❌ No |
| 10000 | ~26,549 chars | 0 | ❌ No |
| 15000 | ~47,823 chars | 0 | ❌ No |

**Causa raíz:** llama-server se inició con el flag `--reasoning-format deepseek`. Esto hace que Qwen3 separe el pensamiento (`reasoning_content`) de la respuesta final (`content`). El modelo se queda en un bucle de razonamiento ("thinking") planeando el código, describiendo la estructura, analizando movimientos de piezas, pero **nunca escribe código ejecutable en el campo `content`**.

**Evidencia:** Los logs de `/tmp/llama-60.log` mostraban `n_gen = 26549`, `n_gen = 30815`, `n_gen = 47823` tokens generados, todos en modo reasoning, sin transición a generación de código.

**Conclusión:** Con `--reasoning-format deepseek`, Qwen3-8B no produce código utilizable en `content`.

---

### Intento 3: Ollama directo vía curl (2 ejecuciones)

**Comandos ejecutados:**
- `curl -X POST http://127.0.0.1:11434/api/chat` con modelo `qwen3:8b`, `num_predict: 6000` y `10000`

**Resultado:** Timeout después de 300 segundos sin respuesta. Ollama no devolvió contenido usable dentro del tiempo límite.

**Causa raíz:** Posiblemente el modelo qwen3:8b en Ollama también entra en modo razonamiento extensivo y no completa la generación dentro del timeout, o hay un problema de carga del modelo en Ollama.

**Conclusión:** Ollama no es una alternativa viable en este momento para generar código grande.

---

### Intento 4: Gateway con chunks pequeños

**Archivo creado:** `/home/kubuntu/curso-ia/modelo-local/generate_chess_chunks.py`

**Estrategia:** Dividir chess.py en 4 chunks pequeños (board, movement, check, game_loop), cada uno con `max_tokens` entre 1000-2000 para evitar el timeout de 120s del gateway.

**Resultado:** El script se ejecutó pero fue interrumpido por el límite de tiempo de la herramienta de shell. Además, incluso si se completara, cada chunk devolvería solo `<thinking>` sin código real debido al `reasoning_format deepseek` de llama-server.

**Conclusión:** La estrategia de chunks es válida pero inútil si el modelo no sale del modo thinking.

---

### Intento 5: Documentación intermedia (varios archivos creados)

**Archivos creados (no son el entregable):**
- `/home/kubuntu/curso-ia/modelo-local/ESTADO_ACTUAL.md`
- `/home/kubuntu/curso-ia/modelo-local/DOCUMENTACION_INTENTO.md`
- `/home/kubuntu/curso-ia/modelo-local/DOCUMENTACION_USUARIO_SOLICITUD.md`

**Propósito:** Rastrear progreso y estado. No generaron código para chess.py.

**Conclusión:** Documentar sin ejecutar no avanza el entregable.

---

## 4. CAUSA RAÍZ (RESUMEN)

Hay **dos problemas independientes** que bloquean la generación de chess.py:

### Problema A: OpenCode cliente → NVIDIA cloud
OpenCode no respeta `baseURL: http://localhost:8081/v1` en config.json. Siempre enruta a NVIDIA cloud (Nemotron 3 Ultra) y falla por rate limit. No hay workaround conocido en OpenCode v2.0.32 para forzar local-only.

### Problema B: llama-server → `--reasoning-format deepseek`
llama-server se inició con `--reasoning-format deepseek`, lo que hace que Qwen3-8B se quede en modo razonamiento infinito y nunca escriba código en el campo `content` de la respuesta OpenAI-compatible.

---

## 5. ESTADO DE ARCHIVOS CREADO (NO SON EL ENTREGABLE)

| Archivo | Ruta | Tamaño | Propósito |
|---------|------|--------|-----------|
| chess.py (OBJETIVO) | `/home/kubuntu/curso-ia/modelo-local/chess/chess.py` | **0 bytes** | Debería contener el código de ajedrez |
| generate_chess_chunks.py | `/home/kubuntu/curso-ia/modelo-local/generate_chess_chunks.py` | Script | Para generar en chunks (no ejecutado con éxito) |
| DOCUMENTACION_REGLAS.md | `/home/kubuntu/curso-ia/modelo-local/DOCUMENTACION_REGLAS.md` | 6474 bytes | Reglas del proyecto |
| DOCUMENTACION_INTENTO.md | `/home/kubuntu/curso-ia/modelo-local/DOCUMENTACION_INTENTO.md` | - | Documento de intentos |
| DOCUMENTACION_USUARIO_SOLICITUD.md | `/home/kubuntu/curso-ia/modelo-local/DOCUMENTACION_USUARIO_SOLICITUD.md` | - | Documento de solicitud del usuario |
| ESTADO_ACTUAL.md | `/home/kubuntu/curso-ia/modelo-local/ESTADO_ACTUAL.md` | - | Estado actual |

---

## 6. QUÉ SE NECESITA PARA RESOLVERLO (SIN EJECUTAR AÚN)

Para generar chess.py correctamente, se necesitaría resolver uno o ambos problemas:

### Opción A: Reiniciar llama-server SIN `--reasoning-format deepseek`
- Matar el proceso actual de llama-server (PID 561764)
- Iniciar de nuevo sin el flag `--reasoning-format deepseek`
- Qwen3 generaría todo en el campo `content` sin separación thinking/output
- Luego usar `curl` contra el gateway (puerto 8081) o directo a llama-server (puerto 8080) para generar el código
- Escribir la salida en `/home/kubuntu/curso-ia/modelo-local/chess/chess.py`

### Opción B: Arreglar OpenCode para forzar gateway local
- Investigar por qué OpenCode ignora `config.json` y conecta a NVIDIA cloud
- Buscar variable de entorno o bandera para forzar `baseURL: http://localhost:8081/v1`
- Si se logra, usar `opencode --prompt "..." --auto` para generar chess.py

### Opción C: Generar en chunks pequeños a través del gateway
- Una vez resuelto el problema de reasoning_format, dividir chess.py en 6-8 chunks
- Cada chunk con `max_tokens < 800` para evitar el timeout de 120s del gateway
- Combinar todos los chunks en el archivo final

### Opción D: Usar Ollama con formato diferente
- Probar Ollama con `format: json` o parámetros que desactiven el modo reasoning
- Generar código directamente en el campo `content`

---

## 7. CONCLUSIÓN HONESTA

**No se ha generado ni una línea de código Python en chess.py.** El archivo permanece en 0 bytes.

Se han realizado al menos 10 intentos diferentes entre OpenCode, llama-server directo, Ollama, y chunks a través del gateway. Todos fallaron por una de las dos causas raíz identificadas: OpenCode se va a NVIDIA cloud, o Qwen3 se queda en modo thinking sin escribir código.

La infraestructura local está perfectamente configurada y funcionando (llama-server 8080, gateway 8081, Ollama 11434). El problema está en la capa de cliente (OpenCode) y en el formato de salida del modelo (reasoning_format deepseek).

Para completar el tarea, es necesario reiniciar llama-server sin `--reasoning-format deepseek` y generar el código a través del gateway o directo a llama-server, escribiendo la salida en `/home/kubuntu/curso-ia/modelo-local/chess/chess.py`.