# SWE-bench Verified — harness incremental

Infraestructura para ejecutar el benchmark oficial **por etapas**, en local y con
modelos locales. No se persigue una cifra rápida: se prepara el terreno para
medir de verdad, cuando haya tiempo de cómputo.

> **Estado actual: harness completo de punta a punta (etapas 1–4), validado
> offline. Todavía NO hay resultados representativos del benchmark.**
> La corrida piloto está pendiente; ver *Pendientes*.

---

## Por qué por etapas

| Restricción real | Consecuencia en el diseño |
|------------------|---------------------------|
| 500 instancias × imagen Docker (~1–3 GB) | Descarga y preparación separadas, reanudables |
| 7.7 GB VRAM, modelos 30B con offload a CPU | Ejecución instancia a instancia, con timeout |
| Cada instancia tarda minutos | Se puede parar y relanzar sin perder trabajo |
| Riesgo de medir mal | Evaluación separada de la ejecución |

Cada etapa escribe su estado en disco y **retoma donde se quedó**.

---

## Pipeline

```
swebench/
├── fetch_dataset.py             # 1. descargar instancias → data/instances.jsonl
├── prepare.py                   # 2. docker pull imágenes → data/state.json
├── build_lai_for_swebench.sh    # 2b. binario compatible (glibc 2.35)
├── run_eval.py                  # 3. agente en contenedor → data/predictions/*.jsonl
├── evaluate.py                  # 4. tests oficiales → data/evaluation/<run>/report.json
├── summarize.py                 # 5. tabla markdown para BENCHMARKS.md (agente + evaluación)
├── run_pilot.sh                 # piloto de 5 instancias (una por parser oficial)
├── make_oracle_predictions.py   # utilidad: predicciones con el parche oficial (validar el evaluador)
├── docker_env.py                # diagnóstico de Docker compartido (versión, pistas)
├── test_evaluate.py             # tests offline: parsers y graduación
├── test_prepare.py              # tests offline: estados de imagen y escritura atómica
├── test_run_eval.py             # tests offline: clasificación y preflight
├── test_harness_offline.py      # tests offline: dataset simulado de punta a punta
├── test_docker_env.py           # tests offline: diagnóstico de Docker
├── test_summarize.py            # tests offline: cruce de metadatos y evaluación
├── bin/                         # binario compilado (ignorado por git)
└── data/                        # instancias, estado, logs, predicciones (ignorado)
```

## Antes de empezar: requisitos de Docker

`docker_env.py` comprueba el motor y, si falla, dice **por qué** en vez de dejar
que las etapas se cuelguen. Vale la pena leer esto antes de culpar al harness:

| Síntoma | Causa | Solución |
|---------|-------|----------|
| El CLI tarda 30–60 s en cada llamada y acaba en timeout | Docker Desktop antiguo frente a WSL reciente (caso real: 4.22 con WSL 3.0.5) | `winget upgrade --id Docker.DockerDesktop --include-unknown` |
| `docker info` no responde pero el pipe existe | El motor no terminó de arrancar | Esperar, o `wsl --shutdown` y reabrir Docker Desktop |
| Docker Desktop no arranca: `invalid character '\ufeff'` | `settings.json` guardado con BOM por PowerShell | Reescibir el JSON sin BOM (`encoding="utf-8"`, no `utf-8-sig`) |
| `127.0.0.1:11434` no responde desde el contenedor | El loopback del contenedor no es el del host | Usar `http://host.docker.internal:11434/v1` |

El harness traduce el loopback automáticamente para el agente, pero el
*preflight* te avisa igualmente, porque es un error de configuración habitual.

### Etapa 1 — Descargar

```bash
python3 swebench/fetch_dataset.py                    # 500 instancias
python3 swebench/fetch_dataset.py --limit 10
python3 swebench/fetch_dataset.py --repos django sympy   # filtrar por repo
python3 swebench/fetch_dataset.py --via api          # sin instalar 'datasets'
```

Usa el dataset oficial **`SWE-bench/SWE-bench_Verified`** (no
`princeton-nlp/...`). Guarda, por instancia: `problem_statement`, `patch`,
`test_patch`, `FAIL_TO_PASS`, `PASS_TO_PASS`, los commits base **y los campos
que necesita la evaluación**: `image`, `eval_script`, `log_parser`, `eval_type`.

