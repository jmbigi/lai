# Cambios 2026-11-26 (tercera tanda) — Docker desbloqueado y dataset descargado

Registro de cambios según **Regla 10**. Continúa
`CAMBIOS_2026-11-26.md`, `CAMBIOS_2026-11-26_PENDIENTES.md` y
`CAMBIOS_2026-11-26_MEJORAS2.md`.

## 0. Resumen

| # | Cambio | Estado |
|---|--------|--------|
| 1 | Docker Desktop actualizado (4.22.0 → 4.100.1, servidor 29.9.3) | ✅ verificado |
| 2 | `swebench/docker_env.py` + `test_docker_env.py` | ✅ 15 tests |
| 3 | Descarga del dataset sin dependencias (`--via api`) | ✅ 500 instancias |
| 4 | `swebench/make_oracle_predictions.py` | ✅ creado |
| 5 | Traducción loopback → `host.docker.internal` en `run_eval.py` | ✅ 4 tests |
| 6 | `build_lai_for_swebench.sh`: rustup y rutas de montaje | ⏳ compilando |
| 7 | Piloto de 5 instancias | ⏳ pendiente |

## 1. El entorno estaba roto, no el harness

**ANTES:** todas las etapas morían con «Docker no está disponible» después de
30–60 s de espera. El diagnóstico que daba el código no permitía distinguir
entre «Docker apagado», «motor colgado» y «versión incompatible».

**DESPUÉS:** causa raíz identificada y corregida, y el diagnóstico ahora es
explícito.

| Dato | Valor |
|------|-------|
| Docker Desktop instalado | 4.22.0 (2023) |
| WSL | 3.0.5.0, kernel 6.18.58.1-1 |
| Síntoma | pipes existentes, `docker info` sin respuesta, timeout a los 30 s |
| Causa | Docker Desktop 4.22 es incompatible con WSL 3.0.5 |
| Solución | `winget upgrade --id Docker.DockerDesktop --include-unknown` |
| Resultado | 4.100.1.247138, servidor **29.9.3**, `docker version` en **1 s** |

Errores propios durante el diagnóstico, registrados para no repetirlos:

1. `Set-Content -Encoding UTF8` de PowerShell escribe **BOM**, y Docker Desktop
   no arranca con él (`invalid character '\ufeff'`). Se reescribió el JSON con
   Python y `encoding="utf-8"`.
2. Subir la VM a 4 GB con 3.7 GB libres en el host dejó el sistema sin memoria.
   Se volvió a 2 GB.

También se perdió la imagen descargada previamente: el VHDX pasó de 11.76 GB a
1.42 GB tras el reinicio del motor. Es un coste real de ~10 min de descarga, no
un bug del harness.

## 2. `docker_env.py`: el diagnóstico deja de ser adivinación

**ANTES:** `prepare.py`, `run_eval.py` y `evaluate.py` tenían cada uno su propia
copia de `docker_available()` con un `subprocess.run` de 30 s y un mensaje
genérico.

**DESPUÉS:** un módulo único, sin dependencias:

```python
server_version(timeout=30) -> (version | None, motivo)
major_of(version) -> int | None
check(timeout=30, quiet=False) -> bool   # imprime causa + pistas accionables
describe() -> str                        # para los informes
```

`check()` avisa (sin fallar) si el servidor es anterior a la versión 20, porque
ese es el síntoma típico de un Docker Desktop desactualizado frente a WSL. Las
tres etapas lo consumen.

Verificación: `swebench/test_docker_env.py`, 15 tests offline (sin Docker).

## 3. Dataset descargado sin instalar nada

**ANTES:** `fetch_dataset.py` exigía `pip install datasets`, que no estaba
disponible. Con el paquete ausente abortaba con código 2.

**DESPUÉS:** tres vías y ninguna obligatoria.

| `--via` | Implementación | Dependencias |
|---------|----------------|--------------|
| `auto` (defecto) | `datasets` si existe, si no la API | ninguna |
| `api` | `urllib` contra `datasets-server.huggingface.co/rows`, 100 filas por página | ninguna |
| `datasets` | `load_dataset` | `datasets` |

