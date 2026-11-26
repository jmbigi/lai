# SWE-bench Verified — harness incremental

Infraestructura para ejecutar el benchmark oficial **por etapas**, en local y con
modelos locales. No se persigue una cifra rápida: se prepara el terreno para
medir de verdad, cuando haya tiempo de cómputo.

> **Estado actual: harness funcional de punta a punta, validado con 1 instancia.
> Todavía NO hay resultados representativos del benchmark.** Ver *Pendientes*.

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
├── evaluate.py                  # 4. (PENDIENTE) tests FAIL_TO_PASS / PASS_TO_PASS
├── bin/                         # binario compilado (ignorado por git)
└── data/                        # instancias, estado, logs, predicciones (ignorado)
```

### Etapa 1 — Descargar

```bash
python3 swebench/fetch_dataset.py --limit 10
python3 swebench/fetch_dataset.py --repos django sympy   # filtrar por repo
```

Guarda `problem_statement`, `patch`, `test_patch`, `FAIL_TO_PASS`,
`PASS_TO_PASS` y los commits base. **500 instancias disponibles.**

### Etapa 2 — Preparar entornos

```bash
python3 swebench/prepare.py --limit 2       # descarga 2 imágenes nuevas
python3 swebench/prepare.py --check-only    # solo informa el estado
```

Comprueba espacio libre antes de cada descarga y guarda el estado por instancia
(`ready` / `pending` / `pulling` / `error`).

### Etapa 2b — Binario compatible

```bash
./swebench/build_lai_for_swebench.sh
```

**Problema resuelto:** el binario del host exige glibc 2.39; las imágenes de
SWE-bench son Ubuntu 22.04 (glibc 2.35) y fallaban con
`GLIBC_2.39 not found`. El script compila lai dentro de `ubuntu:22.04` y deja el
binario en `swebench/bin/lai`.

### Etapa 3 — Ejecutar el agente

```bash
python3 swebench/run_eval.py --limit 5 --model qwen3-coder-opt:latest
python3 swebench/run_eval.py --instance astropy__astropy-12907 --max-iterations 8
```

Arranca la imagen de la instancia, ejecuta `lai auto` dentro de `/testbed` con
el enunciado del issue, y captura `git diff` como predicción en formato oficial
(`instance_id`, `model_name_or_path`, `model_patch`).

### Etapa 4 — Evaluar (PENDIENTE)

Falta implementar `evaluate.py`: aplicar `test_patch`, correr los tests de
`FAIL_TO_PASS` y `PASS_TO_PASS` dentro del contenedor y calcular
*resolved* / *unresolved*.

---

## Validación realizada (1 instancia)

`astropy__astropy-12907` con `qwen3:8b`:

| Etapa | Resultado |
|-------|-----------|
| Descarga dataset | ✓ 500 instancias |
| Imagen Docker | ✓ 2.89 GB, repo en commit base verificado |
| Binario en contenedor | ✓ `lai 0.1.0` ejecuta |
| Ejecución del agente | ✓ parche producido (615 s, 8 iteraciones) |

**Ese parche no está evaluado todavía** — no se sabe si resuelve el issue.

---

## Bugs reales encontrados al montar el harness

| Bug | Arreglo |
|-----|---------|
| `GLIBC_2.39 not found` en el contenedor | Compilar en `ubuntu:22.04` (`build_lai_for_swebench.sh`) |
| `openssl-sys` rompía el build en contenedor limpio | `reqwest` con `default-features = false` (solo rustls) |
| `cp -r` arrastraba `target/` del host (binario con glibc nueva) | `rm -rf /build/target` + `CARGO_TARGET_DIR` |
| Marcadores `<<<...>>>` rompían bash | Marcadores alfanuméricos |
| `read_file` de archivos grandes desbordaba el contexto y el modelo olvidaba la tarea | `truncate_tool_output` (6 000 chars, cabeza+cola, 2 tests) |
| El *nudge* no bastaba: el modelo respondía "no hace falta tool" | Reinyectar la tarea completa en el nudge |
| El agente analizaba en prosa en vez de editar | `build_task()`: encuadre de ingeniería + "no modifiques tests" |

---

## Pendientes (en orden)

1. **`evaluate.py`** — aplicar `test_patch`, ejecutar `FAIL_TO_PASS` y
   `PASS_TO_PASS`, calcular *resolved*. Es la pieza que falta para tener cifras.
2. **Corrida piloto de 10–20 instancias** con `qwen3-coder-opt` para estimar
   tasa de resolución y coste temporal real.
3. **Contexto del modelo**: con 4 096 tokens el agente pierde el hilo. Probar
   `OLLAMA_CONTEXT_LENGTH=16384` de forma sistemática y documentar el efecto.
4. **Estrategia de lectura**: para repos grandes conviene `grep` + lectura por
   rangos en vez de `read_file` completo.
5. **Publicar la tabla** de resultados en `BENCHMARKS.md`, con la misma
   honestidad que el resto: muestra pequeña, hardware limitado, sin comparación
   directa con cifras frontier.

---

## Regla 0

- La descarga del dataset es **preparación puntual**, no parte del camino de
  ejecución de lai.
- La inferencia es 100 % local (`ollama` en el host, alcanzado por
  `--network host`).
- No hay claves API, cuentas ni servicios externos en el bucle de evaluación.
- Los artefactos (`bin/`, `data/`) están en `.gitignore`: no se versionan
  binarios ni datos de máquina.