La descarga tiene dos vías y **ninguna requiere instalar nada**:

| `--via` | Qué usa | Cuándo |
|---------|---------|--------|
| `auto` (defecto) | `datasets` si está instalado, si no la API | uso normal |
| `api` | sólo stdlib (`urllib`), pagina de 100 en 100 | entorno limpio |
| `datasets` | `pip install datasets` | si ya lo tienes |

> Si tienes datos de una descarga antigua, `evaluate.py` no puede usarlos.
> Vuelve a descargar con `--force`.

### Etapa 1b — Validar el evaluador sin agente

Antes de gastar horas de CPU, comprueba que `evaluate.py` sabe graduar:

```bash
python3 swebench/make_oracle_predictions.py --limit 2
python3 swebench/evaluate.py --predictions swebench/data/predictions/oracle.jsonl
```

Genera predicciones con el **parche oficial** del dataset. Si el evaluador es
correcto, todas salen `resolved`. Si sale `resolved: 0`, el fallo está en
`evaluate.py` y no en el modelo: eso ahorra una corrida completa.

**Ejecutado el 2026-11-26 con 5 instancias, una por parser oficial: 5/5
resueltas, 0 errores de infraestructura.** Detalle de F2P/P2P por instancia en
`DOCS/CAMBIOS_2026-11-26_DOCKER.md`. Ojo: eso valida el evaluador, no al agente;
el parche lo escribió una persona.

### Etapa 2 — Preparar entornos

```bash
python3 swebench/prepare.py --limit 2       # descarga 2 imágenes nuevas
python3 swebench/prepare.py --check-only    # solo informa el estado
```

Usa la imagen declarada en el dataset. Comprueba espacio libre antes de cada
descarga (`--min-free-gb`, 20 por defecto) y guarda el estado por instancia
(`ready` / `pending` / `pulling` / `error`). Los timeouts se registran como
`error` en vez de dejar la instancia colgada.

### Etapa 2b — Binario compatible

```bash
./swebench/build_lai_for_swebench.sh
PLATFORM=linux/amd64 ./swebench/build_lai_for_swebench.sh   # forzar plataforma
```

**Problema resuelto:** el binario del host exige glibc 2.39; las imágenes de
SWE-bench son Ubuntu 22.04 (glibc 2.35) y fallaban con
`GLIBC_2.39 not found`. El script compila lai dentro de `ubuntu:22.04` con
`--locked` y deja el binario en `swebench/bin/lai`. Verificado: el binario
resultante imprime `lai 0.1.0` dentro de la imagen de django y alcanza
`host.docker.internal:11434`.

> **Recompila siempre después de tocar `lai_rust`.** El harness no puede saber si
> `swebench/bin/lai` está desactualizado. Un binario viejo falla de formas que
> parecen errores del modelo (por ejemplo, rechazando el endpoint del contenedor
> por la guarda de la Regla 4).
>
> Los artefactos de compilación se guardan en el volumen de caché
> (`~/.cache/lai-swebench-cargo/target`): la primera compilación tarda ~25 min y
> las siguientes reaprovechan las dependencias.

### Etapa 3 — Ejecutar el agente

```bash
./swebench/run_pilot.sh                                     # piloto de 5 instancias
MODEL=qwen2.5-coder:3b ./swebench/run_pilot.sh              # otro modelo
python3 swebench/run_eval.py --limit 5 --model qwen3-coder-opt:latest
python3 swebench/run_eval.py --instance astropy__astropy-12907 --max-iterations 8
```

`run_pilot.sh` ejecuta las 5 instancias del piloto (una por cada parser oficial)
con `--keep-going`, de modo que un fallo de infraestructura no detiene el resto.

**Reejecutar una instancia.** `run_eval.py` salta las instancias que ya tienen
predicción escrita para ese modelo, y muestra «No hay instancias pendientes para
este modelo». Para repetir una, usa `--force`:

```bash
python3 swebench/run_eval.py --instance django__django-12209 --force
```

Los fallos de infraestructura **no** cuentan como predicción: se reintentan sin
necesidad de `--force`.

Arranca la imagen de la instancia, ejecuta `lai auto` dentro de `/testbed` con
el enunciado del issue y captura `git diff` como predicción en formato oficial
(`instance_id`, `model_name_or_path`, `model_patch`).

