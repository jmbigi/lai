# Cambios 2026-11-28 — Muestra n≥62 y correlación intra-repositorio

Registro según **Regla 10**. Continúa `DOCS/CAMBIOS_2026-11-28_REGLA12.md`.

## 0. Punto de partida

| Comprobación | Resultado inicial |
|---|---|
| Muestra SWE medida | n=10 (`qwen3-8b-final`) y n=10 (`qwen3-8b-20261128-local`) |
| Regla 12 | exigía n ≥ 62 para afirmar una tasa |
| Intervalo publicado | Wilson 95 %, que **asume independencia entre instancias** |
| Riesgo declarado | instancias del mismo repo están correlacionadas → IC real más ancho |
| Imágenes Docker listas | 10 de 500 |

## 1. Muestra reproducible de 62 instancias

**ANTES:** las dos corridas usaban 10 instancias elegidas a mano (una por
parser), no una muestra aleatoria del dataset. Eso sesga la estimación.

**DESPUÉS:** `swebench/data/sample62.txt` contiene **62 instancias aleatorias**
de las 500 de SWE-bench Verified, con semilla fija `20261128` (reproducible):

```python
random.seed(20261128)
sample = sorted(random.sample(all_500_instance_ids, 62))
```

Distribución resultante (9 repos, proporciones cercanas al dataset):

| Repo | n |
|---|---:|
| django | 33 |
| sympy | 10 |
| matplotlib / pydata / pylint-dev / scikit-learn / sphinx-doc | 3 cada uno |
| astropy / pytest-dev | 2 cada uno |

## 2. Correlación intra-repositorio (ICC y bootstrap por clusters)

**ANTES:** el intervalo era Wilson puro. Con instancias agrupadas por repo, la
varianza real es mayor que la binomial: el IC quedaba demasiado estrecho.

**DESPUÉS** (`swebench/estimate.py` y `lai_rust/src/estimate.rs`):

- **ICC** con el estimador ANOVA de una vía para resultado binario:
  `ICC = (MSB − MSW) / (MSB + (m̄−1)·MSW)`, con varianza intra-cluster
  `p(1−p)` de Bernoulli.
- **Efecto de diseño** `DE = 1 + (m̄−1)·ICC`, acotado a `[1, n]`.
- **n efectivo** `n/DE`, y Wilson calculado con ese n.
- **Bootstrap por clusters**: 10 000 remuestreos de **repositorios** (no de
  instancias), IC por percentiles 2.5/97.5, semilla fija para reproducibilidad.
- El texto y el markdown muestran las tres cifras y advierten de que **el IC de
  clusters es el que debe citarse** cuando la muestra mezcla repositorios.

Verificación de la implementación:

| Caso | ICC | DE |
|---|---:|---:|
| Clusters sin correlación (`1/1, 0/1, 1/1, 0/1`) | 0.000 | 1.00 |
| Clustering perfecto (`3/3, 0/3, 0/3, 0/3`) | 1.000 | 3.00 |
| Caso mixto (`0/3, 1/1, 0/1, 1/2, 0/1`) | 0.238 | 1.14 |

Y un test comprueba que, con clustering fuerte, el IC por bootstrap es **más
ancho** que Wilson — que es exactamente el motivo de este cambio.

## 3. Orquestación de la corrida grande

**ANTES:** `run_eval.py` exigía tener todas las imágenes antes de empezar.

**DESPUÉS:** `swebench/run_ready.sh <muestra>` lee `state.json`, selecciona solo
las instancias con imagen `ready` y las ejecuta. Permite encadenar pasadas
mientras `prepare.py` sigue descargando, sin registrar errores de
infraestructura por imágenes ausentes.

## 4. Verificación

| Comprobación | Resultado |
|---|---|
| `pytest swebench/test_estimate.py` | ✅ 27 tests |
| `cargo test` | ✅ 248 tests (206 lib + 42 integración) |
| `cargo fmt --check` / `clippy -D warnings` | ✅ |
| `ruff check .` | ✅ |
| ICC/DE coinciden entre Python y Rust | ✅ (mismo caso mixto) |
| `lai swe-estimate` muestra ICC, DE, n efectivo y bootstrap | ✅ |
| `run_ready.sh` | ✅ `bash -n` y ejecución real |

## 5. Lo que NO se ha probado (Regla 6)

- **Segundo modelo con el mismo n y harness.** No hay un segundo modelo de
  código disponible en local (solo Qwen3-8B y un instruct de 725M). Comparar
  modelos exige descargar otro GGUF y repetir las 62 instancias; queda como paso
  siguiente documentado, no como resultado.
- **Corrección de ICC para muestras pequeñas.** El estimador ANOVA es
  insesgado solo de forma aproximada con pocos clusters (9 repos). El bootstrap
  por clusters es la salvaguarda, pero con 9 clusters su resolución también es
  limitada.
- **pass@k.** Todas las cifras son pass@1 (un intento por instancia).
