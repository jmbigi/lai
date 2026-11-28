# Cambios 2026-11-28 — Regla 12: estimación SWE de modelos locales

Registro según **Regla 10**. Continúa `DOCS/CAMBIOS_2026-11-28_LLAMACPP.md`.

## 0. Punto de partida

| Comprobación | Resultado inicial |
|---|---|
| Reglas del proyecto | 11 reglas; ninguna definía cómo reportar capacidad SWE |
| Cifras SWE publicadas | «2/10 (20 %)» y «25 % sobre puntuadas» sin intervalo de confianza |
| Herramienta de estimación | ❌ no existía |
| Muestras | n=10; sin cálculo de cuántas instancias harían falta |
| Riesgo | presentar un 20 % con n=10 como si distinguiera modelos |

## 1. Regla 12 (nueva)

Añadida a `DOCUMENTACION_REGLAS.md` con su fila en la tabla de cumplimiento.
Exige, para toda afirmación de capacidad SWE:

- **denominador explícito** (intentadas y evaluables), nunca un porcentaje suelto;
- **intervalo de Wilson 95 %** en toda tasa;
- **etiqueta PRELIMINAR con n < 30**, sin uso para comparar modelos;
- **cero resueltas ≠ 0 %**: se publica la cota superior por regla de tres (≈ 3/n);
- **contexto de corrida** (modelo, cuantización, contexto, GPU/CPU, versión de
  llama.cpp, timeout, red, fecha, commit);
- **extrapolación solo si se declara** y con su supuesto;
- **errores de infraestructura separados** de los fallos del modelo;
- **prohibido** presentar cifras de papers/cloud como propias, o pass@k sin
  declarar k.

## 2. `swebench/estimate.py` (implementación canónica)

**ANTES:** el resumen (`summarize.py`) producía una tabla sin estadística; el
intervalo de confianza y el tamaño de muestra había que calcularlos a mano.

**DESPUÉS:** script que lee un `report.json` y calcula:

- tasa extremo a extremo y sobre evaluables;
- intervalo de Wilson 95 % para ambas;
- cota superior por regla de tres si no hay resueltas;
- desglose por repositorio;
- n necesario para ±5 pp y ±10 pp (p≈20 % y p≈50 %);
- extrapolación opcional a 500, marcada como extrapolación.

Salidas: texto, `--json` y `--md`. Falla en voz alta si el informe no existe o
no es válido.

## 3. `lai swe-estimate` (CLI Rust)

Misma matemática en `lai_rust/src/estimate.rs` para quien solo tiene el binario:
`SweEstimate::from_report`, `wilson_interval`, `rule_of_three`,
`sample_size_for_margin`, `render_text`, `extrapolation`. 10 tests fijados a los
mismos valores que la versión Python (2/10 → 20 % [5.7 – 51.0]).

## 4. Resultado sobre la corrida real

`qwen3-8b-final` (10 instancias, 2026-11-27):

| Métrica | Valor |
|---|---:|
| Tasa extremo a extremo | **20.0 %** [5.7 – 51.0] |
| Tasa sobre evaluables | 25.0 % [7.1 – 59.1] |
| Errores de infraestructura | 0 |
| Etiqueta | **PRELIMINAR** (n=10 < 30) |

Muestra necesaria: ±10 pp → n ≥ 62 (p≈20 %) / n ≥ 97 (p≈50 %); ±5 pp →
n ≥ 246 / n ≥ 385.

## 4b. Segunda corrida medida (2026-11-28, contexto 16384)

Se repitió la corrida con el servidor local ya configurado (`-ngl 99 -c 16384`)
para tener una segunda estimación del mismo modelo con una variable distinta:

| Métrica | `qwen3-8b-final` (18432) | `qwen3-8b-20261128-local` (16384) |
|---|---:|---:|
| Evaluables | 8 | 5 |
| Vacíos | 2 | 5 |
| Resueltas | 2 | 1 |
| Tasa e2e | 20.0 % [5.7 – 51.0] | 10.0 % [1.8 – 40.4] |
| Errores infra | 0 | 0 |

**No se declara ganador:** los intervalos se solapan. El mecanismo observado en
los logs es que con 16384 tokens el agente condensó el contexto 3 veces en
`astropy-13033` y agotó las 30 iteraciones sin parche válido. Queda como
hipótesis para una corrida con n mayor, no como resultado.

Esto es exactamente lo que la Regla 12 pretende evitar: dos cifras (20 % y 10 %)
que parecen una diferencia y que con n=10 no lo son.

## 5. Verificación

| Comprobación | Resultado |
|---|---|
| `pytest swebench/test_estimate.py` | ✅ 19 tests |
| `cargo test` | ✅ 240 tests (198 lib + 42 integración) |
| `lai swe-estimate …` sobre la corrida real | ✅ 20.0 % [5.7 – 51.0], PRELIMINAR |
| Segunda corrida real (16384) evaluada | ✅ 1/10 [1.8 – 40.4], PRELIMINAR |
| `run_pilot.sh` encadena la estimación | ✅ `bash -n` |
| `make swe-estimate` | ✅ |
| `python swebench/estimate.py --run-id qwen3-8b-final` | ✅ idéntico |
| `--json` y `--md` | ✅ |
| `ruff check .` | ✅ |
| `cargo fmt` / `clippy -D warnings` | ✅ |

## 6. Lo que NO se ha probado (Regla 6)

- **Corrida con n ≥ 30.** No se ejecutó; por eso todo sigue etiquetado
  PRELIMINAR. La estimación es correcta, la muestra no.
- **Comparación entre modelos.** No hay dos modelos con el mismo n y harness;
  la Regla 12 prohíbe comparar sin eso.
- **Intervalos con correlación entre instancias.** Wilson asume independencia;
  instancias del mismo repo pueden estar correlacionadas, así que el intervalo
  real podría ser algo más ancho. Declarado, no corregido.
