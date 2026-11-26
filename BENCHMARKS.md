# lai Benchmarks — resultados honestos

Este documento registra **cómo se mide lai y qué resultados reales obtiene**.
La regla del proyecto es simple: *si una tarea no produce código que compile y
pase tests, no cuenta como éxito*. Nada de "PASS" por compilar una plantilla.

---

## Metodología

### Verificación en tres niveles

Cada tarea se ejecuta en un directorio temporal con un crate Rust mínimo
(`src/lib.rs` con un placeholder y un test trivial). Se toma un hash de
`src/lib.rs` antes de invocar al agente y otro después.

| Resultado | Significado |
|-----------|-------------|
| `PASS`    | El agente modificó archivos **y** al menos 1 test pasa |
| `PARTIAL` | El agente modificó archivos y compila, pero no hay tests |
| `FAIL`    | Sin cambios en archivos, o no compila, o los tests fallan |
| `TIMEOUT` | Superó el límite por tarea |

Esto corrige un fallo de medición anterior: `cargo check` sobre la plantilla
intacta daba falsos positivos.

### Entorno de referencia

| Componente | Valor |
|------------|-------|
| GPU | NVIDIA RTX 3070 Laptop, 7.7 GB VRAM (driver 13.0) |
| Runtime | Ollama 0.32.14, contexto 4096 |
| Backend | `lai auto` → tool calling nativo ([OI] `/v1/chat/completions`) con fallback a bloques JSON |
| Fecha | 2026-11-25 |

> Con 7.7 GB de VRAM, los modelos de 30B se ejecutan con offload parcial a CPU,
> por eso la latencia es alta. En hardware mayor los tiempos bajan.

---

## Scripts

```bash
# Benchmark rápido (3 tareas) o completo (9 tareas)
./run_benchmark.sh qwen3-coder-opt:latest http://127.0.0.1:11434/v1 true

# Competencia entre modelos (mismas 3 tareas)
MODELS_OVERRIDE="qwen3:8b qwen3-coder-opt:latest" TASK_TIMEOUT=600 ./run_competition.sh

# Estrés: 20 tareas, en paralelo
TASKS_LIMIT=20 MAX_ITER=10 ./run_stress.sh qwen3-coder-opt:latest http://127.0.0.1:11434/v1 2
```

Variables de entorno soportadas:

| Variable | Scripts | Uso |
|----------|---------|-----|
| `TASKS_LIMIT` | stress | Ejecutar solo las primeras N tareas |
| `MAX_ITER` | stress | Iteraciones máximas del agente por tarea |
| `TASK_TIMEOUT` | competition | Segundos por tarea antes de `TIMEOUT` |
| `MODELS_OVERRIDE` | competition | Lista de modelos separada por espacios |

---

## Resultados medidos (2026-11-25)

### Competencia: 3 tareas × 2 modelos, `TASK_TIMEOUT=600`

| Modelo | Tamaño | Tool calling | Resultado |
|--------|--------|--------------|-----------|
| `qwen3:8b` | 8.2B denso, Q4_K_M | Sí (nativo) | **2/3** (67%) |
| `qwen3-coder-opt:latest` | 30.5B MoE (3.3B activos), Q4_K_M | Sí (nativo) | **3/3** (100%) |

Tareas: `add_two_numbers`, `factorial`, `fix_off_by_one`.

Detalle del fallo de `qwen3:8b`: en `fix_off_by_one` reescribió el archivo
eliminando el bug, pero también eliminó los tests existentes, por lo que la
verificación no encontró evidencia de corrección (`FAIL`). Es un fallo real de
comportamiento, no de medición.

### Benchmark rápido: 3 tareas × `qwen3:8b`

Tras aplicar el arreglo de tool calling + nudge: **3/3 PASS** con tests
ejecutados. Antes del arreglo el mismo modelo daba **0/3** (respondía en prosa
sin escribir archivos).

### Estrés: 4 tareas × `qwen3:8b`, `MAX_ITER=6`, 2 jobs en paralelo

