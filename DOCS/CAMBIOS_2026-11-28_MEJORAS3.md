# Cambios 2026-11-28 — lote E/F: endurecimiento del sandbox, `edit` tolerante y submit

Registro según **Regla 10**. Continúa `DOCS/CAMBIOS_2026-11-28_MEJORAS2.md`.
Fuentes consultadas para orientar estas mejoras (no son mediciones de lai):

- mini-SWE-agent (SWE-bench): agente bash-only, historial lineal, una acción por
  turno, sentinel `COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT`.
- Anthropic (agente SWE-bench): rutas absolutas y edición por reemplazo exacto
  con errores que el modelo puede corregir.
- Warp (SWE-bench Verified 71 %): fallback de edición tolerante a indentación y
  lista de tareas.
- Pydantic y Plumbrule (sandboxing de agentes): `--clearenv`, ocultar `/run`,
  no heredar secretos, probar el límite con ataques.

Ninguna cifra externa se presenta como resultado de lai (Regla 6/11).

## 1. Sandbox: entorno limpio y `/run` oculto

**ANTES:** el proceso sandboxeado heredaba **todo** el entorno del host. Un
`GITHUB_TOKEN`, `AWS_SECRET_ACCESS_KEY` o `SSH_AUTH_SOCK` presente en la sesión
llegaba al comando que ejecuta el modelo. `/run` se montaba tal cual, con los
sockets de Docker y systemd accesibles por conexión (un montaje de solo lectura
no impide conectarse a un socket).

**DESPUÉS**

- `--clearenv` y solo se pasan `PATH`, `HOME`, `TERM`, `LANG`/`LC_ALL`/`LC_CTYPE`,
  `CARGO_HOME` y `RUSTUP_HOME`.
- `--tmpfs /run` oculta los sockets de demonios del host.
- Se mantienen los binds de solo lectura de `/usr`, `/lib`, `/bin`, `/etc` y los
  binds de `~/.cargo`, `~/.rustup`, `~/.local`; el resto de `$HOME` **no** está
  montado.

**Verificación**

| Caso | Resultado |
|---|---|
| `echo "$GITHUB_TOKEN"` con sandbox | ✅ vacío |
| `echo "$SSH_AUTH_SOCK"` con sandbox | ✅ vacío |
| `cargo --version` con sandbox | ✅ funciona |
| `python3 -c "print(1+1)"` con sandbox | ✅ funciona |
| Fichero decoy en `$HOME` | ✅ `HIDDEN` (no montado) |
| DNS con `LAI_SANDBOX_ALLOW_NET=1` | ✅ resuelve |

Tests nuevos: `sandbox_env_does_not_leak_secrets`,
`sandbox_env_sets_cargo_home`, `sandbox_hides_home_directory_files`,
`sandbox_scrubs_secret_environment_variables`.

## 2. `edit`: fallback tolerante a indentación

**ANTES:** si `old_string` no aparecía byte a byte (indentación distinta), `edit`
fallaba. Es el fallo más común de modelos pequeños al copiar código.

**DESPUÉS:** si la coincidencia exacta falla, `indentation_agnostic_span()` busca
una ventana de líneas que coincida ignorando espacios iniciales/finales. Solo se
acepta si es **única**; si hay cero o varias, se mantiene el error. Cuando se usa
el fallback, `reindent_like()` reaplica la indentación real del archivo a las
líneas nuevas (para no romper Python).

**Verificación:** 4 tests — coincidencia con indentación distinta, ambigüedad
rechazada, conservación del salto de línea final, sin coincidencia.

## 3. Sentinel de entrega en el perfil `bash`

**ANTES:** en el perfil bash el modelo no podía terminar sin dejar de llamar a
la herramienta; el bucle solo acababa por presupuesto o por repetición.

**DESPUÉS:** si la salida de `bash`/`run_command` contiene
`COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT`, el bucle se detiene y lo informa. El
prompt del perfil bash explica que debe ejecutarse como único comando del turno.

**Verificación E2E:** con servidor [OI] falso y
`echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT`, la corrida termina en la iteración
1 con `✅ Tarea entregada por el agente (submit).`

## 4. Parser XML de tool calls

**ANTES:** solo se reconocían tool calls nativas y bloques ```json. Si un modelo
cambiaba a XML a mitad de la corrida (`<tool_call><function=…>`), la llamada
llegaba como texto y el bucle la interpretaba como "tarea terminada" (fallo
documentado por Bees con modelos locales).

**DESPUÉS:** `parse_xml_tool_calls()` recupera:
- `<tool_call><function=NAME><parameter=KEY>VALUE</parameter></function></tool_call>`;
- la variante sin envoltorio `<function=NAME>…`;
- múltiples parámetros y entidades XML (`&amp;`, `&lt;`, …).

Se usa solo si no hubo tool calls nativas ni JSON. 4 tests.

## 5. Coherencia de herramientas y pruebas de ataque

**ANTES:** nada impedía registrar una herramienta sin esquema (o anunciar una
que no existe); el sandbox no tenía pruebas de "escribir fuera del workspace".

**DESPUÉS**
- `every_registered_tool_has_a_schema`: el registro y los esquemas coinciden en
  ambos sentidos.
- `bash_profile_exposes_exactly_one_tool`: el perfil bash expone solo `bash`.
- `sandbox_blocks_writes_outside_the_workspace`: `/etc` es de solo lectura.
- `sandbox_masks_run_directory`: `/run` está vacío (sin sockets de Docker).

## 6. Verificación completa

| Comprobación | Resultado |
|---|---|
| `cargo test` | ✅ **210 tests** (168 lib + 42 integración), 0 fallos |
| `cargo fmt --check` | ✅ |
| `cargo clippy --all-targets -- -D warnings` | ✅ |
| `pytest tests swebench` | ✅ 241 passed, 9 skipped |
| `ruff check .` | ✅ |
| Entorno limpio en sandbox | ✅ tokens vacíos |
| `$HOME` oculto en sandbox | ✅ |
| Submit sentinel | ✅ |
| `edit` con indentación distinta | ✅ |

## 7. Lo que NO se ha probado (Regla 6)

- **Modelos reales.** Todo con comandos directos y servidor falso. No se midió
  cuánto mejora la tasa de parches con el fallback de `edit` ni con el sentinel.
- **Sandbox en macOS/Windows.** Solo Linux.
- **Seccomp/Landlock.** No se añadió un filtro de syscalls; el aislamiento es de
  namespaces + `--cap-drop ALL` + entorno limpio. Un escape del kernel queda
  fuera de alcance (declarado en `SECURITY.md`).
- **`/tmp` compartido.** Se sigue montando `/tmp` del host porque `run_python`
  escribe allí el script temporal; un `--tmpfs /tmp` exigiría reubicar el
  script. Declarado como límite.
