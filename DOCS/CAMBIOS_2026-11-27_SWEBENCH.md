# Cambios 2026-11-27 (tercera tanda) — llama.cpp en GPU y corrida SWE-bench

Registro según **Regla 10**. Continúa `CAMBIOS_2026-11-27_MEJORAS.md`.
Todo lo que aparece como medido se ejecutó en este equipo; lo que no se pudo
medir se declara al final (Regla 6).

## 0. Contexto

El equipo es un portátil con **RTX 3070 Laptop (8 GB)**, 16 hilos y 15 GB de
RAM. El 23 de noviembre llama.cpp funcionaba aquí con la configuración de
`/home/kubuntu/start-optimal.sh`; cuatro días después no había ningún servicio
levantado (`lai status`: llama.cpp, gateway y ollama parados) y el `doctor`
avisaba de que faltaba `~/.config/lai`. No había ningún proceso huérfano ni
puerto ocupado: simplemente no estaba arrancado.

## 1. llama.cpp: arrancado y verificado

**ANTES:** sin servicio. El script del repo (`scripts/start-llama.sh`) usa
`-ngl 99` y `-c 32768`, valores que en esta GPU no caben (el propio log de
noviembre mostraba `failed to fit params to free device memory`).

**DESPUÉS:** se arrancó con la configuración medida para esta GPU, con **dos
cambios deliberados** respecto de `start-optimal.sh`:

| Cambio | Motivo |
|---|---|
| `--host 127.0.0.1` en vez de `0.0.0.0` | El script original exponía el modelo a toda la red local. La Regla 0 y el endurecimiento del proyecto piden loopback; para el harness basta porque usa `--network host`. |
| log a `logs/llama-2026-11-27.log` | No sobrescribir `logs/llama.log`, que es la evidencia de la corrida del 23. |

Comando exacto (build 12567, `bed8ec2f8`):

```
llama-server -m ~/models/Qwen3-8B-Q4_K_M.gguf --host 127.0.0.1 --port 8080 \
  -c 18432 -ngl 50 --threads 16 --batch-size 2048 --ubatch-size 512 \
  --flash-attn on --cont-batching --load-mode mmap --alias qwen3-8b \
  --spec-type ngram-simple --spec-ngram-simple-size-n 12 \
  --spec-ngram-simple-size-m 48 --reasoning-format deepseek
```

**Verificación (no supuesta):**

| Comprobación | Resultado |
|---|---|
| `/health` | `{"status":"ok"}` |
| `/v1/models` | `qwen3-8b` |
| Chat simple | `2+2=4`, 6 tokens de salida |
| **Tool calling nativo** | devuelve `tool_calls` con `finish_reason: tool_calls` |
| VRAM | 7399 / 8192 MiB (estable) |
| RAM | 3 GB usados, 11 disponibles |
| Throughput (486 peticiones del log) | prompt eval **p50 1109 t/s**; generación **p50 65 t/s** |

También se creó `~/.config/lai`, que era el único error del `lai doctor`.

## 2. Revisión de la aplicación en este equipo

| Comando | Resultado | Nota |
|---|---|---|
| `lai status` | ✅ | detecta llama.cpp en 8080 |
| `lai doctor` | ✅ | solo advertencias (gateway/ollama ausentes, opcional) |
| `lai chat "…"` | ✅ | responde `OK` |
| `lai code "…" factorial.py --yolo` | ✅ | archivo válido, 93 caracteres |
| `lai auto` (tarea real) | ✅ | `read_file` → `edit` → tests en verde, sin pasos redundantes |
| `lai mcp --stdio` | ✅ | 13→15 herramientas, recursos y denegación de `../` |
| `lai mcp --port` (TCP) | ✅ | `tools/list` por socket |
| `lai repo-map / metrics / diagnose / baseline / memory / checkpoint` | ✅ | motores reales (tanda anterior) |
| `lai verify` | ✅ | `SuccessWithDeclaredGaps` en un proyecto Python |
| `lai bench all` | ⚠ | **eran resultados mock presentados como éxito** (ver §3) |
| `lai bench-swe --quick` | ✅ | remite a `cargo test --test swe_bench` |
| `lai tui` sin TTY | ⚠ | `Error: No such device or address (os error 6)` |
| `python3 lai.py status` (legado) | ✅ | detecta los servicios |

