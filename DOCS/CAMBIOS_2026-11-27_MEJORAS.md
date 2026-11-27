# Cambios 2026-11-27 (segunda tanda) — agente eficiente, comandos reales y deadlock

Registro según **Regla 10**. Continúa `DOCS/CAMBIOS_2026-11-27.md`.
Todo lo que aparece como medido se ejecutó en esta máquina (Linux, Rust 1.9x,
sin GPU). Lo que no se pudo ejecutar se declara al final.

## 0. Investigación: qué hacen los agentes modernos que gastan poco

Antes de tocar código se revisó el estado del arte en agentes de código
ligeros. Cifras y afirmaciones **externas**, no reproducidas por lai
(Regla 11):

| Fuente | Idea que se adoptó |
|---|---|
| **mini-SWE-agent** (Princeton/SWE-agent, 2025): agente de ~100 líneas, solo `bash`, >74 % en SWE-bench Verified según su leaderboard | Un agente bash-only es competitivo y arranca más rápido; el andamiaje gigante no es lo que resuelve el problema. |
| **SWE-agent / ACI** (Yang et al., 2024) | La interfaz influye en el resultado: salidas acotadas, comandos claros y errores accionables. |
| **Harness design for coding agents** (arXiv 2609.20804, 2026): elisión de resultados antiguos, limpieza al 60 % del presupuesto, compactación al 80 % | Elidir salidas de herramientas antiguas conserva la tarea y evita expulsar el objetivo. |
| Práctica común en agentes de 2025-2026 ([CC], Aider, Cline): formato de edición *search/replace* con fragmento exacto | Para modelos pequeños es más fiable que reescribir el archivo o generar un diff con números de línea. |

Traducción a lai (todo local, Regla 0):

1. **Perfil `bash`** (`lai auto --profile bash`): una sola herramienta y un
   prompt corto, en lugar de 14 esquemas.
2. **`edit`** (search/replace) como vía preferida para modificar archivos.
3. **Presupuesto de contexto** con elisión de salidas antiguas.
4. Seguir arreglando lo que el agente ya tenía mal (deadlock de telemetría,
   comandos que eran stubs, `py_compile .`).

## 1. Deadlock de telemetría: `lai verify` no terminaba nunca

**ANTES:** `record_*` y `task_span` hacían

```rust
if let Some(mut t) = TELEMETRY.lock().unwrap().take() {
    ...
    *TELEMETRY.lock().unwrap() = Some(t);   // ← el guard del if let sigue vivo
}
```

El guard temporal del `if let` vive hasta el final del bloque, así que el
segundo `lock()` bloqueaba el mismo mutex. **Cualquier** operación que
registrara telemetría colgaba el proceso. Medido: `lai verify .` no terminaba
(se mató a los 20 s con `timeout`, código 124).

**DESPUÉS:** helper `with_telemetry(|t| …)` que toma el guard una sola vez y
muta en el sitio. Test de regresión `recording_events_does_not_deadlock`.
Medido después: `lai verify . --level 1` termina en 156 ms.

## 2. `lai verify` reprobaba proyectos Python válidos

**ANTES**

- La comprobación de sintaxis ejecutaba `python3 -m py_compile .`, que **no
  acepta directorios**: todo proyecto Python fallaba con
  `[Errno 21] Is a directory: '.'` (verificado).
- Si `ruff`, `mypy` o `eslint` no estaban instalados, el nivel se marcaba como
  fallido («ruff failed: No such file or directory»).

**DESPUÉS**

- `python3 -m compileall -q .` (recorre el árbol).
- `tool_missing()` distingue «herramienta ausente» de «hallazgos»: si no está
  instalada, el nivel pasa como omitido y lo dice en el detalle.

Medido: proyecto temporal con `pyproject.toml` y un `def` válido →
`PASS [syntax] 100/100`, veredicto `Success`; con todos los niveles →
`SuccessWithDeclaredGaps`.

## 3. Comandos que eran stubs

Siete subcomandos imprimían «to be implemented» aunque sus motores ya existían
en el crate. **ANTES → DESPUÉS**:

| Comando | Antes | Después (verificado a mano) |
|---|---|---|
| `repo-map` | texto fijo | mapa real; `--file` lista símbolos con línea |
| `verify` | texto fijo | `VerificationEngine`, `--level 1-8`, código de salida 1 si falla |
| `diagnose` | texto fijo | `DiagnosticEngine`; acepta mensaje o archivo de log, extrae rutas |
| `metrics` | texto fijo | archivos/símbolos/líneas del workspace + telemetría de sesión |
| `baseline` | texto fijo | `create/list/show/compare` sobre SQLite; acepta prefijo de ID |
| `memory` | texto fijo | `list/search/store` sobre SQLite (`.lai/memory.db`) |
| `checkpoint` | texto fijo | `list/show/create/rollback` sobre `ChangeEngine` |
| `repair` | texto fijo | diagnostica el `--error` y explica cómo aplicar el arreglo |
| `plan` | texto fijo | explica que necesita modelo local y remite a `auto` |

Ejemplos reales ejecutados:

```
$ lai repo-map --path lai_rust/src --file repomap.rs
repomap.rs
      6  fn is_skipped_dir
     36  fn extract_symbols
     88  struct FileInfo
...
$ lai diagnose "Traceback ... File \"src/app.py\", line 3 NameError ..."
1. [TypeError] ... Ubicación: src/app.py:3
$ lai memory store "El proyecto usa Rust 2021" --type semantic --source manual
Memoria guardada: bd5d1162-...
$ lai baseline create demo .
Baseline creado: 9235fbf6-...
$ lai baseline compare 9235fbf6 .
Compared 3 metrics: 0 regressions, 1 improvements, 2 unchanged
```