Antes de ejecutar, hace *preflight* solo de las instancias pendientes: Docker,
imagen, modelo presente en Ollama y longitud de contexto. Si el contexto es
menor de 8 192 tokens, avisa y recomienda `OLLAMA_CONTEXT_LENGTH=16384`.

> **Medido el 2026-11-26 (Regla 11).** En un i7-10510U con 15.8 GB de RAM y
> `qwen2.5-coder:3b`, subir o bajar el contexto entre 16384 y 32768 **no cambió
> el throughput**: 7.11 tok/s con 32768 frente a 7.07 tok/s con 16384 (4
> muestras cada uno, misma prompt). El aviso de contexto sigue teniendo sentido
> para tareas de agente con repos grandes, pero no esperes que
> `OLLAMA_CONTEXT_LENGTH=16384` acelere nada. Detalle y comando reproducible en
> `BENCHMARKS.md` y `scripts/measure_context_throughput.sh`.

Si `lai` termina con error, **el diff se conserva igualmente**; solo se
descartan los fallos de infraestructura (imagen ausente, CLI no ejecutable,
glibc incompatible, timeout), que no se escriben como predicciones.

### Etapa 4 — Evaluar

```bash
python3 swebench/evaluate.py --limit 5
python3 swebench/evaluate.py --instance astropy__astropy-12907 --dry-run
python3 swebench/evaluate.py --predictions swebench/data/predictions/qwen3-coder-opt.jsonl
```

Reproduce la evaluación oficial:

1. Aplica el parche del modelo en `/testbed` con `git apply`.
2. Ejecuta el **`eval_script` oficial de la instancia** (el que viene en el
   dataset): activa el entorno, reinstala el paquete, resetea los archivos de
   test al commit base, aplica `test_patch`, corre los tests y resetea de nuevo.
3. Parsea la salida con el `log_parser` oficial correspondiente al repo.
4. Calcula `FAIL_TO_PASS` / `PASS_TO_PASS` y *resolved* con la semántica oficial
   (`PASSED`/`XFAIL` cuentan como *pass*; `PASS_TO_PASS` admite `SKIPPED`).

Escribe `data/evaluation/<run_id>/report.json`, `report.md` y logs por instancia.
Los fallos de infraestructura se separan y **no se puntúan como parche fallido**.

Prueba offline de los parsers y la graduación, sin Docker:

```bash
python3 -m unittest swebench/test_evaluate.py -v
```

### Etapa 5 — Resumir para `BENCHMARKS.md`

```bash
python3 swebench/summarize.py --model qwen2.5-coder:7b
python3 swebench/summarize.py --model qwen2.5-coder:7b --md    # solo la tabla
```

Cruza los metadatos del agente (`data/predictions/<modelo>.meta.json`) con el
informe de evaluación (`data/evaluation/<run_id>/report.json`) y emite una tabla
markdown lista para pegar. Lo que no está medido aparece como `—`; si falta la
evaluación lo dice en vez de inventar un cero. Registra el **contexto real** que
Ollama tenía cargado (`/api/ps` después de la corrida), no un valor supuesto.

---

## Pruebas offline del harness (sin Docker)

Todo lo que no depende de un *daemon* se prueba en cualquier equipo:

```bash
python3 -m unittest swebench.test_prepare swebench.test_run_eval \
    swebench.test_harness_offline swebench.test_evaluate \
    swebench.test_docker_env swebench.test_summarize
```

| Suite | Qué cubre |
|-------|-----------|
| `test_prepare.py` | Nombre de imagen (dataset y convención), `image_state` (presente/ausente/desconocido/timeout), estado atómico, JSON corrupto, `free_gb` |
| `test_run_eval.py` | `slug` sin traversal, `build_task`, `ollama_root`, `read_context` (nunca inventa contexto), traducción del base-url al contenedor, extracción del diff (primer bloque), clasificación `timeout`/`infra_error`/`no_patch`/`patched` (glibc, librerías, `command not found`, rechazo de la Regla 4, marcadores ausentes), preflight de Ollama e imágenes |
| `test_harness_offline.py` | Normalización de instancias (listas JSON), escritura atómica, carga en `evaluate.py`, `choose_predictions`, wrapper del `eval_script`, graduación con dataset simulado |
| `test_evaluate.py` | Parsers oficiales, códigos de salida, inyección del `exit code`, graduación oficial, `FAIL_TO_PASS`/`PASS_TO_PASS` vacíos válidos |
| `test_docker_env.py` | Diagnóstico de Docker: CLI ausente, motor colgado, versión vacía, antigüedad del servidor |
| `test_summarize.py` | Cruce de metadatos con el informe, modo solo-informe, evaluación ausente, `--md` |

