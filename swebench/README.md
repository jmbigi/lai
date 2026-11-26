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
├── test_evaluate.py             # tests offline de parsers y graduación (sin Docker)
├── bin/                         # binario compilado (ignorado por git)
└── data/                        # instancias, estado, logs, predicciones (ignorado)
```

### Etapa 1 — Descargar

```bash
python3 swebench/fetch_dataset.py --limit 10
python3 swebench/fetch_dataset.py --repos django sympy   # filtrar por repo
```

Usa el dataset oficial **`SWE-bench/SWE-bench_Verified`** (no
`princeton-nlp/...`). Guarda, por instancia: `problem_statement`, `patch`,
`test_patch`, `FAIL_TO_PASS`, `PASS_TO_PASS`, los commits base **y los campos
que necesita la evaluación**: `image`, `eval_script`, `log_parser`, `eval_type`.

> Si tienes datos de una descarga antigua, `evaluate.py` no puede usarlos.
> Vuelve a descargar con `--force`.

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
`--locked` y deja el binario en `swebench/bin/lai`.

### Etapa 3 — Ejecutar el agente

```bash
python3 swebench/run_eval.py --limit 5 --model qwen3-coder-opt:latest
python3 swebench/run_eval.py --instance astropy__astropy-12907 --max-iterations 8
```

Arranca la imagen de la instancia, ejecuta `lai auto` dentro de `/testbed` con
el enunciado del issue y captura `git diff` como predicción en formato oficial
(`instance_id`, `model_name_or_path`, `model_patch`).

Antes de ejecutar, hace *preflight* solo de las instancias pendientes: Docker,
imagen, modelo presente en Ollama y longitud de contexto. Si el contexto es
menor de 8 192 tokens, avisa y recomienda `OLLAMA_CONTEXT_LENGTH=16384`.

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

1. **Corrida piloto de 10–20 instancias** con `qwen3-coder-opt` para estimar
   tasa de resolución y coste temporal real. Requiere un equipo con Docker.
2. **Contexto del modelo**: con 4 096 tokens el agente pierde el hilo. Probar
   `OLLAMA_CONTEXT_LENGTH=16384` de forma sistemática y documentar el efecto
   (una variable a la vez, Regla 11).
3. **Publicar la tabla** de resultados en `BENCHMARKS.md`, con la misma
   honestidad que el resto: muestra pequeña, hardware limitado, sin comparación
   directa con cifras frontier.

> Nada de esto se ha ejecutado todavía. **No hay cifras que publicar.**

---

## Regla 0

- La descarga del dataset es **preparación puntual**, no parte del camino de
  ejecución de lai.
- La inferencia es 100 % local (`ollama` en el host, alcanzado por
  `--network host`).
- No hay claves API, cuentas ni servicios externos en el bucle de evaluación.
- Los artefactos (`bin/`, `data/`) están en `.gitignore`: no se versionan
  binarios ni datos de máquina.
