# Reglas del Proyecto — Fuente Única de Verdad

**Vigente desde:** 2026-11-21
**Alcance:** todo el repositorio `modelo-local`.
**Estado:** reglas consolidadas; reemplazan las versiones duplicadas del `README.md` y de documentos históricos.

---

## 0. Misión y Principio Fundacional

**LAI existe para impulsar el uso de IAs locales, abiertas y soberanas.**

Todo decisión técnica, de arquitectura o de producto debe evaluarse bajo este principio:

- **Open source first:** Preferir modelos, herramientas y licencias abiertas.
- **Sin dependencia de claves (API keys):** El usuario no debe necesitar cuentas en la nube, suscripciones ni tokens externos para usar LAI.
- **Sin dependencia de cuentas en la nube:** La inferencia, el almacenamiento y el procesamiento ocurren localmente por defecto.
- **Software gratuito y de calidad:** Promover modelos abiertos (Qwen, Llama, Gemma, etc.) servidos localmente con llama.cpp/Ollama, que igualan o superan a alternativas cerradas en tareas de código.
- **Privacidad por diseño:** El código y los datos del usuario nunca salen de su máquina salvo decisión explícita.

> **Regla 0:** *Cualquier feature, arquitectura o dependencia que introduzca dependencia obligatoria de cloud, API keys, cuentas externas o software propietario será rechazada salvo aprobación explícita y documentada del operador.*

---

## 1. Reglas