Resultado real: **500 instancias** en `swebench/data/instances.jsonl`, con todos
los campos que necesita la evaluación (`image`, `eval_script`, `log_parser`,
`eval_type`, `FAIL_TO_PASS`, `PASS_TO_PASS`).

| Repo | Instancias | Parser |
|------|-----------:|--------|
| django | 231 | `parse_log_django` |
| sympy | 75 | `parse_log_sympy` |
| sphinx | 44 | `parse_log_pytest_v2` |
| matplotlib | 34 | `parse_log_matplotlib` |
| scikit-learn | 32 | `parse_log_pytest_v2` |
| astropy | 22 | `parse_log_pytest_v2` |
| xarray | 22 | `parse_log_pytest` |
| pytest | 19 | `parse_log_pytest` |
| pylint | 10 | `parse_log_pytest_options` |
| requests | 8 | `parse_log_pytest_options` |
| seaborn | 2 | `parse_log_seaborn` |
| flask | 1 | `parse_log_pytest` |

Dificultad: 194 `<15 min fix`, 261 `15 min - 1 hour`, 42 `1-4 hours`,
3 `>4 hours`. `eval_type` es `pass_and_fail` en las 500.

## 4. `make_oracle_predictions.py`: validar el evaluador sin gastar CPU

**ANTES:** no había forma de saber si un `resolved: 0` venía del modelo o de un
evaluador roto. La única manera era correr el agente y confiar en el resultado.

**DESPUÉS:** genera predicciones usando el `patch` oficial del dataset
(`model_name_or_path='oracle-gold-patch'`). Si el evaluador es correcto, todas
deben salir `resolved`. Si no, el fallo está en `evaluate.py`.

```bash
python3 swebench/make_oracle_predictions.py --limit 5
python3 swebench/evaluate.py --predictions swebench/data/predictions/oracle.jsonl
```

## 5. El loopback del host no existe dentro del contenedor

**ANTES:** `run_eval.py` pasaba `--base-url` tal cual al agente. Con
`http://127.0.0.1:11434/v1` el agente dentro del contenedor no llegaba a Ollama.

**DESPUÉS:** medido con la imagen de django, 2026-11-26:

| URL | `--network host` | bridge |
|-----|------------------|--------|
| `http://127.0.0.1:11434` | ❌ falla | ❌ falla |
| `http://host.docker.internal:11434` | ✅ `0.32.15` | ✅ `0.32.15` |

`container_base_url()` traduce el loopback conservando puerto y ruta, y
`container_base_url_warning()` avisa en el *preflight* para que el usuario
entienda qué está pasando. 4 tests nuevos en `test_run_eval.py`.

## 6. Script de compilación: dos bugs reales

**ANTES:** fallaba siempre, por dos motivos distintos y acumulativos.

**DESPUÉS:**

| Bug | Mensaje | Causa | Arreglo |
|-----|---------|-------|---------|
| 1 | `/cargo/env: No such file or directory` | El caché de rustup venía de otra versión y no tenía `env` | Exportar `CARGO_HOME`, `RUSTUP_HOME`, `PATH` explícitamente; instalar con `--no-modify-path`; comprobar `command -v cargo` |
| 2 | `cp: cannot stat '/src/.'` | Git Bash pasa `/c/DESA/...` y Docker Desktop no monta esa ruta | `to_host_path()` con `cygpath -m` → `C:/DESA/...` |

El script sigue compilando en `ubuntu:22.04` (glibc 2.35) con `--locked`, y
copia el binario a `swebench/bin/lai`.

## 7. Verificación

| Comprobación | Resultado |
|--------------|-----------|
| `python -m unittest swebench.test_docker_env test_prepare test_run_eval test_harness_offline test_evaluate` | **92 tests OK** |
| `python swebench/fetch_dataset.py --via api` | 500 instancias |
| `docker version` | servidor 29.9.3 en 1 s |
| Red del contenedor | `host.docker.internal` funciona en host y bridge |
| `evaluate.py` con parches oficiales (oracle5) | **5/5 resueltas** |

