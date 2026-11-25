# lai Benchmarks — resultados honestos

Este documento registra **cómo se mide lai y qué resultados reales obtiene**.
La regla del proyecto es simple: *si una tarea no produce código que compile y
pase tests, no cuenta como éxito*. Nada de "PASS" por compilar una plantilla.

---

## Metodología

### Verificación en tres niveles

Cada tarea se ejecuta en un directorio temporal con un crate Rust mínimo
(`src/lib.rs` con un placeholder y un test trivial). Se toma un hash de
`src/lib.rs` antes de invocar al agente y otro después.

| Resultado | Significado |
|-----------|-------------|
| `PASS`    | El agente modificó archivos **y** al menos 1 test pasa |
| `PARTIAL` | El agente modificó archivos y compila, pero no hay tests |
| `FAIL`    | Sin cambios en archivos, o no compila, o los tests fallan |
| `TIMEOUT` | Superó el límite por tarea |

Esto corrige un fallo de medición anterior: `cargo check` sobre la plantilla
intacta daba falsos positivos.

### Entorno de referencia

| Componente | Valor |
|------------|-------|
| GPU | NVIDIA RTX 3070 Laptop, 7.7 GB VRAM (driver 13.0) |
| Runtime | Ollama 0.32.14, contexto 4096 |
| Backend | `lai auto` → tool calling nativo ([OI] `/v1/chat/completions`) con fallback a bloques JSON |
| Fecha | 2026-11-25 |

> Con 7.7 GB de VRAM, los modelos de 30B se ejecutan con offload parcial a CPU,
> por eso la latencia es alta. En hardware mayor los tiempos bajan.

---

## Scripts

```bash
# Benchmark rápido (3 tareas) o completo (9 tareas)
./run_benchmark.sh qwen3-coder-opt:latest http://127.0.0.1:11434/v1 true

# Competencia entre modelos (mismas 3 tareas)
MODELS_OVERRIDE="qwen3:8b qwen3-coder-opt:latest" TASK_TIMEOUT=600 ./run_competition.sh

# Estrés: 20 tareas, en paralelo
TASKS_LIMIT=20 MAX_ITER=10 ./run_stress.sh qwen3-coder-opt:latest http://127.0.0.1:11434/v1 2
```

Variables de entorno soportadas:

| Variable | Scripts | Uso |
|----------|---------|-----|
| `TASKS_LIMIT` | stress | Ejecutar solo las primeras N tareas |
| `MAX_ITER` | stress | Iteraciones máximas del agente por tarea |
| `TASK_TIMEOUT` | competition | Segundos por tarea antes de `TIMEOUT` |
| `MODELS_OVERRIDE` | competition | Lista de modelos separada por espacios |

---

## Resultados medidos (2026-11-25)

### Competencia: 3 tareas × 2 modelos, `TASK_TIMEOUT=600`

| Modelo | Tamaño | Tool calling | Resultado |
|--------|--------|--------------|-----------|
| `qwen3:8b` | 8.2B denso, Q4_K_M | Sí (nativo) | **2/3** (67%) |
| `qwen3-coder-opt:latest` | 30.5B MoE (3.3B activos), Q4_K_M | Sí (nativo) | **3/3** (100%) |

Tareas: `add_two_numbers`, `factorial`, `fix_off_by_one`.

Detalle del fallo de `qwen3:8b`: en `fix_off_by_one` reescribió el archivo
eliminando el bug, pero también eliminó los tests existentes, por lo que la
verificación no encontró evidencia de corrección (`FAIL`). Es un fallo real de
comportamiento, no de medición.

### Benchmark rápido: 3 tareas × `qwen3:8b`

Tras aplicar el arreglo de tool calling + nudge: **3/3 PASS** con tests
ejecutados. Antes del arreglo el mismo modelo daba **0/3** (respondía en prosa
sin escribir archivos).

### Estrés: 4 tareas × `qwen3:8b`, `MAX_ITER=6`, 2 jobs en paralelo

**1/4 PASS (25%)**. Las tareas de estrés son deliberadamente más amplias
(algoritmos, concurrencia, serialización) y con menos iteraciones.

### `deepseek-r1:8b`

No declara capacidad `tools` en Ollama y depende del protocolo JSON simulado.
Medido: >20 min por tarea, 1 PASS antes de agotar el tiempo. **No es práctico**
para ciclos de agente en este hardware. Queda excluido de la comparativa.

---

## Qué se arregló gracias a estas pruebas

| Bug encontrado | Arreglo |
|----------------|---------|
| Modelos sin capacidad `tools` no escribían archivos | Tool calling nativo + fallback JSON + *nudge* si responde en prosa |
| `\n` literales (JSON doble-escapado) → archivo en una línea | `unescape_literal_escapes` en `normalize_tool_arguments` |
| Ruta con `:` (`qwen3:8b/`) rompía `cargo test` | Sanear nombre de directorio en los scripts |
| "PASS" falso al no modificarse nada | Hash antes/después + tests obligatorios |
| Tests ausentes contaban como éxito | Nivel `PARTIAL` explícito |
| Modelos lentos bloqueaban la suite | `TASK_TIMEOUT` por tarea |
| Sin paralelismo en estrés | `xargs -P` con jobs configurables |

---

## Limitaciones declaradas

- **No es SWE-bench Verified.** Son tareas propias, reproducibles, no el
  benchmark académico de 500 issues. La comparación con cifras publicadas de
  modelos frontier **no es directa** y aquí no se hace.
- **No determinismo.** Los modelos locales varían entre ejecuciones; los
  números son una muestra, no una garantía.
- **Muestra pequeña.** 3–20 tareas por corrida. Sirven para detectar
  regresiones, no para ranking fino.
- **Un solo equipo.** VRAM de 7.7 GB condiciona qué modelos son viables.

---

## Próximos pasos de medición

1. Corrida completa del estrés (20 tareas) por modelo y publicar la tabla.
2. Comparar `lai auto` contra el mismo modelo con tool calling nativo puro
   (sin fallback) para aislar el efecto del *nudge*.
3. Añadir tareas multi-archivo (refactor con tests existentes) para medir
   comprensión de repositorio, no solo generación de un archivo.