**Bug encontrado al probar `baseline create`:** «key must be a string».
`BaselineMetric` derivaba `Serialize`, así que `Custom(String)` se convertía en
`{"Custom": "x"}`, que no es una clave JSON válida. Ahora `BaselineMetric` se
serializa como cadena (`custom_files`, `custom_lines`…) y se deserializa con
`from_string`, que es lo que ya esperaba el lector de la base de datos.

## 4. Perfil `bash` (una herramienta, prompt corto)

**ANTES:** `auto` anunciaba siempre 14 herramientas (~6 KB de prompt solo en
esquemas) y un prompt de sistema largo. En un modelo de 8k-32k tokens eso
compite con el código.

**DESPUÉS:** `lai auto --profile bash` expone **una** herramienta (`bash`) y un
prompt de ~800 caracteres con instrucciones de un comando por turno, edición
con heredoc y verificación con el compilador/tests. `--profile full` sigue
siendo el valor por defecto.

Medido contra un servidor [OI] falso: `tools: 1`, `system_chars: 804` en los
cuatro turnos, frente a 14 herramientas en el perfil completo.

## 5. Presupuesto de contexto (`context_budget.rs`)

**ANTES:** el historial crecía sin límite. Con un `read_file` de 6 KB por
turno, en 4-5 turnos se llenaba una ventana de 8k y el modelo empezaba a
responder en prosa (es lo que se observó en el piloto: `Listo.` sin tocar
código).

**DESPUÉS:** antes de cada llamada se condensa el historial:

1. Se conservan el mensaje de sistema y la tarea original.
2. Se conservan los últimos 4 mensajes.
3. Se **eliden** los resultados de herramientas antiguos (marcador de una
   línea que invita a releer con `read_file`/`grep`).
4. Si aún no cabe, se eliden también los recientes salvo el último.
5. Después se recortan turnos de asistente largos y, como último recurso, la
   cola del contexto de sistema.

Nunca se borra un mensaje `tool`: debe seguir emparejado con su
`tool_call_id` o el protocolo de function calling se rompe.

Ventana configurable con `LAI_CONTEXT_WINDOW` (por defecto 32768, el valor con
el que se midió el piloto). No se consulta al servidor: un valor supuesto no se
presenta como medido.

Medido: con `LAI_CONTEXT_WINDOW=1024` y dos salidas de 4 KB, el log muestra
`Contexto condensado: 9062 → 5211 caracteres (1 salidas elididas, ventana 1024
tokens)` y el modelo sigue recibiendo el resultado más reciente.

## 6. Herramienta `edit` (search/replace)

**ANTES:** para cambiar una línea había dos opciones malas para un modelo
pequeño: `write_file` (reproducir el archivo entero) o `apply_patch` (generar
un diff con números de línea correctos).

**DESPUÉS:** `edit` con `path`, `old_string`, `new_string` y `replace_all`
opcional:

- Si el fragmento no aparece → error que pide copiarlo exactamente de
  `read_file`.
- Si aparece más de una vez → error que pide más contexto o `replace_all`.
- Si `old_string == new_string` → error.
- Pasa por el `PolicyEngine` igual que `write_file` (frontera del workspace,
  `.git`, credenciales).

5 tests nuevos. Se añadió al prompt del perfil completo como vía preferida.

## 7. Otros arreglos de esta tanda

- **`repomap`**: la extracción de símbolos se movió a `repomap::extract_symbols`
  y la usan tanto `lai repo-map` como la herramienta `repo_map`; antes eran dos
  implementaciones y una de ellas mutilaba `async fn`.
- **`repomap`** ahora respeta `is_skipped_dir()` (antes recorría `target/` y
  `node_modules/`).
- **`auto --output <dir>`** ya no es decorativo: escribe
  `lai-auto-<fecha>.md` con la tarea, cada llamada y su resultado.
- **`baseline show/compare`** aceptan el ID corto que imprime `list`.
- **Lint**: `ruff check .` sigue en verde tras los cambios.

## 8. Verificación

| Comprobación | Resultado |
|---|---|
| `cargo fmt --check` | ✅ |
| `cargo clippy --all-targets` | ✅ sin avisos |
| `cargo test` | ✅ **148 tests** (106 lib + 42 integración), 0 fallos |
| `pytest tests swebench` | ✅ 238 passed, 9 skipped |
| `ruff check .` | ✅ All checks passed |
| `lai verify` (proyecto Python temporal) | ✅ `Success` / `SuccessWithDeclaredGaps` |
| `lai repo-map / diagnose / metrics / memory / baseline / checkpoint` | ✅ ejecutados a mano |
| `lai auto --profile bash` (servidor falso) | ✅ 1 herramienta, prompt 804 chars, condensación |
| `lai auto --output` | ✅ transcripción escrita |

Tests Rust nuevos en esta tanda: **21** (16 unitarios + 5 de herramientas y
comandos).

## 9. Lo que NO se ha probado (Regla 6)

- **Modelo real.** No hay llama.cpp ni Ollama levantados aquí; el bucle se
  validó con un servidor [OI] falso. Las cifras de mini-SWE-agent y del paper
  de harness son **externas** y no se presentan como resultados de lai.
- **Impacto en la tasa de resolución.** El perfil `bash`, `edit` y la elisión
  están implementados y probados funcionalmente, pero **no hay medición** de
  cuántos issues resuelven: eso requiere el piloto SWE-bench con un modelo real.
- **Windows.** Solo se compiló y ejecutó en Linux.
- **`plan` y la reparación automática completa** siguen sin conectar al
  modelo; se documenta en el propio comando en vez de simular trabajo.