## 8. Bugs reales encontrados al ejecutar por primera vez

Ejecutar de verdad destapó tres fallos que los tests offline no podían ver:

| Bug | Síntoma | Causa | Arreglo |
|-----|---------|-------|---------|
| Temporales compartidos | `PermissionError: state.tmp` al lanzar dos `prepare.py` a la vez | Todos los `save_state` usaban el mismo `.tmp` | Nombre único `.{pid}.{hex}.tmp` en `prepare.py`, `evaluate.py`, `run_eval.py` y `fetch_dataset.py`, con limpieza en `finally` |
| `PASS_TO_PASS` vacío rechazado | `❌ Datos incompletos: faltan PASS_TO_PASS` en django y sphinx | El dataset oficial trae `[]` legítimamente | `ALLOW_EMPTY_LIST_FIELDS`: la clave debe existir, la lista puede estar vacía |
| Aviso de campos faltantes | Ruido en la descarga | Mismo criterio `if not value` | Ahora sólo avisa si es `None` o cadena vacía |

Verificado con `swebench.test_evaluate.ValidationTests` (3 tests nuevos) y
`swebench.test_prepare.StatePersistenceTests` (2 nuevos).

## 9. Estado de las 5 imágenes del piloto

| Instancia | Tamaño | Parser que ejercita |
|-----------|-------:|---------------------|
| django__django-12209 | 2.88 GB | `parse_log_django` |
| sphinx-doc__sphinx-8595 | 2.81 GB | `parse_log_pytest_v2` |
| sympy__sympy-20916 | 2.77 GB | `parse_log_sympy` |
| scikit-learn__scikit-learn-14141 | ~2.8 GB | `parse_log_pytest_v2` |
| pytest-dev__pytest-5809 | ~2.8 GB | `parse_log_pytest` |

Las cinco están `ready` en `data/state.json`.

## 10. Validación del evaluador con parches oficiales (oracle5)

Un evaluador que siempre dice «no resuelto» parece correcto y no lo es. Para
comprobarlo sin agente se generaron las predicciones con el `patch` oficial del
dataset (`make_oracle_predictions.py`) y se pasaron por `evaluate.py`. Si el
evaluador está bien, **todas** deben salir resueltas.

| Instancia | Parser | F2P | P2P | Exit tests | Tiempo | Resuelta |
|-----------|--------|-----|-----|-----------:|-------:|----------|
| django__django-12209 | `parse_log_django` | 1/1 | 0/0 | 0 | 174.6 s | sí |
| pytest-dev__pytest-5809 | `parse_log_pytest` | 1/1 | 3/3 | 0 | 49.6 s | sí |
| scikit-learn__scikit-learn-14141 | `parse_log_pytest_v2` | 1/1 | 2/2 | 0 | 88.7 s | sí |
| sphinx-doc__sphinx-8595 | `parse_log_pytest_v2` | 1/1 | 0/0 | 0 | 57.3 s | sí |
| sympy__sympy-20916 | `parse_log_sympy` | 1/1 | 1/1 | 0 | 50.5 s | sí |

**Resultado: 5/5 resueltas (100 %), 0 errores de infraestructura.** Esto valida
de punta a punta el wrapper del parche, la inyección de `SWEBENCH_TEST_EXIT_CODE`,
los cinco parsers implicados y el cálculo de F2P/P2P. Es una comprobación del
evaluador, **no** un resultado del agente: el parche lo escribió una persona.

Efecto secundario útil: `django__django-12209` y `sphinx-doc__sphinx-8595` tienen
`PASS_TO_PASS` vacío, así que este ejercicio también confirma el arreglo del
bug anterior.

## 11. Bug del script de compilación: copiaba `target/` antes de borrarlo