**1/4 PASS (25%)**. Las tareas de estrés son deliberadamente más amplias
(algoritmos, concurrencia, serialización) y con menos iteraciones.

### `deepseek-r1:8b`

No declara capacidad `tools` en Ollama y depende del protocolo JSON simulado.
Medido: >20 min por tarea, 1 PASS antes de agotar el tiempo. **No es práctico**
para ciclos de agente en este hardware. Queda excluido de la comparativa.

### CPU sin GPU dedicada: 3 tareas × `qwen2.5-coder:3b` (2026-11-26)

Primera medición propia en un equipo **sin GPU dedicada**, hecha con la versión
corregida de `run_benchmark.sh` (`LAI_BIN` en lugar de rutas absolutas).

| Dato | Valor |
|------|-------|
| Hardware | Intel Core i7-10510U (4c/8t, 1.8 GHz), 15.8 GB RAM, Intel UHD Graphics (sin GPU dedicada) |
| Modelo | `qwen2.5-coder:3b` (Q4, ~1.9 GB) |
| Runtime | Ollama en `127.0.0.1:11434`. Contexto cargado **32768** (verificado con `/api/ps`; la etiqueta anterior «4096» era una suposición y estaba mal) |
| Binario | `lai 0.1.0` (Rust, `--release`) |
| Tareas | 3 (modo `quick`: `add_two_numbers`, `factorial`, `fix_off_by_one`), `--max-iterations 10` |

| Tarea | Resultado | Tiempo | Motivo |
|-------|-----------|--------|--------|
| `add_two_numbers` | FAIL | 255.6 s | sin cambios en archivos (`rc=0`) |
| `factorial` | FAIL | 391.6 s | no compila o los tests fallan |
| `fix_off_by_one` | PARTIAL | 223.3 s | compila, sin tests ejecutados |
| **Total** | **0 PASS / 1 PARTIAL / 2 FAIL** | **870.5 s (14.5 min)** | — |

