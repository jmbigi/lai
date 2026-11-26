# Cambios propuestos — pendientes sin Docker (2026-11-26, tanda B)

> **Regla 10** (documentación obligatoria de cambios): este documento registra
> el estado **antes** y **después** de cada cambio de esta tanda.
> **Regla 6** (honestidad): ninguna cifra de benchmark se publica sin medición
> propia. Lo que no se pudo ejecutar se declara como no ejecutado.
> **Regla 11**: si se obtiene alguna medición real, se declara con su hardware,
> modelo y una sola variable; si no, no hay números.

## 0. Contexto y alcance

Tanda de trabajo posterior al commit `7b8985e` (*stage 4 evaluation + harden
harness*). El piloto SWE-bench sigue **bloqueado**: el *daemon* de Docker no está
activo en el equipo de trabajo, así que no se pueden preparar imágenes ni correr
instancias. Esta tanda hace todo lo verificable **sin Docker**:

| # | Trabajo | Bloqueado por Docker |
|---|---------|----------------------|
| 1 | Tests offline de `prepare.py` | No |
| 2 | Tests offline de `run_eval.py` (+ refactor mínimo para testear) | No |
| 3 | Test del harness con dataset simulado | No |
| 4 | `AGENTS.md`: arquitectura real del crate | No |
| 5 | `scripts/check_docs.py`: avisos de documentación | No |
| 6 | `ROADMAP.md` / `ESTADO_ACTUAL.md` desactualizados | No |
| 7 | `run_benchmark.sh`: rutas absolutas del autor + intento de medición real | No (la medición no necesita Docker) |
| 8 | Piloto SWE-bench 10–20 instancias y tabla en `BENCHMARKS.md` | **Sí** |

Entorno de verificación: Windows 10/11, Python 3.14.3, Rust/cargo 1.99.1
(`rustfmt` y `clippy` **no instalados**), Ollama activo con varios modelos
locales, Docker detenido.

---

## 1. Tests offline de `prepare.py`

**Archivo afectado:** `swebench/test_prepare.py` (nuevo).

**ANTES:** `prepare.py` no tenía ningún test. Sus funciones puras
(`image_name`, `normalise` implícito en `load_instances`, `save_state`,
`image_state`) solo se habían ejercitado a mano con Docker, así que un cambio
futuro podía romper el parseo de estados o el nombre de imagen sin que nada
fallara.

**DESPUÉS:** creado `swebench/test_prepare.py` con **13 tests** que mockean
`subprocess.run` (no necesitan Docker):

| Test | Cubre |
|---|---|
| `ImageNameTests` (3) | usa `image` del dataset; convención de reserva; minúsculas |
| `ImageStateTests` (5) | presente; ausente (`No such image`); error inesperado → `None`; Docker inexistente → `None`; timeout → `None` |
| `StatePersistenceTests` (2) | escritura atómica sin `.tmp` residual + `updated_at`; JSON corrupto tolerado |
| `LoadInstancesTests` (2) | ignora líneas vacías/corruptas; archivo ausente → `SystemExit(2)` |
| `FreeSpaceTests` (1) | `free_gb()` devuelve GB |

**Verificación:** `python -m unittest swebench.test_prepare` → 13/13 OK.

---

## 2. Tests offline de `run_eval.py` (+ refactor mínimo)

**Archivos afectados:** `swebench/run_eval.py`, `swebench/test_run_eval.py` (nuevo).

**ANTES:** la clasificación de resultados (`timeout`, `infra_error`,
`no_patch`, `patched`) y la extracción del diff estaban **inline** dentro de
`run_instance()`, que exige Docker. No se podían probar sin un *daemon*. Tampoco
había tests de `slug()`, `build_task()` ni `ollama_root()`.

**DESPUÉS:** se extrajeron dos funciones puras y se creó
`swebench/test_run_eval.py` con **28 tests**:

- `extract_patch(stdout)` — corta el **primer** par de marcadores (antes usaba
  `rsplit`, que arrastraba contenido si los marcadores se repetían).
- `classify_result(stdout, stderr, rc)` — devuelve `(estado, motivo)` sin tocar
  Docker. Orden de comprobación corregido: los errores de enlazado dinámico
  (`error while loading shared libraries`, `GLIBC_... not found`) se detectan
  **antes** del código de salida genérico 125/126/127, porque el *loader*
  también devuelve 127 y antes se reportaba el motivo equivocado.
- `slug()` ahora recorta también `.` iniciales (`../etc/passwd` → `etc_passwd`).

Cobertura: nombre de imagen (dataset y convención), `slug` sin traversal,
`build_task` (encuadre + prohibición de tocar tests), `ollama_root`,
extracción del diff, clasificación completa (timeout, rc 125, glibc,
librerías compartidas, `command not found`, marcadores ausentes, parche vacío,
parche con `agent_rc` distinto de cero), preflight de Ollama (modelo presente,
ausente, Ollama caído, contexto bajo solo avisa) y preflight de imágenes.

**Verificación:** `python -m unittest swebench.test_run_eval` → 28/28 OK.

---

## 3. Test del harness con dataset simulado

