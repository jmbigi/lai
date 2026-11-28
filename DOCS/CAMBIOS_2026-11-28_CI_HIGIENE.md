# Cambios 2026-11-28 — higiene del repositorio, CI Rust y veracidad documental

Registro según **Regla 10**. Continúa `DOCS/CAMBIOS_2026-11-28.md`.
Todo lo marcado como medido se ejecutó en este equipo (Linux, Rust estable).
Lo no probado se declara en §6 (Regla 6).

## 0. Punto de partida

Una revisión del estado del proyecto encontró tres problemas que no eran
features faltantes sino deuda estructural:

| Hallazgo | Evidencia |
|---|---|
| Artefactos de build versionados | `git ls-files lai_rust/target` = **345 archivos, 82 MB** de 85 MB totales del repo |
| Sesiones locales versionadas | 6 JSON de `sessions/` en el índice pese a estar en `.gitignore` |
| CI ciego a la implementación principal | `make check`, `scripts/ci_local.sh`, `scripts/quality_gates.sh` y `.pre-commit-config.yaml` solo ejecutaban Python |
| Documentación engañosa | `README.md` atribuía al Rust 50+ tools y Plan/ReAct/Reflection; `AGENTS.md` afirmaba sandbox bubblewrap en Rust; `DOCS/README.md` no indexaba los cambios del 27–28 |

`HEAD` de partida: `dec99f9`, árbol limpio, 158 tests Rust y 241 Python en verde.

---

## 1. Dejar de versionar `lai_rust/target/` y `sessions/`

**ANTES:** `.gitignore` ya contenía `lai_rust/target/` y `sessions/`, pero ambos
se habían añadido al índice antes de esas reglas. Un clone limpio descargaba
82 MB de `.rlib`, `.rmeta`, `.so` y fingerprints de Cargo.

**DESPUÉS:** `git rm -r --cached lai_rust/target sessions`. Los archivos siguen
en disco (no se borró nada del workspace), pero dejan de viajar en el
repositorio. 351 entradas retiradas del índice.

**Verificación:** `git ls-files lai_rust/target | wc -l` → 0; `test -d
lai_rust/target` → sí (los artefactos locales se conservan).

---

## 2. El CI local cubre Rust

**Archivo afectado:** `scripts/ci_local.sh`, `Makefile`.

**ANTES:** los pasos obligatorios eran `py_compile`, `ruff` y `pytest`. El
código Rust (23.045 líneas, la implementación principal) no se compilaba ni se
probaba en CI. Un cambio en Rust podía entrar sin que el pipeline lo notara.

**DESPUÉS**

- `scripts/ci_local.sh` añade tres pasos obligatorios cuando `cargo` existe:
  `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` y
  `cargo test`, ejecutados en `lai_rust/`.
- En modo `--ci`, la ausencia de `cargo` **falla** en lugar de omitirse: Rust es
  el código activo.
- `go build` del gateway queda como paso **opcional** (el gateway no es
  necesario para que el agente funcione).
- `make check` pasa a ser `compile + lint + test + rust-check`.
- Nuevos targets: `make rust-check` y `make gateway-check`.

**Verificación:** `bash -n scripts/ci_local.sh` → OK; `make rust-check` → fmt,
clippy y 158 tests en verde; `make gateway-check` → compila.

---

## 3. `AGENTS.md`: el sandbox no existe en Rust

**ANTES:** la sección *Security Notes* afirmaba:

> `run_command` and `run_python` run with sandbox (bubblewrap) by default

Era falso para el CLI Rust: `run_shell_tool` ejecuta `sh -c` y `run_python`
ejecuta `python3` sin aislamiento. El sandbox bubblewrap existe en el CLI Python
legado y en la rama `feat/fase-0-1-harness`, no en `master`.

**DESPUÉS:** la nota dice explícitamente que no hay sandbox en Rust, que `auto`
no aplica la allowlist y que la guarda de manifiestos no cubre `apply_patch` ni
el perfil `bash`. Recomienda usar `auto` solo en repositorios confiables hasta
que se porte el sandbox.

**Verificación:** `grep -n "sandbox" AGENTS.md` ya no contiene la afirmación
falsa; coincide con `SECURITY.md`, que ya decía «not a sandbox».

---

## 4. `README.md`: separar lo que es del CLI legado

**ANTES:** dos secciones sin etiqueta de vigencia atribuían al Rust
características que solo tiene `lai.py`:

- «Tool Registry (50+ tools)» — el Rust tiene **15**.
- «AutoMode Intelligence: Plan-and-Execute, ReAct, Reflection» — el `auto` de
  Rust es un bucle de herramientas con presupuesto de contexto, escalada por
  turnos y perfiles `full`/`bash`; no implementa esos patrones.

**DESPUÉS:** ambas secciones llevan un aviso de vigencia que las identifica como
CLI Python legado y enumeran las 15 herramientas reales del Rust.

**Verificación:** `grep -n "Tool Registry" README.md` muestra la etiqueta
«CLI Python legado (50+ tools)»; la lista de 15 herramientas coincide con
`lai_rust/src/tools.rs`.

---

## 5. `DOCS/README.md`: índice y arquitectura al día

**ANTES:** faltaban los registros `CAMBIOS_2026-11-27.md`,
`CAMBIOS_2026-11-27_MEJORAS.md`, `CAMBIOS_2026-11-27_SWEBENCH.md`,
`CAMBIOS_2026-11-28.md` y este documento. El diagrama mostraba solo `lai.py`,
recomendaba `-ngl 99` sin advertir que no entra en 8 GB de VRAM y describía
`make check` como si solo ejecutara Python.

**DESPUÉS:** índice completo, diagrama con Rust como implementación principal y
Python/Go como legado/opcional, nota sobre `-ngl` según VRAM, y sección de
quality gates que incluye `make rust-check` y `make gateway-check`.

**Verificación:** los enlaces del índice apuntan a archivos existentes
(`ls DOCS/CAMBIOS_2026-11-*`).

---

## 6. Verificación completa

| Comprobación | Resultado |
|---|---|
| `cargo fmt --check` | ✅ |
| `cargo clippy --all-targets -- -D warnings` | ✅ sin avisos |
| `cargo test` | ✅ 158 tests, 0 fallos |
| `pytest tests swebench` | ✅ 241 passed, 9 skipped |
| `ruff check .` | ✅ |
| `bash -n scripts/ci_local.sh` | ✅ |
| `make rust-check` | ✅ |
| `make gateway-check` | ✅ |
| `git ls-files lai_rust/target \| wc -l` | ✅ 0 |
| `git ls-files sessions \| wc -l` | ✅ 0 |
| Tamaño del índice tras la limpieza | 85 MB → ~3 MB |

---

## 7. Lo que NO se ha probado (Regla 6)

- **CI en un runner limpio.** Se ejecutó en este equipo, con toolchains ya
  instalados. No hay GitHub Actions ni un contenedor limpio donde verificar la
  ruta «cargo ausente».
- **Portar el sandbox.** Sigue pendiente; este cambio solo corrige la
  documentación para que no prometa aislamiento inexistente.
- **`apply_patch` y el perfil `bash` sobre manifiestos.** Siguen sin pasar por
  la guarda; declarado en `AGENTS.md` y en `CAMBIOS_2026-11-28.md`.
- **La rama `feat/fase-0-1-harness`.** No se tocó: 13 commits y 10.210 líneas
  sin integrar. Requiere una decisión de producto (rescatar el sandbox y
  archivar el resto, o reconciliar).
- **Windows/macOS.** Solo Linux.
