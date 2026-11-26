# Cambios propuestos y aplicados — tanda 2 (2026-11-26)

Registro exigido por la **Regla 10** (documentación obligatoria de cambios).
Cada punto lleva su estado **ANTES** y **DESPUÉS**, más la verificación real
ejecutada. Regla 6 (honestidad): no se afirma nada que no se haya comprobado.

## 0. Resumen

| # | Cambio | Tipo | Verificación |
|---|--------|------|--------------|
| 1 | Guarda local (Regla 4) implementada en `llm.rs` | bug de cumplimiento | 5 tests nuevos + `cargo test` |
| 2 | Parser de tool calls JSON reparado | bug funcional crítico | 7 tests nuevos + smoke test end-to-end |
| 3 | `cargo fmt` / `cargo clippy` instalados y limpios | calidad | `cargo clippy --all-targets` = 0 avisos |
| 4 | Avisos de compilación eliminados | calidad | `cargo build --release` sin avisos |
| 5 | Rutas absolutas del autor eliminadas (`services.rs`) | portabilidad | `cargo test` |
| 6 | `run_competition.sh` / `run_stress.sh` reescritos | portabilidad | `bash -n` + ejecución real |
| 7 | Suite completa de tests ejecutada | verificación | Rust 90, Python 113 |
| 8 | Medición de contexto y varianza | medición (Regla 11) | ver sección 8 |
| 9 | `fix_main.py` eliminado | limpieza | `git rm` |

## 1. Regla 4 no estaba implementada en Rust

**ANTES:** `is_local_url()` no existía en el código Rust. `LlmClient` enviaba
peticiones a cualquier URL y los parámetros `_allow_remote` de `cmd_chat`,
`cmd_code` y `cmd_auto` se ignoraban (prefijo `_`). La tabla de cumplimiento
citaba únicamente la ruta por defecto de `lai.py` (legado). Es decir: la regla
se cumplía por convención, no por código.

**DESPUÉS:** en `lai_rust/src/llm.rs`:

- `pub fn is_local_url(url: &str) -> bool` — elimina esquema, userinfo y puerto
  (incluido IPv6 entre corchetes) y acepta `localhost`, `0.0.0.0`, `::1`, `::`,
  sufijos `.localhost`/`.local`, IPv4 loopback/privadas/link-local
  (`127/8`, `10/8`, `172.16/12`, `192.168/16`, `169.254/16`) e IPv6
  `fc00::/7` y `fe80::/10`. Rechaza DNS público y direcciones como `8.8.8.8`.
- `LlmClient` gana el campo `allow_remote: bool` (por defecto `false`),
  `with_allow_remote()` y `check_endpoint() -> Result<()>`, que devuelve el
  error de Regla 4.
- `chat_request()` pasa a devolver `Result<RequestBuilder>` y llama
  `check_endpoint()?` antes de construir la petición (4 call sites ajustados).
- `commands.rs`: `cmd_chat`, `cmd_code` y `cmd_auto` construyen el cliente con
  `.with_allow_remote(allow_remote)` en lugar de ignorar el flag.

**Tests:** `accepts_loopback_endpoints`, `accepts_private_ranges`,
`rejects_public_endpoints`, `strips_userinfo_and_query`,
`check_endpoint_enforces_rule_4`.

## 2. Parser de tool calls JSON: causa raíz del 0 PASS

**ANTES:** `parse_simulated_tool_calls` hacía `response.find("```json")` +
`serde_json::from_str` y exigía la clave `"tool"`. Con `qwen2.5-coder:3b`
el modelo emitía el tool call correctamente, pero lai respondía
`No tool call emitted` y gastaba las iteraciones pidiendo al modelo "que actúe".
Causas reales observadas en el log:

1. El valor `content` llevaba **saltos de línea literales** dentro de la cadena
   JSON → JSON inválido para `serde_json`.
2. Algunas iteraciones usaban la clave `"name"` en lugar de `"tool"`.
3. Algunos modelos entregan `arguments` como **string** con JSON dentro.

**DESPUÉS:**

