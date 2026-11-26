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

## 17. Pendientes

| Pendiente | Estado |
|-----------|--------|
| Compilar `swebench/bin/lai` (glibc 2.35) | ✅ `lai 0.1.0`, 6.7 MB |
| Validar el evaluador con parches oficiales (`oracle5`) | ✅ **5/5 resueltas** |
| Piloto de 5 instancias con el agente | en curso |
| Tabla representativa en `BENCHMARKS.md` | depende del piloto |

No hay cifras de agente que publicar todavía (Regla 11).