1. **Seguir las reglas de [thinker](https://github.com/jmbigi/thinker/).**
2. **Seguir las reglas de [better-ai](https://github.com/jmbigi/better-ai/).**
3. **Seguir las reglas de [better-project](https://github.com/jmbigi/better-project/).**
4. **El código que genera LAI debe ser generado por LLMs locales.**
   Todo archivo, proyecto o reparación producida por las funciones de generación de LAI (`lai code`, `lai project`, `lai auto`, `lai fix` cuando genere código, y cualquier otra ruta que produzca código de usuario) debe provenir de un modelo local. Ningún modelo cloud puede generar, reparar ni sustituir esa salida.
5. **El código propio de LAI puede ser generado con IA cloud.**
   `lai.py`, `harness.py`, `gateway.py`, `gateway-go/`, `scripts/`, los tests de la herramienta y la documentación pueden generarse o repararse con modelos locales o cloud, siempre que se revisen, prueben y documenten. La IA cloud no puede usarse para simular, falsificar o reemplazar la generación local de la salida de LAI.
6. **Honestidad 100%.**
   No afirmar éxito sin evidencia. Documentar fallos, limitaciones, correcciones manuales y resultados no verificados.
7. **Exactitud 99%.**
   Verificar rutas, versiones, puertos, comandos y cifras antes de publicarlos. Si un dato no está verificado, marcarlo como tal.
8. **El proyecto chess debe generarse con LLMs y harness locales.**
   Las correcciones manuales posteriores deben quedar documentadas y no pueden presentarse como generación local.

9. **Lenguaje de programación objetivo: Rust.**
    LAI está implementado íntegramente en Rust (`lai_rust/`), el mejor lenguaje para
    sus propósitos: binario estático autónomo (sin depender de Python ni del entorno
    del usuario), arranque rápido, bajo uso de memoria, seguridad de memoria sin
    recolector de basura, concurrencia sin miedo a races de datos, y bindings maduros
    a llama.cpp (`llama_cpp-rs`) para inferencia in-process cuando se desee.
    Ninguna reescritura debe romper la Regla 0 (sin cloud, sin API keys, sin cuentas
    externas). La implementación previa en Python (`lai.py`) queda como legado y no se
    modifica salvo correcciones críticas.

    **Objetivo de supervivencia del agente:** el agente autónomo de LAI debe priorizar
    su propio funcionamiento continuo:
    - **No auto-destrucción:** no debe ejecutar acciones que corrompan su propio código,
      borren sus herramientas o impidan su futura ejecución.
    - **Disponibilidad de modelos locales:** antes de operar, verifica que al menos un
      endpoint de modelo local (llama.cpp u Ollama) esté disponible. Si no hay modelos
      locales, se detiene y solicita intervención humana en lugar de fallar
      silenciosamente.
    - **Auto-reparación ante fallos:** si encuentra un error en su propia ejecución,
      debe intentar diagnosticarlo y repararse antes de rendirse.

    **Objetivo de inteligencia del agente:** debe usar bucle de reflexión (Reflection) y
    críticos separados para evaluar y mejorar sus propios resultados, nunca asumiendo
    que su primera salida es correcta.

    Esta regla es subordinada a la Regla 0: ninguna acción de supervivencia o
    optimización puede introducir dependencia de cloud, API keys o cuentas externas.

10. **Documentación obligatoria de cambios (Regla de trabajo en equipo).**
    Antes de realizar cualquier cambio en el código, configuración, documentación o arquitectura:
    - **Crear un documento de cambios propuestos** que describa qué se va a modificar, por qué, qué archivos afecta, y cuál es el resultado esperado.
    - **Documentar el estado ANTES del cambio** (versión actual, comportamiento observado, tests relevantes).
    - **Documentar el estado DESPUÉS del cambio** (nueva versión, comportamiento verificado, tests pasados, evidencia de corrección).
    - Cada cambio individual debe tener su registro antes/después, no solo el resultado final.
    - La documentación debe ser lo suficientemente clara para que otro miembro del equipo pueda entender, revisar y continuar el trabajo sin ambigüedades.
    - Esto aplica a: código fuente, archivos de configuración, documentación, tests, scripts, y cualquier artefacto del repositorio.
    - **No hacer cambios "silenciosos" ni batch de cambios sin documentación intermedia.**

11. **Inferencia local en equipos de baja gama: medir antes de recomendar.**
    Para equipos sin GPU dedicada (8–16 GB de RAM), lai no debe presentar
    recomendaciones de modelo o cuantización como si fueran resultados propios.
    - **Medir una variable a la vez** (modelo, cuantización, contexto, KV, hilos)
      con controles intercalados y registrar tokens/s, RAM libre, tiempo por tarea
      y tasa de éxito.
    - **Calcular primero el techo de ancho de banda**: pesos + caché KV + reserva
      del SO + pico de herramientas debe caber en la RAM libre. Si no cabe, ninguna
      optimización de software lo resuelve.
    - **Compuerta de contexto completo**: ningún cambio de cuantización, KV o
      especulación se adopta si degrada la recuperación de símbolos lejanos.
    - **Verificación obligatoria**: el bucle determinista (compilador, linter,
      tests) define la capacidad efectiva; no se sustituye por confianza en el
      modelo.
    - **Cifras externas se citan como externas.** Los números de papers o
      benchmarks de terceros (p. ej. tasas de resolución por cuantización) deben
      marcarse como no reproducidos por lai hasta que exista una corrida propia.
    - La guía de referencia es `DOCS/INFERENCIA_LOCAL_BAJA_GAMA.md`, subordinada a
      la Regla 0 (sin cloud, sin API keys, sin cuentas externas) y a la Regla 6
      (honestidad).

---

## 2. Definiciones para evitar ambigüedades

| Término | Significado |
|---|---|
| **Salida de LAI** | Código, archivos o proyectos producidos por las funciones de generación de LAI para un usuario o proyecto destino. |
| **Código propio de LAI** | Implementación de la herramienta: `lai.py` (legado), `lai_rust/` (implementación principal en Rust), `harness.py`, `gateway.py`, `gateway-go/`, `scripts/`, tests de la herramienta y documentación. |
| **Modelo local** | Modelo servido en `localhost`/`127.0.0.1`/`::1` (llama.cpp u Ollama). Una IP privada de la LAN sólo se acepta si el operador lo declara explícitamente. |
| **Modelo cloud** | Cualquier modelo accesible por API remota. |
| **Documentación** | Markdown, configuración, changelogs, guías y comentarios de ayuda. No incluye el código de usuario generado por LAI. |

---

## 3. Jerarquía en caso de conflicto

Cuando dos reglas o una orden del usuario entren en conflicto, aplicar este orden:

1. Seguridad.
2. Legalidad.
3. Privacidad.
4. Control humano.
5. Exactitud.
6. Eficiencia.

Si el conflicto persiste, detenerse y preguntar al operador humano.

---

## 4. Estado de cumplimiento (honesto)

| Regla | Estado | Evidencia / observación |
|---|---|---|
| 1–3. Reglas externas | ✅ | `AGENTS.md` resume el ruleset y enlaza las fuentes. |
| 4. Salida de LAI con LLM local | ✅ | `llm.rs` implementa `is_local_url()` (loopback, rangos privados, `.local`, alias `host.docker.internal`/`gateway.docker.internal` de Docker) y `LlmClient::check_endpoint()` aborta antes de cualquier request si el endpoint no es local, salvo `--allow-remote`. Cubierto por 6 tests unitarios (2026-11-26). Los alias de Docker se aceptan porque resuelven a la propia máquina: son la vía para llegar a un Ollama local desde un contenedor. |
| 5. Código propio de LAI con IA cloud | ✅ | Permitido explícitamente por esta revisión (2026-11-21). |
| 6. Honestidad | ⚠️ En mejora | Documentos históricos contenían cifras desactualizadas (p. ej. `chess.py` 12,959 bytes vs. 8,706 reales). Se añadieron avisos de vigencia. |
| 7. Exactitud | ⚠️ En mejora | `DOCS/quality_comparison.md` contenía métricas de ejemplo presentadas como resultados; se convirtió en plantilla sin resultados inventados. |
| 8. Chess con LLM local | ⚠️ Parcial | `chess/chess.py` fue generado con Qwen3-8B local, pero recibió correcciones manuales; ver `chess/README.md`. |
| 9. Lenguaje óptimo + supervivencia | ✅ | `lai_rust/` implementación completa en Rust. CLI real (verificado 2026-11-26, 19 subcomandos clap): `chat`, `code`, `status`, `auto`, `repo-map`, `baseline`, `verify`, `diagnose`, `repair`, `checkpoint`, `memory`, `plan`, `metrics`, `bench`, `init`, `doctor`, `tui`, `mcp`, `bench-swe`. **No existen** `serve`, `review`, `fix`, `search`, `project` en el CLI Rust (sí en el Python legado, que está en mantenimiento y no a la par). Build release compila limpio, `cargo clippy --all-targets` sin avisos. |
| 10. Documentación obligatoria de cambios | 🆕 Nueva | Regla agregada 2026-11-23. Requiere documento de cambios propuestos, estado antes/después por cada cambio, y trazabilidad para trabajo en equipo. |
| 11. Baja gama: medir antes de recomendar | 🆕 Nueva | Regla agregada 2026-11-26. Exige medir una variable a la vez, calcular el techo de ancho de banda, aplicar la compuerta de contexto completo y marcar las cifras externas como no reproducidas. Guía: `DOCS/INFERENCIA_LOCAL_BAJA_GAMA.md`. |

---

## 5. Control de cambios de reglas

Cualquier cambio en estas reglas debe:

1. Actualizar este archivo (`DOCUMENTACION_REGLAS.md`).
2. Actualizar el resumen de `README.md` y `AGENTS.md`.
3. Añadir una entrada en `CHANGELOG.md`.
4. Mantener la numeración 1–11 y evitar duplicados.
5. Verificarse con `make check` (incluye `scripts/check_docs.py`).

---

## 6. Documentos relacionados

- `README.md` — resumen y guía de uso.
- `AGENTS.md` — reglas operativas para agentes.
- `CHECKLIST.md` — verificación previa a entrega.
- `ESTADO_ACTUAL.md` — estado verificado del proyecto.
- `DOCS/README.md` — índice y vigencia de la documentación.
- `DOCS/INFERENCIA_LOCAL_BAJA_GAMA.md` — guía de inferencia local en CPU para equipos de baja gama (Regla 11).
- `CHANGELOG.md` — historial de cambios.