El build se quedó «colgado» más de 19 minutos en `cp -r /src/. /build/`. No era
un cuelgue: estaba copiando los **7.2 GB** de `lai_rust/target` del host al disco
virtualizado, para ejecutar `rm -rf /build/target` justo después. Diagnóstico con
`docker exec ... ps aux`: `cp -r /src/. /build/` llevaba 5:28 de CPU y seguía
en estado `D`.

Arreglo: sustituir la copia por una tubería de `tar` que excluye desde el
principio.

```bash
tar -C /src --exclude=./target --exclude=./.git -cf - . | tar -C /build -xf -
```

Es el tercer bug del mismo script (los otros dos: `. /cargo/env` inexistente y
rutas `/c/DESA/...` de Git Bash). Todos aparecieron sólo al ejecutarlo de verdad.

## 12. Regla 4 y contenedores: aceptar los alias de Docker

El piloto ejecuta `lai` **dentro** del contenedor de SWE-bench. La única vía
medida para llegar al Ollama del host es `http://host.docker.internal:11434/v1`
(`127.0.0.1` no funciona ni en `host` ni en bridge). Pero `is_local_url()`
rechazaba ese host por ser un nombre DNS, así que la guarda de la Regla 4 habría
abortado cada ejecución del piloto.

Arreglo: `is_local_url()` acepta ahora `host.docker.internal` y
`gateway.docker.internal`. No es una excepción a la Regla 0: esos alias resuelven
a la propia máquina, y son la forma estándar de alcanzar un servicio local desde
un contenedor. Cualquier DNS público sigue rechazado sin `--allow-remote`.

Test nuevo: `accepts_docker_host_aliases` (3 URLs). `cargo test --lib llm::` →
6 tests OK.

## 13. Otros ajustes de `run_eval.py` para el piloto

- El binario se monta en `/tmp/lai_bin` (directorio) y dentro del contenedor se
  copia a `/usr/local/bin/lai` con `chmod +x`. Antes se montaba directamente
  sobre `/usr/local/bin/lai`: en Windows el bit de ejecución no viaja con el
  archivo, así que el comando podía fallar con «Permission denied».
- Las rutas de `--mount` se convierten con `.as_posix()`; Docker Desktop no
  digiere bien las barras invertidas en `type=bind,source=...`.
- Nuevos códigos de salida clasificados como error de infraestructura: `90`
  (`/testbed` no existe) y `91` (no se pudo copiar el binario). Se añadieron 2
  tests a `ClassifyResultTests`.

## 14. El binario del piloto rechazaba `host.docker.internal`

Primer smoke test real del agente (`run_eval.py --instance django__django-12209
--model qwen2.5-coder:7b`): el contenedor arrancó, el binario se copió y el
agente recibió el enunciado, pero abortó en la primera iteración:

```
Error: Regla 4: el endpoint 'http://host.docker.internal:11434/v1' no es local.
```

Dos causas acumuladas:

1. **El binario era anterior al arreglo.** Se compiló a las 14:22 y el cambio en
   `is_local_url()` se hizo después. Un binario viejo hace exactamente lo que
   decía la versión vieja del código.
2. **Se clasificaba mal.** El resultado salió como `no_patch` porque el diff
   vacío es válido formalmente. Un rechazo de la Regla 4 no es «el modelo no
   propuso nada», es configuración rota: ahora `classify_result()` lo detecta
   (`"regla 4"` + `"no es local"`) y devuelve `infra_error`.

También se corrigió un detalle que solo aparece con el bind mount de Windows:
montar el binario sobre `/tmp/lai_bin` como directorio hacía que `cp` fallara con
`Not a directory`. El destino ahora es el archivo `/tmp/lai_bin/lai`.

**Aprendizaje de proceso:** recompilar el binario *después* de tocar `llm.rs` no
es opcional. El harness no puede detectar que el binario está desactualizado, así
que conviene reconstruirlo en la misma tanda que el cambio.

## 15. El build recompilaba desde cero cada vez

