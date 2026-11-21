# DOCUMENTACIÓN - Intento de Generación de chess.py

## Objetivo
Generar `chess.py` (juego de ajedrez completo en Python) usando OpenCode con modelo local a través de gateway + harness, siguiendo las reglas del README.md.

## Reglas del Proyecto (README.md)
1. Seguir reglas de thinker/better-ai/better-project
2. Todo el código fuente debe ser generado por LLMs locales, sin excepciones
3. Las IAs cloud solo pueden configurar scripts genéricos, no generar código específico
4. Honestidad 100%
5. Exactitud 99%
6. El proyecto chess debe ser generado con LLMs y harness locales

## Stack Tecnológico (todos activos)
| Servicio | Puerto | Estado | Modelo |
|----------|--------|--------|--------|
| llama-server | 8080 | ✅ Activo | Qwen3-8B-Q4_K_M |
| Gateway local | 8081 | ✅ Activo | Filtro + compresión |
| Ollama | 11434 | ✅ Activo | qwen3:8b |
| OpenCode service | - | ⚠️ Activo | Conecta a NVIDIA cloud |

## Archivos Creados
- `/home/kubuntu/curso-ia/modelo-local/ESTADO_ACTUAL.md` - Este documento
- `/home/kubuntu/curso-ia/modelo-local/chess/chess.py` - 0 bytes (vacío, objetivo)
- `/home/kubuntu/curso-ia/modelo-local/gateway.py` - Proxy con filtro y compresión
- `/home/kubuntu/curso-ia/modelo-local/harness.py` - Harness para sesiones
- `/home/kubuntu/.opencode/config.json` - Configurado para gateway local (baseURL: localhost:8081/v1, model: qwen3-8b)

## Problemas Encontrados

### 1. OpenCode conecta a NVIDIA cloud en lugar del gateway local
A pesar de configurar `baseURL: http://localhost:8081/v1` y `model: qwen3-8b` en config.json, OpenCode siempre intenta conectar a NVIDIA cloud (Nemotron 3 Ultra) y falla con:
```
Rate limit exceeded: free-models-per-day-high-balance
```

Intentos realizados:
- `opencode --prompt "..." --auto` → Falla (cloud rate limit)
- `opencode --standalone --prompt "..." --auto` → Falla (cloud rate limit)
- Matar servicio `opencode serve --service` → Se reinicia automáticamente

### 2. Qwen3-8B con --reasoning-format deepseek no genera código
llama-server iniciado con `--reasoning-format deepseek` separa pensamiento en `reasoning_content` y respuesta en `content`. El modelo se queda en modo thinking y nunca genera código real en `content`.

Respuestas obtenidas (llama-server directo, puerto 8080):
- max_tokens=3000: 2296 tokens generados, solo thinking
- max_tokens=8000: 30815 chars en reasoning_content, 0 en content
- max_tokens=10000: 26549 chars en reasoning_content, 0 en content
- max_tokens=15000: 47823 chars en reasoning_content, 0 en content

### 3. Gateway timeout (120s)
El gateway tiene un timeout de 120s. Qwen3-8B genera ~15 tokens/seg, así que 3000+ tokens exceden el tiempo límite.

## Enfoques Probados (todos fallidos para generar código)
1. ✅ OpenCode directo con gateway → Cloud rate limit
2. ✅ OpenCode standalone con gateway → Cloud rate limit
3. ✅ llama-server directo con max_tokens=3000-15000 → Solo thinking, sin código
4. ✅ Ollama directo → Timeout (>300s sin respuesta)
5. ✅ Gateway con chunks pequeños (max_tokens=800) → Solo thinking, sin código

## Próximos Pasos Recomendados

### Opción A: Arreglar OpenCode para usar gateway local
1. Investigar por qué OpenCode ignora config.json y conecta a cloud
2. Buscar variable de entorno para forzar local-only (ej: OPENCODE_DISABLE_CLOUD)
3. Configurar modelo correcto en el catálogo de OpenCode
4. Reintentar generación de chess.py

### Opción B: Modificar llama-server para no usar reasoning-format deepseek
1. Reiniciar llama-server sin `--reasoning-format deepseek`
2. Qwen3 generaría todo en `content` sin separación thinking/output
3. Reintentar generación de chess.py

### Opción C: Generar chess.py en chunks pequeños a través del gateway
1. Dividir el código en 6-8 chunks pequeños (cada uno < 800 tokens)
2. Generar cada chunk por separado a través del gateway
3. Combinar todos los chunks en chess.py
4. Verificar que el código sea funcional

### Opción D: Usar Ollama con formato diferente
1. Probar Ollama con `format: json` o sin sistema de reasoning
2. Generar código directamente en content

## Estado del Archivo chess.py
- Ruta: `/home/kubuntu/curso-ia/modelo-local/chess/chess.py`
- Tamaño: 0 bytes (vacío)
- Contenido: Ninguno aún

## Comandos Útiles
```bash
# Verificar servicios
ss -tlnp | grep -E "8080|8081|11434"

# Health check gateway
curl -s http://127.0.0.1:8081/health

# Health check llama-server
curl -s http://127.0.0.1:8080/health

# Listar modelos gateway
curl -s http://127.0.0.1:8081/v1/models

# Probar gateway (corto)
curl -s -X POST http://127.0.0.1:8081/v1/chat/completions \
  -d '{"model":"qwen3-8b","messages":[{"role":"user","content":"Hola"}],"max_tokens":20}'
```

## Logs Disponibles
- `/tmp/gateway.log` - Log del gateway
- `/tmp/llama-60.log` - Log de llama-server
- `/home/kubuntu/.local/share/opencode/shell/` - Logs de OpenCode