Ninguna de estas suites usa Docker, HuggingFace ni red.

---

## Validación realizada (1 instancia)

`astropy__astropy-12907` con `qwen3:8b`:

| Etapa | Resultado |
|-------|-----------|
| Descarga dataset | ✓ 500 instancias |
| Imagen Docker | ✓ 2.89 GB, repo en commit base verificado |
| Binario en contenedor | ✓ `lai 0.1.0` ejecuta |
| Ejecución del agente | ✓ parche producido (615 s, 8 iteraciones) |
| Evaluación | ⛔ pendiente: sin Docker en el equipo de trabajo |

**Ese parche no está evaluado todavía** — no se sabe si resuelve el issue.

---

## Bugs reales encontrados al montar el harness

| Bug | Arreglo |
|-----|---------|
| `GLIBC_2.39 not found` en el contenedor | Compilar en `ubuntu:22.04` (`build_lai_for_swebench.sh`) |
| `openssl-sys` rompía el build en contenedor limpio | `reqwest` con `default-features = false` (solo rustls) |
| `cp -r` arrastraba `target/` del host (binario con glibc nueva) | `rm -rf /build/target` + `CARGO_TARGET_DIR` |
| Marcadores `<<<...>>>` rompían bash | Marcadores alfanuméricos |
| `read_file` de archivos grandes desbordaba el contexto y el modelo olvidaba la tarea | `truncate_tool_output` (6 000 chars, cabeza+cola) + `read_file` por rangos |
| El *nudge* no bastaba: el modelo respondía "no hace falta tool" | Reinyectar la tarea completa en el nudge |
| El agente analizaba en prosa en vez de editar | `build_task()`: encuadre de ingeniería + "no modifiques tests" |
| `set -e` alrededor de `lai` perdía el diff si el agente fallaba | Captura explícita de `LAI_RC`; el diff se emite siempre |
| Fallos de infraestructura se guardaban como predicciones vacías | Clasificación `infra_error` y no se escriben |
| El nombre de imagen se adivinaba por convención | Usar `image` del dataset |

---

## Estrategia de lectura para repos grandes

Un repo de SWE-bench no cabe en el contexto. El agente debe:

1. `grep` con `regex`, `glob` y `case_insensitive` para localizar símbolos.
2. `read_file` con `start_line`/`end_line` (máximo 400 líneas por defecto,
   2 000 configurable) alrededor de los aciertos.
3. `find_files` con globs (`src/**/*.rs`) en lugar de listar todo el árbol.

El contexto del proyecto inyectado en `auto` está acotado a 8 000 caracteres
para que no desplace la tarea.

---

## Pendientes (en orden)

1. **Corrida piloto de 5 instancias** (django, sphinx, sympy, scikit-learn,
   pytest: uno por cada parser oficial) con `qwen2.5-coder:3b` o el modelo que
   elijas. Docker ya funciona en este equipo.
2. **Ampliar a 10–20 instancias** si el piloto termina sin fallos de
   infraestructura, para estimar tasa de resolución con algo más de muestra.
3. **Contexto del modelo**: medido ya el efecto en throughput (ver arriba): no
   hay ganancia entre 16384 y 32768. Falta medirlo en una corrida de agente real
   con repos grandes, que es donde el contexto se agota de verdad.
4. **Publicar la tabla** de resultados en `BENCHMARKS.md`, con la misma
   honestidad que el resto: muestra pequeña, hardware limitado, sin comparación
   directa con cifras frontier.

> El punto 3 está parcialmente cerrado (medición de throughput hecha). Los
> puntos 1, 2 y 4 dependen de terminar el piloto. **Todavía no hay cifras de
> SWE-bench que publicar.**

---

## Regla 0

- La descarga del dataset es **preparación puntual**, no parte del camino de
  ejecución de lai.
- La inferencia es 100 % local (`ollama` en el host, alcanzado por
  `--network host`).
- No hay claves API, cuentas ni servicios externos en el bucle de evaluación.
- Los artefactos (`bin/`, `data/`) están en `.gitignore`: no se versionan
  binarios ni datos de máquina.