`CARGO_TARGET_DIR=/tmp/target` vivía dentro del contenedor efímero, así que cada
compilación repetía las ~25 minutos de dependencias. Ahora apunta a
`/cargo/target`, dentro del volumen ya montado para el caché de rustup. La
primera compilación cuesta lo mismo; las siguientes reaprovechan todo.

## 16. Resumen reproducible: `summarize.py` y contexto medido

**ANTES:** para pasar los resultados a `BENCHMARKS.md` había que abrir a mano el
`meta.json` y el `report.json` y cruzarlos. El contexto que se anotaba era el que
se suponía, no el que Ollama tenía cargado.

**DESPUÉS:**

- `run_eval.py` consulta `/api/ps` **después** de cada corrida (cuando el modelo
  ya está cargado) y guarda `context_length` en los metadatos. Si no se puede
  saber, queda `null`: no se rellena con un valor por defecto.
- `read_context()` factoriza esa lectura y la reutiliza el preflight. Se
  consulta `/api/ps` y, si el modelo no está cargado, `/api/show` (`num_ctx`).
- `summarize.py` cruza metadatos y evaluación y emite la tabla markdown:

  ```
  python3 swebench/summarize.py --model qwen2.5-coder:7b --md
  ```

  Lo no medido aparece como `—`. Si falta la evaluación lo dice en vez de
  mostrar un cero. También resume evaluaciones sin metadatos de agente (por
  ejemplo la validación con parches oficiales).

Medido durante el piloto (i7-10510U, 15.8 GB): `qwen2.5-coder:7b` Q4_K_M, 7.6B,
6.9 GB de pesos, `size_vram: 0` (**100 % CPU**) y `context_length: 32768`.

## 17. Primera corrida real del agente: el parche que destruyó Django

**ANTES:** el harness estaba validado con parches oficiales (oracle5), pero nunca
había ejecutado el agente contra una instancia real.

**DESPUÉS:** corrida de `django__django-12209` con `qwen2.5-coder:7b`:
1048.7 s, `rc=0`, parche de 81 236 B en un solo archivo, **no resuelta**.

El parche no edita el código: sustituye `django/db/models/base.py` entero. El
diff es `@@ -1,1911 +1,18 @@`: las 1911 líneas del core de Django quedaron
reemplazadas por las 18 líneas del ejemplo del issue. El agente copió el ejemplo
que ilustraba el bug y lo trató como solución.

### Bug 1 — el evaluador descartaba fallos de importación

**ANTES:** si el parser no encontraba ninguna línea de resultado y `SUITE_RAN` no
coincidía, `evaluate_log()` devolvía `no_test_results` y la instancia quedaba
**no puntuable**. Un parche que rompe la importación del paquete desaparecía del
denominador, lo que **infla** la tasa de resolución.

**DESPUÉS:** el código de salida de los tests se lee antes de esa comprobación.
Si el wrapper capturó un `SWEBENCH_TEST_EXIT_CODE`, el comando de tests se
ejecutó y la instancia **se puntúa**: con el mapa de estados vacío, todos los
`FAIL_TO_PASS` fallan y el resultado es `RESOLVED_NO`. Solo cuando no hay ni
resultados ni código de salida se devuelve `no_test_results`.

### Bug 2 — slugs distintos entre `run_eval.py` y `summarize.py`

**ANTES:** `run_eval.slug()` conservaba `.`, `-` y `_` (`qwen2.5-coder_7b`)
mientras `summarize.slug()` los convertía todos en `_` (`qwen2_5_coder_7b`). El
resumen no encontraba los metadatos y decía «sin datos de agente».

**DESPUÉS:** `summarize.py` usa la misma regex que `run_eval.py`, y además
`load_meta()` escanea todos los `*.meta.json` si el nombre derivado no existe. Un
cambio futuro de slug no rompe corridas ya hechas.

### Bug 3 — `write_file` podía destruir un archivo sin avisar