> **Corrección posterior (Regla 6/7).** Esta corrida se hizo con un bug activo en
> el parser de tool calls: `qwen2.5-coder:3b` emitía el bloque ```` ```json ````
> correctamente pero lai respondía `No tool call emitted` porque el `content`
> traía saltos de línea literales y sólo se aceptaba la clave `"tool"`. Es decir,
> el `FAIL` de `add_two_numbers` **no era falta de capacidad del modelo sino un
> bug del agente**. Los números se conservan como registro histórico de la
> medición; la comparación válida es la de la subsección siguiente.

### CPU sin GPU dedicada, con el parser corregido (2026-11-26, repetición)

Misma máquina, mismo modelo, mismas 3 tareas, mismo binario `--release`, pero
con el parser de tool calls arreglado. El agente **sí escribe archivos** ahora.

| Dato | Valor |
|------|-------|
| Hardware | Intel Core i7-10510U (4c/8t, 1.8 GHz), 15.8 GB RAM, Intel UHD Graphics |
| Modelo | `qwen2.5-coder:3b` (Q4_K_M, 3.1B, ~1.9 GB de pesos) |
| Runtime | Ollama `127.0.0.1:11434`, `context_length` cargado **32768** (leído de `/api/ps`) |
| Binario | `lai 0.1.0` (Rust, `--release`, con el parser corregido) |
| Tareas | 3 (`quick`), `--max-iterations 10` |

| Tarea | Resultado | Tiempo | Motivo real (revisado a mano) |
|-------|-----------|--------|-------------------------------|
| `add_two_numbers` | FAIL | 1312.1 s | reescribió `add(i32, i32)` como `add(&str, &str)` y rompió la compilación y el test existente |
| `factorial` | FAIL | 345.5 s | **sobrescribió `Cargo.toml`** con un `...` literal → TOML inválido |
| `fix_off_by_one` | PARTIAL | 198.0 s | cambió `0..=arr.len()` por `0..arr.len() - 1` (sigue mal: underflow con slice vacío); compila, no hay tests |
| **Total** | **0 PASS / 1 PARTIAL / 2 FAIL** | **1855.5 s (30.9 min)** | — |

**Lectura honesta.** El arreglo del parser funcionó: los archivos cambian y el
`FAIL` ya no es «no file changes». Pero el resultado sigue siendo 0 PASS, y la
causa es distinta y más informativa:

1. El modelo **reemplaza código que ya funcionaba** en lugar de extenderlo:
   `add(i32, i32)` pasó a `add(&str, &str)`, lo que rompe el test que existía.
2. El modelo **toca archivos que no debería**: sobrescribió `Cargo.toml` con
   contenido inventado. No hay ninguna guarda en el agente que impida escribir
   manifiestos.
3. El «arreglo» del off-by-one es incorrecto pero compila, así que el
   verificador solo puede dar `PARTIAL`.

Esto es un problema de **diseño de la tarea y del prompt**, no solo del modelo:
los tres enunciados («escribe una función que sume…») son ambiguos cuando el
archivo ya contiene una función que lo hace, y nada le dice al agente que
conserve la API pública ni que no toque `Cargo.toml`. Comparar este 0 PASS con
cifras de modelos frontier no tendría sentido. Con 3 tareas tampoco se puede
afirmar nada sobre la calidad del modelo (Regla 11).

### Efecto de `OLLAMA_CONTEXT_LENGTH` en el throughput (2026-11-26)

Medición controlada, una sola variable (Regla 11): mismo modelo, mismo binario,
mismo prompt (43 tokens de prompt, 128 tokens generados, `temperature: 0`), dos
instancias de Ollama en el mismo equipo. Reproducible con
`scripts/measure_context_throughput.sh`. `tok/s` sale de `eval_count` /
`eval_duration` que devuelve `/api/generate`; el contexto cargado se lee de
`/api/ps`, no se asume.

| Instancia | `context_length` real | Muestras (tok/s) | Media |
|-----------|----------------------|------------------|-------|
| `:11434` (por defecto) | 32768 | 7.56, 7.19, 6.44, 7.26 | **7.11** |
| `:11435` (`OLLAMA_CONTEXT_LENGTH=16384`) | 16384 | 7.15, 7.28, 6.82, 7.04 | **7.07** |

**Conclusión medida:** la diferencia es del 0.6 %, indistinguible del ruido con
4 muestras. En este equipo, con prompts cortos, bajar el contexto de 32768 a
16384 **no acelera la decodificación**. La recomendación de
`OLLAMA_CONTEXT_LENGTH=16384` que figuraba en `swebench/README.md` no está
respaldada por esta medición: puede servir para ahorrar memoria (la caché KV de
32k ocupa más), pero no como mejora de velocidad. Además, `32768` no era una
suposición: es lo que el modelo trae por defecto, así que la etiqueta «contexto
4096» de la tabla anterior era falsa y se corrigió.

Lectura honesta: en CPU pura, 3 tareas consumen ~15 minutos y el modelo de 3B no
completa ninguna con evidencia de tests. Es coherente con lo descrito en
`DOCS/INFERENCIA_LOCAL_BAJA_GAMA.md` (el ancho de banda manda), pero **una sola
corrida de 3 tareas no demuestra nada sobre la calidad del modelo**: puede ser
varianza. Para afirmar algo haría falta repetir con controles intercalados
(Regla 11).

---

## Qué se arregló gracias a estas pruebas

| Bug encontrado | Arreglo |
|----------------|---------|
| Modelos sin capacidad `tools` no escribían archivos | Tool calling nativo + fallback JSON + *nudge* si responde en prosa |
| El modelo emitía ```` ```json ```` correcto pero lai decía «No tool call emitted» | Parser reparado: saltos de línea literales dentro de cadenas, alias `tool`/`name`, `arguments` como string |
| El agente sobrescribe `Cargo.toml` con contenido inventado | **Pendiente** (hallazgo 2026-11-26): no hay guarda sobre manifiestos |
| El agente reemplaza una API pública que ya funcionaba | **Pendiente**: los enunciados del benchmark no declaran la restricción |
| `\n` literales (JSON doble-escapado) → archivo en una línea | `unescape_literal_escapes` en `normalize_tool_arguments` |
| Ruta con `:` (`qwen3:8b/`) rompía `cargo test` | Sanear nombre de directorio en los scripts |
| "PASS" falso al no modificarse nada | Hash antes/después + tests obligatorios |
| Tests ausentes contaban como éxito | Nivel `PARTIAL` explícito |
| Modelos lentos bloqueaban la suite | `TASK_TIMEOUT` por tarea |
| Sin paralelismo en estrés | `xargs -P` con jobs configurables |

