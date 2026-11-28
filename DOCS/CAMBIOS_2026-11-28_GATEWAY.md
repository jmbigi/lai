# Cambios 2026-11-28 — gateway y `lai serve` activos, contexto real

Registro según **Regla 10**. Continúa `DOCS/CAMBIOS_2026-11-28_MUESTRA62.md`.

## 0. Punto de partida

| Comprobación | Resultado inicial |
|---|---|
| Gateway (8081) | ❌ parado; 4 tests de integración *skipped* |
| `lai serve` (8082) | ❌ parado; 3 tests de integración *skipped* |
| `/health` del gateway | `max_context: 32768` mientras llama.cpp corría con **16384** |
| Timeout de los tests de modelo | 30 s fijos → fallos intermitentes con la GPU ocupada |

## 1. El gateway anuncia el contexto real del upstream

**ANTES:** `context.max_tokens: 32768` salía del YAML. Con llama.cpp arrancado a
16384, el gateway aceptaba peticiones que el upstream no podía atender y
`/v1/context/analyze` respondía `fits_in_context: true` a prompts imposibles.

**DESPUÉS:** al arrancar, `detectUpstreamContext()` consulta
`{llama_cpp}/props` y usa `default_generation_settings.n_ctx`; si no responde,
conserva el valor del YAML. Lo registra en el log:

```
context: using upstream n_ctx=16384 (config said 32768)
```

**Verificación:** `GET /health` → `"max_context":16384`, coherente con
`/props` de llama.cpp.

## 2. Gateway y `lai serve` en marcha

- `./scripts/start-gateway.sh --build` → gateway v3.1.0 en `127.0.0.1:8081`,
  health OK, chat a través del gateway OK.
- `python3 lai.py serve --port 8082` → `/health` OK.

Efecto medido: los **7 tests de integración** que antes se omitían por servicios
caídos (`test_gateway_*`, `test_serve_*`) ahora se ejecutan y pasan.

## 3. Timeout de los tests de modelo, configurable

**ANTES:** `urllib.request.urlopen(req, timeout=30)` fijo. Con un contenedor de
SWE-bench ocupando la GPU, una respuesta de 3 s tarda 60 y el test falla por
timeout (observado: 1 failed por `TimeoutError`).

**DESPUÉS:** `_model_timeout()` lee `LAI_TEST_TIMEOUT` (por defecto **180 s**).
Los cuatro puntos de llamada al modelo lo usan.

**Verificación:** con la GPU ocupada por la corrida de SWE, los 7 tests pasan.

## 4. Verificación

| Comprobación | Resultado |
|---|---|
| `go build` / `go vet` | ✅ |
| Gateway `/health` | ✅ `max_context` = 16384 (real) |
| Chat vía gateway | ✅ |
| `lai serve` `/health` | ✅ |
| Tests `gateway or serve` | ✅ 7 passed |
| `ruff check .` | ✅ |
| `lai status` | ✅ llama.cpp Running, gateway Running |

## 5. Lo que NO se ha probado (Regla 6)

- **Carga concurrente sostenida** sobre el gateway: se probó una petición, no un
  benchmark de concurrencia con la GPU ocupada.
- **Ollama.** Sigue parado y sin modelos; el gateway arranca con
  `llama_cpp.weight = 0` (deshabilitado) y usa solo llama.cpp. No se probó el
  failover a Ollama.
- **`lai serve` en producción.** Es el CLI Python legado (en mantenimiento); se
  levantó para desbloquear sus tests, no como servidor recomendado.