**ANTES:** `write_file` hacía `fs::write` directo. Nada impedía sustituir 1911
líneas por 18, y el error no se detectaba hasta que fallaban los tests.

**DESPUÉS:** nueva `write_file_tool()` con guarda anti-destrucción: si el archivo
existe, tiene 40+ líneas y el contenido nuevo es menos del 25 % de la longitud
original, devuelve un error explicativo que recomienda `apply_patch` o
`"allow_shrink": true`. La constante `SHRINK_MIN_ORIGINAL_LINES` documenta el
caso real que la motivó. 5 tests nuevos cubren rechazo, permiso explícito,
crecimiento, reducción moderada, `path` obligatorio y creación de directorios.

También se actualizó la guía de herramientas del prompt: `apply_patch` pasa a ser
la vía recomendada para modificar archivos existentes, y se avisa explícitamente
de que el código de ejemplo del issue **no** es la solución.

## 18. Dependencias declaradas y nunca usadas

**ANTES:** `lai_rust/Cargo.toml` declaraba 26 crates que el código no importa
nunca. Cada compilación del binario Linux (que se hace desde cero dentro de
`ubuntu:22.04`) avisaba de ellos con `unused dependency`, y todos se descargaban
y compilaban —o al menos se resolvían— sin aportar nada.

La lista se obtuvo del propio compilador, no de una suposición: `cargo check`
emite un aviso por cada dependencia declarada que ningún `extern crate`/`use`
toca. Se contrastó además con un script que busca el identificador de cada crate
en `src/` y `tests/` ignorando comentarios, y coincidió.

**DESPUÉS:** se eliminaron `tree-sitter` y sus siete gramáticas (Python, Rust,
JavaScript, TypeScript, Go, Java, más el paquete base), `blake3`, `indicatif`,
`colored`, `dashmap`, `itertools`, `petgraph`, `serde_yaml`, `syn`, `quote`,
`proc-macro2`, `nix`, `tokio-tungstenite`, `tungstenite`, `rustls-pemfile`,
`webpki-roots`, `mime_guess`, `async-stream`, `tracing-subscriber` y `base64`.
También los comentarios que agrupaban bloques ya vacíos («Tree-sitter for
structural code analysis», «File hashing for incremental indexing», «Graph
algorithms for planning»).

Se conservaron a propósito `rustls`, `mime`, `percent-encoding` y `rand`: no
aparecen en el código pero son plausibles como dependencias indirectas o de
feature, y quitarlas no aportaba nada medible.

**Corrección posterior (misma tanda).** Esa prudencia estaba mal calibrada en un
caso: la compilación cruzada dentro de `ubuntu:22.04` emitió
`warning: unused dependency 'rand'`. Se comprobó con
`grep -E '\brand::|use rand'` sobre todo `lai_rust/` (cero coincidencias) y se
eliminó. Las otras tres no generan aviso, así que se quedan. `cargo check
--all-targets` sigue en 0 avisos.

Lección de método: el aviso de Cargo es la fuente de verdad, no la intuición
sobre qué podría ser una dependencia indirecta. Si Cargo no se queja, se queda;
si se queja, se va.

`cargo check --all-targets` ya no emite ningún aviso `unused dependency`. Los 103
tests siguen pasando y `clippy` sigue en 0 avisos, así que la limpieza no cambió
comportamiento: solo quitó peso.

## 19. `run_pilot.sh` encadena evaluación y resumen

**ANTES:** el script lanzaba las 5 instancias y terminaba imprimiendo un mensaje
recordatorio de que había que evaluar «a mano». Dos problemas:

1. No usaba `--force`, así que relanzar el piloto se saltaba las instancias que
   ya tenían predicción y el script parecía no hacer nada.
2. El usuario tenía que copiar y pegar el comando de `evaluate.py` con el nombre
   del archivo de predicciones correcto (que depende del slug del modelo) y
   luego el de `summarize.py`.

