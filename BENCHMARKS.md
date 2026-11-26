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

Estado: **harness completo (etapas 1–4)**. La evaluación ya está implementada
(`swebench/evaluate.py`): aplica el parche del modelo, ejecuta el `eval_script`
oficial de cada instancia, parsea los logs con el parser oficial y calcula
`FAIL_TO_PASS`/`PASS_TO_PASS` y *resolved* con la semántica oficial. Los parsers
y la graduación tienen tests offline (`swebench/test_evaluate.py`, 15 tests, sin
Docker).

La única corrida de agente realizada fue `astropy__astropy-12907` con
`qwen3:8b` (parche generado en 615 s). **Ese parche no está evaluado**, y no
hay corrida piloto de 10–20 instancias.

> **No hay todavía resultados representativos.** Cualquier cifra de esta sección
> sería prematura. La tabla siguiente se publicará solo cuando exista una
> corrida real, con hardware, fecha y contexto declarados.

| Modelo | Instancias | Resueltas | Tasa | Tiempo medio | Contexto |
|--------|-----------|-----------|------|--------------|----------|
| *(pendiente de corrida piloto)* | — | — | — | — | — |

Metodología prevista (Regla 11): una variable a la vez, `OLLAMA_CONTEXT_LENGTH`
fijado y documentado, lectura por `grep` + rangos, y separación explícita entre
fallos del modelo y fallos de infraestructura.