---

## Limitaciones declaradas

- **No es SWE-bench Verified.** Son tareas propias, reproducibles, no el
  benchmark académico de 500 issues. La comparación con cifras publicadas de
  modelos frontier **no es directa** y aquí no se hace.
- **No determinismo.** Los modelos locales varían entre ejecuciones; los
  números son una muestra, no una garantía.
- **Muestra pequeña.** 3–20 tareas por corrida. Sirven para detectar
  regresiones, no para ranking fino.
- **Dos entornos.** La comparativa de 2026-11-25 se midió con GPU (RTX 3070
  Laptop, 7.7 GB VRAM); la corrida de CPU de 2026-11-26, con una iGPU y 15.8 GB
  de RAM. **No son comparables entre sí**: son filas distintas de hardware.

---

## Próximos pasos de medición

1. Corrida completa del estrés (20 tareas) por modelo y publicar la tabla.
2. Comparar `lai auto` contra el mismo modelo con tool calling nativo puro
   (sin fallback) para aislar el efecto del *nudge*.
3. Añadir tareas multi-archivo (refactor con tests existentes) para medir
   comprensión de repositorio, no solo generación de un archivo.

---

## SWE-bench Verified

Hay un harness incremental en [`swebench/`](swebench/README.md) para ejecutar el
benchmark oficial por etapas (descargar → preparar → ejecutar → evaluar).

Estado: **harness completo (etapas 1–4) y validado de punta a punta**. La
evaluación (`swebench/evaluate.py`) aplica el parche del modelo, ejecuta el
`eval_script` oficial de cada instancia, parsea los logs con el parser oficial y
calcula `FAIL_TO_PASS`/`PASS_TO_PASS` y *resolved* con la semántica oficial.

**Validación del evaluador (2026-11-26):** se generaron predicciones con el
`patch` oficial del dataset (`swebench/make_oracle_predictions.py`) para cinco
instancias, una por cada parser oficial, y se evaluaron. Resultado: **5/5
resueltas, 0 errores de infraestructura**. Esto demuestra que el evaluador no
devuelve «no resuelto» por defecto; **no es un resultado del agente**, porque el
parche lo escribió una persona.

| Instancia | Parser | Tiempo de evaluación |
|-----------|--------|---------------------:|
| django__django-12209 | `parse_log_django` | 174.6 s |
| pytest-dev__pytest-5809 | `parse_log_pytest` | 49.6 s |
| scikit-learn__scikit-learn-14141 | `parse_log_pytest_v2` | 88.7 s |
| sphinx-doc__sphinx-8595 | `parse_log_pytest_v2` | 57.3 s |
| sympy__sympy-20916 | `parse_log_sympy` | 50.5 s |

Las corridas de agente anteriores (`astropy__astropy-12907` con `qwen3:8b`, 615 s)
se hicieron con un binario que no podía llegar a Ollama desde el contenedor, así
que **sus parches no están evaluados y no cuentan**.

### Primera corrida real del agente (2026-11-26)

| Dato | Valor |
|------|-------|
| Instancia | `django__django-12209` (`parse_log_django`, `pass_and_fail`) |
| Modelo | `qwen2.5-coder:7b` (Q4_K_M, 7.6B, 6.9 GB) |
| Runtime | Ollama `host.docker.internal:11434`, **100 % CPU** (`size_vram: 0`) |
| Contexto cargado | **32768** tokens (leído de `/api/ps`) |
| Hardware | i7-10510U (4c/8t), 15.8 GB RAM, sin GPU dedicada |
| Agente | 1048.7 s (17.5 min), 10 iteraciones, `rc=0` |
| Parche | 81 236 B, **1 archivo** |
| Evaluación oficial | **no resuelta** (6.5 s) |