**DESPUÉS:** el script genera un `RUN_ID` con marca de tiempo, pasa `--force` a
cada instancia y, al terminar, encadena `evaluate.py --run-id $RUN_ID --force` y
`summarize.py --model $MODEL --run-id $RUN_ID`. Un solo comando produce la tabla
lista para `BENCHMARKS.md`. `SKIP_EVAL=1` deja solo la fase de agente y
`RUN_ID=...` fija el identificador del informe. Verificado con `bash -n`.

## 20. El agente no sabía que su parche había fallado

Segunda instancia del piloto (`sphinx-doc__sphinx-8595`, `qwen2.5-coder:7b`).
Resultado: **timeout a los 1505.5 s con 0 bytes de parche**. El log del agente
explica por qué, y no es lo que parecía a primera vista.

El modelo propuso un `apply_patch` razonable, el parche no encajó (el contexto
no existía en el archivo), y el agente recibió `✓ Done`. Repitió **la misma
llamada idéntica cuatro veces** y agotó el tiempo. Dos bugs encadenados:

**Bug 1 — `apply_patch` devolvía éxito al fallar.** `apply_patch_tool` terminaba
con `Ok(format!("Patch failed.\ngit apply:\n..."))`. Un `Ok` con texto de error
dentro es indistinguible de un éxito para el bucle del agente, que imprime
`✓ Done`. Arreglo: devolver `Err` con el stderr real y una instrucción concreta
(«lee el archivo primero y construye el diff contra lo que viste»).

**Bug 2 — no había detección de bucles.** El agente no tiene memoria de sus
propios intentos fallidos. Arreglo en `cmd_auto`: un `HashMap` con la firma
`herramienta + argumentos`; a partir de la cuarta llamada idéntica no se ejecuta,
se le devuelve al modelo un mensaje que le dice que no repita y que cambie de
estrategia.

**Bug 3 (Windows) — un parche fallido ensuciaba el árbol.** Medido con
`examples/patch_probe.rs`: `patch -p1` reescribe el archivo con CRLF y deja
`.orig`/`.rej` **aunque el hunk falle**. Arreglo: `git apply --check` primero y
solo después `git apply`; `patch` queda como último recurso únicamente cuando el
directorio no es un repositorio, y también con `--dry-run` previo y
`--no-backup-if-mismatch`. Ahora un parche fallido no escribe nada.

Verificación: `cargo test --lib` → **70 passed** (3 tests nuevos:
`apply_patch_reports_failure_as_error`, `apply_patch_applies_a_valid_hunk_in_a_repo`,
`apply_patch_requires_patch_text`). El test de fallo comprueba además que no
quedan `.rej` ni `.orig`.

**Observación medida sobre el tiempo.** `django__django-12209` completó 10
iteraciones en 1048.7 s; `sphinx-doc__sphinx-8595` hizo 4 en 1505.5 s. El coste
por iteración varía con el tamaño del contexto leído, así que 1500 s no es un
límite universal: para repos grandes con un 7B en CPU hay que subir `TIMEOUT`.

## 21. Pendientes

| Pendiente | Estado |
|-----------|--------|
| Compilar `swebench/bin/lai` (glibc 2.35) | ✅ `lai 0.1.0`, 6.7 MB |
| Validar el evaluador con parches oficiales (`oracle5`) | ✅ **5/5 resueltas** |
| Primera corrida real del agente | ✅ 1 instancia, **no resuelta** |
| Piloto de 5 instancias | 2/5 evaluadas (django no resuelta, sphinx timeout); sympy, scikit-learn y pytest en curso |
| Tabla representativa en `BENCHMARKS.md` | bloqueada hasta terminar el piloto |
| Dependencias muertas | ✅ 27 eliminadas, 0 avisos |

Con dos instancias no se puede afirmar nada sobre la calidad del modelo
(Regla 11). Lo que sí se puede afirmar, y está medido: el agente destruyó un
archivo de 1911 líneas en una instancia, y en otra repitió cuatro veces un
parche que no se aplicaba. Ambas son observaciones del comportamiento del
sistema, no impresiones.