**Archivo afectado:** `swebench/test_harness_offline.py` (nuevo).

**ANTES:** las etapas 1–4 solo se habían probado contra el dataset real de
HuggingFace o contra Docker. No había ninguna prueba de que
`fetch_dataset.normalise()`, `existing_is_usable()` ni la escritura atómica
funcionaran con datos sintéticos, ni de que `evaluate.py` rechazara instancias
sin los campos de evaluación.

**DESPUÉS:** creado `swebench/test_harness_offline.py` con **19 tests** que
cargan `fetch_dataset.py` y `evaluate.py` por ruta y trabajan sobre un dataset
sintético en directorios temporales:

- `FetchNormaliseTests` (6): conserva los cinco campos obligatorios, parsea
  listas JSON, respeta listas ya parseadas, tolera JSON roto, rellena `None` y
  distingue datos utilizables de datos antiguos.
- `AtomicWriteTests` (2): `instances.jsonl` y `manifest.json` sin `.tmp`
  residual y con ida/vuelta exacta.
- `EvaluateLoadingTests` (5): carga de instancias (ignora filas sin id, parsea
  listas), predicciones sin `model_patch` → vacío, filas sin `instance_id`
  ignoradas, `choose_predictions` exige elección explícita si hay ambigüedad y
  autodetecta un único archivo.
- `EvalScriptWrapperTests` (2): el wrapper aplica el parche del modelo **antes**
  del `eval_script` oficial y `validate_instances()` detecta campos ausentes.
- `EndToEndOfflineTests` (4): graduación con dataset simulado (resuelto,
  parcial, F2P fallido, P2P regresado).

Se añadió `evaluate.validate_instances()` para poder comprobar los campos
obligatorios sin Docker (antes estaba inline en `main()`).

**Verificación:** `python -m unittest swebench.test_harness_offline` → 19/19 OK.

---

## 4. `AGENTS.md`: arquitectura real del crate

**Archivo afectado:** `AGENTS.md`.

**ANTES:** el árbol de `lai_rust/src/` listaba módulos que **no existen**
(`automode.rs`, `agents.rs`, `session.rs`, `thinking.rs`, `config.rs`) y omitía
los reales (`planner.rs`, `verification_engine.rs`, `repair_engine.rs`,
`diagnostic_engine.rs`, `change_engine.rs`, `context.rs`, `policy.rs`,
`quality.rs`, `repomap.rs`, `memory_store.rs`, `mcp_server.rs`, `tui.rs`,
`telemetry.rs`, `baseline.rs`, `init_wizard.rs` y los directorios
`benchmark/`, `orchestration/`, `verification/`). La tabla de migración
Python→Rust también apuntaba a módulos inexistentes. Un agente que siguiera ese
documento buscaría archivos que no están.

**DESPUÉS:** el árbol refleja `lai_rust/src/lib.rs` (26 módulos + 3
directorios), el CLI con **19 subcomandos** y el `ToolRegistry` con **13
herramientas**, con una descripción por módulo. La tabla de migración se
reetiqueta como "correspondencia histórica" y avisa de que no hubo port 1:1.
Se añade una nota explícita: el CLI Rust **no** tiene `serve`, `review`, `fix`,
`search` ni `project`; esas rutas siguen en el CLI Python legado.

**Verificación:** los nombres de módulo se contrastaron con
`Get-ChildItem lai_rust/src` y `lib.rs`; los subcomandos, con el `enum Commands`
de `main.rs` (19 variantes); las herramientas, con las llamadas
`register_tool(...)` de `tools.rs` (13).

---

## 5. `scripts/check_docs.py`: avisos de documentación

**Archivos afectados:** `scripts/check_docs.py` y los documentos que disparan
los avisos.

**ANTES:** el script reportaba avisos en documentos históricos:
`context_16384` (7 apariciones, incluida `MEJORAS_IMPLEMENTADAS.md` línea 291) y
`reasoning_format_none` (5 apariciones, p. ej. `MEJORAS_IMPLEMENTADAS.md`
líneas 20 y 34). Además, el script solo **informa**: no falla con código de
salida distinto de cero, por lo que `make check` no detecta la regresión.

**DESPUÉS:** el script ahora clasifica cada patrón con severidad:

| Severidad | Significado | Efecto |
|---|---|---|
| `ok` | comprobación correcta | oculta salvo `--show-ok` |
| `info` | revisar a mano (p. ej. `16384`, que es la recomendación de SWE-bench) | informa, no rompe |
| `warning` | fallo real (`--reasoning-format none`) | devuelve código 1 |

Los documentos históricos (`LECCIONES_APRENDIDAS.md`, `DOCUMENTACION_FINAL.md`,
`DOCUMENTACION_INTENTO.md`, `ANALISIS_EXHAUSTIVO.md`, `MEJORAS_IMPLEMENTADAS.md`,
`COMPARACION_2026.md`, `COMPARACION_KIMICODE.md`) **no se reescriben** (Regla 6):
sus coincidencias se degradan a `info` y se etiquetan `[histórico: se conserva]`.
Se añadió `DOCS/INFERENCIA_LOCAL_BAJA_GAMA.md` a la lista de ficheros revisados.