**Qué pasó, sin adornos.** El agente no editó el código: reemplazó
`django/db/models/base.py` completo. El diff es `@@ -1,1911 +1,18 @@` — las 1911
líneas del core de Django sustituidas por las 18 líneas del ejemplo del issue
(`from uuid import uuid4`, `class Sample(models.Model)`). El ejemplo ilustraba el
bug; el modelo lo tomó como la solución.

Consecuencia: `AttributeError: module 'django.db.models' has no attribute
'Model'` y los tests fallan. El evaluador lo puntúa como **no resuelta**, que es
lo correcto: un parche que rompe la importación del paquete no arregla nada.

**Tres bugs del harness que esta corrida destapó y que ya están corregidos:**

1. El evaluador marcaba el caso como `no_test_results` (no puntuable) porque el
   parser no encontraba líneas de test. Descartar un fallo de importación
   **inflaría** la tasa de resolución. Ahora, si el wrapper capturó un código de
   salida de tests, la instancia se puntúa y los `FAIL_TO_PASS` fallan.
2. `summarize.py` y `run_eval.py` calculaban slugs distintos para el mismo
   modelo, así que el resumen no encontraba los metadatos.
3. `write_file` no tenía ninguna protección: aceptaba sustituir 1911 líneas por
   18. Ahora rechaza reducir un archivo de 40+ líneas a menos del 25 % salvo que
   se pase `allow_shrink: true`.

> **No hay resultados representativos del agente.** La corrida piloto cubre 5
> instancias, una por cada parser oficial. Sirve para validar el harness de punta
> a punta, no para juzgar al modelo.

| Instancia | Parser | Agente | Parche | Evaluación | Resultado |
|-----------|--------|-------:|-------:|------------|-----------|
| `django__django-12209` | `parse_log_django` | 1048.7 s | 81 236 B | 34.4 s | no resuelta (rompió el paquete) |
| `sphinx-doc__sphinx-8595` | `pytest_v2` | 1505.5 s | 0 B | 0 s | sin parche (timeout) |
| `sympy__sympy-20916` | `parse_log_sympy` | 1501.8 s | 0 B | 0 s | sin parche (timeout) |
| `scikit-learn__scikit-learn-14141` | `pytest_v2` | — | — | — | en curso |
| `pytest-dev__pytest-5809` | `parse_log_pytest` | — | — | — | en curso |

Contexto real de Ollama en todas las corridas: **32768** tokens (leído de
`/api/ps` después de cada corrida, no asumido). Hardware: i7-10510U, 15.8 GB,
sin GPU dedicada, `size_vram: 0` (100 % CPU).

**Lo que esta tabla sí demuestra:** el harness funciona de punta a punta con
cuatro parsers distintos y separa correctamente el fallo del modelo
(`django`, parche que rompe la importación) del fallo de tiempo (`sphinx`,
`sympy`, sin parche).

**Lo que no demuestra:** nada sobre la calidad de `qwen2.5-coder:7b`. Tres de
cinco instancias no llegaron a producir un parche evaluable, y el único parche
producido destruyó un archivo de 1911 líneas. Con esta muestra no se puede
estimar una tasa de resolución.

**Hallazgo operativo medido:** con 7B en CPU y contexto de 32768, 1500 s no
alcanzan para repos grandes. `django` hizo 10 iteraciones en 1048 s, pero
`sphinx` y `sympy` consumieron los 1500 s en 4 y 2 iteraciones respectivamente.
El coste por iteración depende de cuánto contexto se lee, no del número de
iteraciones.

Metodología prevista (Regla 11): una variable a la vez, `OLLAMA_CONTEXT_LENGTH`
fijado y documentado, lectura por `grep` + rangos, y separación explícita entre
fallos del modelo y fallos de infraestructura.