## 3. `lai bench`: dejaba de ser honesto

**ANTES:** `lai bench all` imprimía «✅ … Success: true … 100.0 %» con
`MockBenchmarkAdapter`. Latencia 0 ms, memoria 0 MB: son cifras inventadas por
el mock, presentadas como si fueran una medición. Viola la Regla 6.

**DESPUÉS:** se avisa al principio y cada resultado lleva el prefijo `⚠ MOCK`,
con el texto «NO miden nada del sistema (Regla 6)». Los adaptadores reales
siguen sin estar registrados; el comando ya no puede confundirse con un
benchmark.

## 4. Agente: tres correcciones que salieron de mirar las corridas

### 4.1 Escalada por presupuesto de turnos

**ANTES:** en `astropy-12907`, qwen3-8b gastó 8 iteraciones leyendo archivos y
**nunca editó**. Nada en el bucle le decía que se le acababa el tiempo.

**DESPUÉS:** al pasar la mitad del presupuesto sin ningún cambio, se le inyecta
un mensaje con los turnos que quedan y la orden explícita de aplicar el arreglo
con `edit` o `write_file`. Una sola vez. Medido: en la corrida final 8/10
instancias produjeron parche (antes 5/10).

### 4.2 El bucle se detenía tarde

**ANTES:** en `pytest-10081` el modelo repitió **9 veces** el mismo `read_file`;
el bucle lo detectaba y saltaba la ejecución, pero seguía girando hasta agotar
las 30 iteraciones.

**DESPUÉS:** a partir de la séptima repetición idéntica se rompe el bucle con
`⛔ El modelo repite la misma llamada sin avanzar`. Ahorra tiempo y deja claro
en el log por qué terminó.

### 4.3 Sin nudges redundantes tras editar

**ANTES:** tras un `edit` correcto, el modelo decía «listo» y el bucle lo
empujaba a seguir actuando: repetía el mismo `write_file` dos veces.

**DESPUÉS:** si ya hubo una edición, la respuesta en prosa se acepta como final.

### 4.4 Temperatura explícita

`LlmClient` tenía `temperature: 0.7` fijo. Ahora es configurable
(`with_temperature`, `LAI_TEMPERATURE`) y `auto` usa **0.2** por defecto, para
que el mismo estado produzca el mismo camino. No se afirma que 0.2 sea mejor:
con una sola corrida por configuración no hay evidencia (ver §6).

## 5. Herramientas: dos fallos que el benchmark destapó

### 5.1 `edit` podía destruir un archivo

**ANTES:** en `pytest-5809`, qwen3-8b llamó `edit` con `old_string` = todo el
archivo y `new_string` de 5 líneas: `src/_pytest/pastebin.py` pasó de 71 a 5
líneas. `write_file` tenía guarda anti-destrucción; `edit` no.

**DESPUÉS:** `edit` aplica la misma guarda (40+ líneas y menos del 25 % se
rechaza salvo `allow_shrink: true`). Test nuevo con el caso real.

### 5.2 `grep`/`find_files` con rutas inexistentes

**ANTES:** devolvían `No matches`, así que el modelo creía que el símbolo no
existía y probaba rutas inventadas (`/testbed/src/_pytest/terminal/pytest/…`,
10 iteraciones en `pytest-5809`).

**DESPUÉS:** devuelven un error explícito («la ruta … no existe») con la
sugerencia de usar una ruta real. Dos tests nuevos.

## 6. Harness SWE-bench: tres correcciones

### 6.1 La traducción del base-url rompía Linux

**ANTES:** `container_base_url()` traducía siempre `127.0.0.1` →
`host.docker.internal`. En Linux con `--network host` el loopback del contenedor
**es** el del host, y ese nombre no resuelve sin `--add-host`. La corrida
anterior en este equipo dependía de un alias que aquí no existe.

**DESPUÉS:** `uses_host_network()` decide por plataforma y modo de red; en Linux
con red host se mantiene `127.0.0.1`. En bridge se traduce **y** se añade
`--add-host host.docker.internal:host-gateway`. Tests actualizados (6 casos).

