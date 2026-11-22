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

9. **Lenguaje de programación óptimo y objetivo de supervivencia e inteligencia.**
   LAI debe estar implementado en el mejor lenguaje de programación para sus propósitos:
   Python para la CLI, orquestación e integración (ecosistema IA/ML, glue code); Rust
   para partes críticas de rendimiento, seguridad de memoria y concurrencia
   (`lai_rust/`). Ninguna reescritura debe romper la Regla 0 (sin cloud, sin API keys,
   sin cuentas externas).

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

---

## 2. Definiciones para evitar ambigüedades

| Término | Significado |
|---|---|
| **Salida de LAI** | Código, archivos o proyectos producidos por las funciones de generación de LAI para un usuario o proyecto destino. |
| **Código propio de LAI** | Implementación de la herramienta: `lai.py`, `harness.py`, `gateway.py`, `gateway-go/`, `scripts/`, tests de la herramienta y documentación. |
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
| 4. Salida de LAI con LLM local | ✅ | `lai.py` usa por defecto `http://localhost:8080/v1`; las rutas de generación verifican que el endpoint sea local. |
| 5. Código propio de LAI con IA cloud | ✅ | Permitido explícitamente por esta revisión (2026-11-21). |
| 6. Honestidad | ⚠️ En mejora | Documentos históricos contenían cifras desactualizadas (p. ej. `chess.py` 12,959 bytes vs. 8,706 reales). Se añadieron avisos de vigencia. |
| 7. Exactitud | ⚠️ En mejora | `DOCS/quality_comparison.md` contenía métricas de ejemplo presentadas como resultados; se convirtió en plantilla sin resultados inventados. |
| 8. Chess con LLM local | ⚠️ Parcial | `chess/chess.py` fue generado con Qwen3-8B local, pero recibió correcciones manuales; ver `chess/README.md`. |
| 9. Lenguaje óptimo + supervivencia | 🚧 En progreso | `lai_rust/` prototipo en Rust iniciado; `lai.py` detecta servicios apagados y pide activación. |

---

## 5. Control de cambios de reglas

Cualquier cambio en estas reglas debe:

1. Actualizar este archivo (`DOCUMENTACION_REGLAS.md`).
2. Actualizar el resumen de `README.md` y `AGENTS.md`.
3. Añadir una entrada en `CHANGELOG.md`.
4. Mantener la numeración 1–8 y evitar duplicados.
5. Verificarse con `make check` (incluye `scripts/check_docs.py`).

---

## 6. Documentos relacionados

- `README.md` — resumen y guía de uso.
- `AGENTS.md` — reglas operativas para agentes.
- `CHECKLIST.md` — verificación previa a entrega.
- `ESTADO_ACTUAL.md` — estado verificado del proyecto.
- `DOCS/README.md` — índice y vigencia de la documentación.
- `CHANGELOG.md` — historial de cambios.