**Verificación:** `python scripts/check_docs.py` → `fallos (warning): 0`,
`avisos informativos: 13`, código de salida 0.

---

## 6. `ROADMAP.md` / `ESTADO_ACTUAL.md` desactualizados

**Archivos afectados:** `ROADMAP.md`, `ESTADO_ACTUAL.md`.

**ANTES:** ambos describen una arquitectura que ya no es la del repositorio:

- `ROADMAP.md` da por pendientes los tests de integración con
  `docker-compose` (llama.cpp, gateway, langfuse, joern, codebadger, mem0,
  graphiti, neo4j, dafny, TLA+, Marabou, ERAN…). Ese stack no forma parte del
  lai actual y su validación no es un objetivo del proyecto.
- `ESTADO_ACTUAL.md` describe `lai.py` como CLI principal y "16 tools" cuando la
  implementación principal es Rust (`lai_rust/`) con otro número de herramientas;
  también fija la versión en `0.0.2`, que ya no coincide con `lai_rust/Cargo.toml`.

**DESPUÉS:** ninguno de los dos se borra (contienen historia), pero dejan de
inducir a error:

- `ROADMAP.md`: aviso de vigencia al inicio + el bloque de tareas pendientes se
  etiqueta como histórico y se redirige a `swebench/README.md` y
  `DOCS/CAMBIOS_2026-11-26_PENDIENTES.md` como pendientes reales.
- `ESTADO_ACTUAL.md`: aviso de vigencia (el documento describe el CLI Python del
  2026-11-21), sección final **"Estado actual (Rust) — verificado 2026-11-26"**
  con tabla de comprobaciones, 19 subcomandos, 13 herramientas y la lista de
  cosas **no** verificadas (`cargo fmt`/`clippy` no instalados). La sección de
  versiones ahora distingue Rust 0.1.0 y Python 0.0.2.

**Verificación:** versiones leídas de `lai_rust/Cargo.toml` (0.1.0) y
`pyproject.toml` (0.0.2); recuentos de subcomandos y herramientas obtenidos de
`main.rs` y `tools.rs`; `python scripts/check_docs.py` sin fallos.

---

## 7. `run_benchmark.sh`: rutas absolutas del autor

**Archivos afectados:** `run_benchmark.sh`, `BENCHMARKS.md` (solo si hay medición).

**ANTES:** el script contiene rutas absolutas de la máquina del autor:

```
cd /home/kubuntu/lai/lai_rust
OUTPUT=$(/home/kubuntu/lai/lai_rust/target/release/lai ...)
```

En cualquier otro equipo el script falla o, peor, usa un binario ajeno. Además
`set -euo pipefail` combinado con la sustitución de `OUTPUT=$(...)` hace que un
fallo del agente aborte el benchmark entero en vez de registrar `FAIL`.

**DESPUÉS:**

- `LAI_BIN` (con valor por defecto `$SCRIPT_DIR/lai_rust/target/release/lai`)
  sustituye las rutas absolutas; el script comprueba que el binario existe y es
  ejecutable antes de empezar.
- `BUILD=1` recompila opcionalmente; por defecto no se compila nada (medir sin
  recompilar evita mezclar el tiempo de `cargo` con el del agente).
- `set -uo pipefail` en lugar de `set -euo pipefail`: un fallo del agente ya no
  aborta la corrida. Se captura `AGENT_RC` y se guarda en `results.list` y en
  cada `*_result.json`.
- `sha256sum` con *fallback* a `shasum -a 256` (macOS no trae `sha256sum`).
- `jq` deja de ser obligatorio: si falta, `summary.json` guarda `"results": []`
  y se avisa; los `*_result.json` individuales siguen completos.

**Verificación:** corrida real ejecutada el 2026-11-26 en este equipo (Windows,
`bash` de Git para Windows, binario `lai.exe` 0.1.0 en `--release`):

```
LAI_BIN=/c/DESA/lai/lai_rust/target/release/lai.exe ./run_benchmark.sh \
    qwen2.5-coder:3b http://127.0.0.1:11434/v1 true
```

Resultado: 0 PASS / 1 PARTIAL / 2 FAIL, 870.5 s totales
(255.6 s + 391.6 s + 223.3 s). El script llegó al final sin abortar, registró
`agent_rc` en cada tarea y avisó de la ausencia de `jq`. Detalle y hardware en
`BENCHMARKS.md`.

> **Una corrida de 3 tareas no es evidencia de calidad del modelo** (Regla 11).
> Se registra como medición puntual, no como conclusión.

---

## 8. Pendientes que siguen bloqueados

| Pendiente | Motivo | Qué falta |
|-----------|--------|-----------|
| Piloto SWE-bench 10–20 instancias | Docker detenido | Equipo con Docker + ~30–60 GB de disco |
| Tabla de resultados en `BENCHMARKS.md` | Depende del piloto | Corrida real |
| Efecto de `OLLAMA_CONTEXT_LENGTH=16384` en SWE-bench | Depende del piloto | Corrida real, una variable a la vez |

Nada de esto se ha ejecutado. **No hay cifras que publicar.**