### 6.2 `extract_patch` corrompía el diff

**ANTES:** `.strip()` sobre el texto entre marcadores borraba el espacio de una
línea de contexto vacía al final del hunk. `git apply` respondía
`corrupt patch at line 12` y la instancia se marcaba `apply_failed`. Pasó en
`astropy__astropy-13236`.

**DESPUÉS:** se quitan solo los saltos de línea del delimitador
(`strip("\r\n")`). Test de regresión con el caso exacto.

### 6.3 Datos del dataset incompletos

**ANTES:** `data/instances.jsonl` tenía 3 filas sin `image`, `eval_script` ni
`log_parser`; `evaluate.py` abortaba con «Datos incompletos».

**DESPUÉS:** se volvió a descargar `SWE-bench_Verified` por la API pública
(500 instancias, 17 campos) y se prepararon 10 imágenes. El manifiesto y el
estado quedan en `swebench/data/`.

Además, el binario del contenedor (`swebench/bin/lai`) se recompiló dos veces
con los arreglos de esta tanda; sin eso el benchmark habría medido el código
viejo (es el error que invalidó el piloto del 26).

### 6.4 El informe separa las dos tasas

`report.md` calculaba «Tasa de resolución» solo sobre instancias puntuadas, lo
que oculta los parches vacíos. Ahora muestra **ambas**: sobre puntuadas y de
extremo a extremo (resueltas / intentadas).

### 6.5 Pista de tests en la tarea

El enunciado que recibe el agente incluye ahora los nombres de los tests que
deben pasar (`FAIL_TO_PASS`, máx. 8) y cómo ejecutarlos. **No revela el
parche**: son los tests del issue, los mismos que ejecutaría una persona. Sin
esa pista el modelo leía archivos y nunca comprobaba nada.

## 7. Resultado del benchmark (medido)

Ver la tabla completa en `BENCHMARKS.md` (§ «Corrida real 2026-11-27»).

| Métrica | Valor |
|---|---|
| Instancias | 10 (una por repo de 5 repos + 3 astropy + 2 pytest) |
| Parches producidos | 8 / 10 |
| Parches vacíos | 2 / 10 |
| Resueltas por el evaluador oficial | **2** (`pytest-5809`, `matplotlib-13989`) |
| Errores de infraestructura | 0 |
| Tasa de extremo a extremo | 2/10 = 20 % |
| Tasa sobre puntuadas | 2/8 = 25 % |
| Informe | `swebench/data/evaluation/qwen3-8b-final/report.md` |

## 8. Verificación

| Comprobación | Resultado |
|---|---|
| `cargo fmt --check` | ✅ |
| `cargo clippy --all-targets` | ✅ sin avisos |
| `cargo test` | ✅ **151 tests** (109 lib + 42 integración) |
| `pytest tests swebench` | ✅ 242 passed, 8 skipped |
| `ruff check .` | ✅ All checks passed |
| `cargo build --release` | ✅ |
| llama.cpp | ✅ healthy, tool calling nativo, 7.4/8 GB VRAM |
| SWE-bench 10 instancias | ✅ 0 errores de infraestructura, 2 resueltas |

## 9. Lo que NO se ha probado (Regla 6)

- **Muestra pequeña.** 10 instancias de 500. El 20 % no es extrapolable; el
  intervalo de confianza con n=10 es enorme.
- **Un solo modelo y una sola configuración** (qwen3-8b Q4_K_M, ctx 18432).
  No se comparó con el perfil `bash` ni con otras temperaturas en condiciones
  controladas; hay una corrida suelta a 0.7 con 1/5 y otra a 0.2 con 2/10, pero
  **no son comparables** (instancias y código distintos).
- **`lai tui`** no se probó interactivamente: sin TTY falla con un error de
  sistema poco amable, que no se corrigió.
  **Actualización 2026-11-28:** corregido y verificado con PTY; ver
  `DOCS/CAMBIOS_2026-11-28.md` §2.
- **El gateway (8081)** sigue sin arrancar; `lai` habla directo con llama.cpp.
- **Windows** no se probó.