- `parse_simulated_tool_calls` recorre cualquier bloque ```` ``` ```` con
  etiqueta opcional (`json`, `JSON`, vacía), exige que el cuerpo empiece por
  `{` (para no confundir bloques de código Rust) y delega en
  `parse_tool_call_json`.
- `repair_json_control_chars(raw)` escapa `\n`, `\r`, `\t` y controles `< 0x20`
  **solo dentro de cadenas JSON**, respetando los escapes existentes.
- `parse_tool_call_json` intenta el parseo estricto y luego el reparado, acepta
  los alias `tool | name | tool_name | function`, acepta `arguments` como objeto
  o como string JSON y normaliza siempre a `{"tool": ..., "arguments": {...}}`.

**Tests:** 7 nuevos (`simulated_call_with_raw_newlines_is_repaired`,
`simulated_call_accepts_name_alias`, `simulated_call_accepts_string_arguments`,
`simulated_call_accepts_uppercase_fence`,
`simulated_call_ignores_rust_code_blocks`, `simulated_call_requires_arguments`,
`repair_keeps_escaped_quotes_intact`).

**Verificación end-to-end:** la misma tarea `task_1` que antes daba
`FAIL (no file changes, agent rc=0)` en 234 s ahora da `PASS (tests pass)` en
335 s con el binario release recompilado.

> Consecuencia documental: las cifras anteriores de `BENCHMARKS.md`
> (0 PASS / 1 PARTIAL / 2 FAIL) se midieron con el parser defectuoso. Se
> conservan marcadas como tales; no describen la capacidad del modelo.

## 3. rustfmt y clippy

**ANTES:** `cargo fmt` y `cargo clippy` no estaban instalados
(`rustup component add` pendiente), por lo que no se podía formatear ni lintar.

**DESPUÉS:** instalados con `rustup component add rustfmt clippy`.
`cargo fmt` aplicado y `cargo fmt --check` limpio.
`cargo clippy --all-targets` → **0 warnings, 0 errores** (antes 59 avisos).

Limpieza asociada en tests: `tests/infrastructure.rs` marcado como HISTÓRICO
con `#![allow(...)]` explícito y aviso de cabecera; `tests/swe_bench.rs`,
`tests/core.rs` y `tests/benchmarks.rs` sin imports ni variables muertas.

## 4. Avisos de compilación

**ANTES:** 4 avisos en `main.rs` (`base_url`, `quick`, `compare_file`,
`bench_file` sin usar) y 2 locales muertos en `repair_engine.rs`.

**DESPUÉS:** `BenchSwe` usa `model`, `base_url` y `quick` (los imprime) y avisa
si el archivo de comparación no existe; `.join(&model.replace(...))` →
`.join(model.replace(...))`; en `repair_engine.rs` se eliminan `config` y
`diag_config` y se reescribe `ChangeType::Delete` como guarda
`if !change.old_content.is_empty()`. `cargo build --release` sin avisos.

## 5. Rutas absolutas del autor

**ANTES:** `services.rs` contenía rutas fijas `/home/kubuntu/lai/...` y
`lai_rust/fix_main.py` era un script que reescribía `main.rs`.

**DESPUÉS:** `services.rs` resuelve los binarios por entorno con defaults:
`LAI_LLAMA_SERVER` (`llama-server`), `LAI_GATEWAY_BIN` (`gateway`),
`LAI_GATEWAY_DIR` (opcional) y `LAI_OLLAMA_BIN` (`ollama`); los mensajes de la
Regla 9 mencionan esas variables. `fix_main.py` eliminado con `git rm`.

## 6. Scripts de benchmark

**ANTES:** `run_competition.sh` y `run_stress.sh` tenían rutas
`/home/kubuntu/...`, exigían `jq`, abortaban con `set -e` cuando el agente
fallaba y mostraban mojibake.

**DESPUÉS:** `SCRIPT_DIR` + `LAI_BIN` (con soporte `.exe` y mensaje de
compilación), `jq` opcional, `set -uo pipefail`, captura de `agent_rc`,
omisión real de modelos ausentes en `run_competition.sh`, salida ASCII y
verificación de Ollama. Los tres scripts pasan `bash -n`.
`run_benchmark.sh` no se modificó: es el que produce las cifras publicadas.

## 7. Suites de tests

**ANTES:** sólo se ejecutaban `cargo test --lib` y las suites offline de
`swebench/`.

**DESPUÉS:**

| Suite | Comando | Resultado |
|---|---|---|
| Rust completa | `cargo test` | 90 OK, 0 fallos |
| Rust lib (tras el fix del parser) | `cargo test --lib` | 61 OK |
| Python legado | `python -m pytest tests -q` | 113 passed, 9 skipped |
| Harness SWE-bench offline | `python -m unittest swebench.test_*` | 75 OK |

`tests/infrastructure.rs` no ejecuta pruebas (sus funciones no son
`#[tokio::test]`); se documenta como histórico.

## 8. Mediciones (Regla 11)

**ANTES:** una sola corrida de 3 tareas, con parser defectuoso, contexto 4096
(dato falso), y ninguna medición del efecto del contexto.

**DESPUÉS:** dos cosas separadas.

**(a) Repetición del benchmark con el parser corregido.** Mismo modelo, mismas
tareas, mismo binario; solo cambia el parser. Resultado real:

| Tarea | Resultado | Tiempo | Motivo revisado a mano |
|---|---|---|---|
| `add_two_numbers` | FAIL | 1312.1 s | reescribió `add(i32,i32)` como `add(&str,&str)`, rompe compilación y test |
| `factorial` | FAIL | 345.5 s | sobrescribió `Cargo.toml` con `...` → TOML inválido |
| `fix_off_by_one` | PARTIAL | 198.0 s | `0..arr.len()-1`, sigue mal; compila sin tests |
| **Total** | **0 PASS / 1 PARTIAL / 2 FAIL** | **1855.5 s** | |

Diferencia clave con la corrida anterior: ahora los archivos **sí cambian**
(`FAIL` por código roto, no por «no file changes»), lo que confirma que el
arreglo del parser era correcto. El 0 PASS restante se explica por dos causas
nuevas y documentadas: el agente reescribe una API pública que ya funcionaba y
escribe en `Cargo.toml`, que no debería tocar.

**(b) Efecto de `OLLAMA_CONTEXT_LENGTH`.** Segunda instancia de Ollama en
`:11435` con `OLLAMA_CONTEXT_LENGTH=16384`; la de `:11434` queda en su valor
por defecto. Mismo modelo, mismo prompt (43 tokens), 128 tokens generados,
`temperature: 0`, 4 muestras por instancia. Medido con
`scripts/measure_context_throughput.sh`, leyendo `eval_count`/`eval_duration` de
`/api/generate` y el contexto real de `/api/ps`:

| Instancia | `context_length` real | tok/s (4 muestras) | Media |
|---|---|---|---|
| `:11434` | 32768 | 7.56, 7.19, 6.44, 7.26 | 7.11 |
| `:11435` | 16384 | 7.15, 7.28, 6.82, 7.04 | 7.07 |

Conclusión: 0.6 % de diferencia, indistinguible del ruido. La recomendación de
`OLLAMA_CONTEXT_LENGTH=16384` no mejora la velocidad en este hardware. También
se corrigió el dato de `BENCHMARKS.md` que afirmaba contexto 4096: el modelo
carga 32768 por defecto.

## 9. Pendientes que siguen bloqueados

| Pendiente | Bloqueo |
|---|---|
| Piloto SWE-bench 10–20 instancias | Docker daemon caído en este host |
| Evaluación real de parches | depende del punto anterior |
| Tabla representativa en `BENCHMARKS.md` | depende de los dos anteriores |

## 10. Hallazgos nuevos que quedan abiertos

La repetición del benchmark con el parser corregido (sección 8a) destapó dos
comportamientos del agente que **no** se han corregido en esta tanda, porque
requieren decisión de diseño y no un parche rápido:

1. **El agente sobrescribe manifiestos.** `qwen2.5-coder:3b` reemplazó
   `Cargo.toml` con contenido inventado y rompió el proyecto. No existe ninguna
   guarda en el `ToolRegistry` que impida escribir en `Cargo.toml`,
   `package.json`, `pyproject.toml`, etc.
2. **El agente reemplaza APIs públicas existentes.** Convirtió
   `add(i32, i32) -> i32` en `add(&str, &str) -> Result<...>` y rompió el test
   preexistente. Los enunciados del benchmark tampoco declaran la restricción,
   así que parte de la culpa es del diseño de la tarea.

Ninguno de los dos se presenta como resuelto.
