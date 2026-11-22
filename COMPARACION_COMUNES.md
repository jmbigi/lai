# Comparación honesta: lai vs herramientas y servicios comunes de IA para código

**Fecha:** 2026-11-22  
**Objetivo:** Comparación veraz entre `lai` y las herramientas/servicios más comunes del mercado de asistentes de código con IA.

> Esta comparación se basa en documentación pública de cada herramienta (accedida vía web search el 2026-11-22) y en el código fuente de `lai` (commit `e413fe4`).

---

## 1. Resumen ejecutivo

| Herramienta | Tipo | Modelo de negocio | Local-first | Precio aproximado | Lai vs esta herramienta |
|-------------|------|-------------------|-------------|-------------------|-------------------------|
| **lai** | CLI Python | Open source gratuito | ✅ Regla 0 estricta | $0 | Referencia |
| **GitHub Copilot** | IDE extension | Suscripción cloud | ❌ Cloud only | $10-19/mes | Lai es gratis y 100% local; Copilot es cloud y de pago |
| **Cursor** | IDE (fork VS Code) | Suscripción cloud | ⚠️ Híbrido | $8-30/mes | Lai es CLI y local; Cursor es IDE gráfico con cloud |
| **Continue** | IDE extension | Open source gratuito | ✅ Configurable | Gratis | Lai es más estricto en local-by-default; Continue es más flexible |
| **Cline** | VS Code extension | Open source gratuito | ✅ Configurable | Gratis | Lai tiene AutoMode con auto-evaluación; Cline requiere aprobación paso a paso |
| **Aider** | CLI terminal | Open source gratuito | ✅ Configurable | Gratis | Lai tiene más herramientas integradas; Aider es git-native |
| **Tabby** | Self-hosted server | Open source / Enterprise | ✅ Self-hosted | Gratis / Enterprise | Lai es CLI simple; Tabby es servidor para equipos |
| **Sourcegraph Cody** | IDE extension | Freemium / Enterprise | ✅ Self-hosted posible | Gratis / $59/user/mes | Lai es CLI local; Cody es enterprise con codebase search |
| **Tabnine** | IDE extension | Freemium / Enterprise | ✅ Self-hosted posible | Gratis / $39/user/mes | Lai es 100% gratis y local; Tabnine tiene versión cloud |

---

## 2. Matriz detallada de capacidades

| Capacidad | lai | Copilot | Cursor | Continue | Cline | Aider | Tabby | Cody | Tabnine |
|-----------|-----|---------|--------|----------|-------|-------|-------|------|---------|
| **Interfaz** | CLI | IDE ext | IDE | IDE ext | IDE ext | CLI | Server + ext | IDE ext | IDE ext |
| **Local LLM por defecto** | ✅ | ❌ | ⚠️ Opcional | ✅ Opcional | ✅ Opcional | ✅ Opcional | ✅ Self-hosted | ✅ Opcional | ⚠️ Opcional |
| **Cloud obligatorio** | ❌ No | ✅ Sí | ⚠️ Híbrido | ❌ No | ❌ No | ❌ No | ❌ No | ⚠️ Opcional | ⚠️ Opcional |
| **API keys obligatorias** | ❌ No | ✅ Sí | ✅ Sí | ❌ No | ❌ No | ❌ No | ❌ No | ⚠️ Opcional | ⚠️ Opcional |
| **Cuenta requerida** | ❌ No | ✅ Sí | ✅ Sí | ❌ No | ❌ No | ❌ No | ❌ No | ⚠️ Opcional | ⚠️ Opcional |
| **RAG semántico código** | ✅ Básico | ✅ | ✅ | ✅ | ✅ | ✅ (repo map) | ✅ | ✅ (avanzado) | ⚠️ Básico |
| **Memoria persistente** | ✅ JSON | ❌ | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ | ⚠️ Limitada |
| **AutoMode autónomo** | ✅ Con auto-eval | ❌ | ⚠️ Composer | ❌ | ✅ Tool loop | ❌ | ❌ | ❌ | ❌ |
| **Aprobación humana tools** | ❌ No | ❌ | ⚠️ Opcional | ❌ | ✅ Paso a paso | ❌ | ❌ | ❌ | ❌ |
| **Multi-agente** | ❌ No | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Calidad de código integrada** | ✅ ruff/mypy/bandit | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **CI local 100%** | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **OCR integrado** | ✅ tesseract | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **System One decisiones** | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **MCP servidor** | ✅ 23 tools | ❌ | ❌ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| **MCP cliente** | ❌ No | ❌ | ❌ | ✅ | ✅ | ❌ | ❌ | ✅ | ❌ |
| **Precio** | $0 | $10-19/mes | $8-30/mes | Gratis | Gratis | Gratis | Gratis/Ent | Gratis/$59 | Gratis/$39 |

**Leyenda:** ✅ Sí | ⚠️ Parcial/opcional | ❌ No

---

## 3. Análisis detallado por herramienta común

### 3.1 GitHub Copilot
- **Qué es:** Extensión de IDE (VS Code, JetBrains, Vim) basada en modelos de OpenAI/Microsoft.
- **Modelo:** Cloud-only. Código y prompts se envían a servidores de Microsoft/GitHub.
- **Precio:** $10/mes (individual), $19/mes (business).
- **Soberanía:** ❌ Los datos salen de la máquina del usuario.
- **Lai vs Copilot:** Lai es 100% gratis y local; Copilot cuesta dinero y envía código a la cloud. Para usuarios que priorizan privacidad y costo cero, lai gana. Para usuarios que quieren la mejor integración con GitHub y modelos frontier, Copilot puede ser mejor.

### 3.2 Cursor
- **Qué es:** Fork de VS Code con IA integrada nativa (autocomplete, chat, Composer).
- **Modelo:** Híbrido — puede usar modelos cloud o locales vía endpoint OpenAI-compatible.
- **Precio:** $8/mes (Plus, solo Grok), $30/mes (Premium, Claude + otros).
- **Soberanía:** ⚠️ Por defecto envía datos a servidores de Cursor. Se puede configurar con endpoint local pero no es local-by-default.
- **Lai vs Cursor:** Lai es CLI y 100% local por regla 0. Cursor es IDE gráfico con mejor UX pero cloud por defecto y de pago.

### 3.3 Continue
- **Qué es:** Extensión de código abierto para VS Code y JetBrains. Permite usar cualquier modelo (local o cloud).
- **Modelo:** Open source Apache 2.0. Configurable con Ollama, LM Studio, llama.cpp.
- **Precio:** Gratis.
- **Soberanía:** ✅ Si se configura con endpoint local, todo queda en la máquina.
- **Lai vs Continue:** Continue tiene mejor integración con IDE (autocomplete inline, chat sidebar). Lai tiene más herramientas integradas (calidad de código, OCR, CI local, AutoMode con auto-evaluación). Continue es más flexible para uso en IDE; lai es más estricto en soberanía y tiene más funcionalidades de código integradas.

### 3.4 Cline
- **Qué es:** Extensión de VS Code (fork de Aider/Claude Code estilo). Agente que lee/escribe archivos, ejecuta comandos, pide aprobación paso a paso.
- **Modelo:** Open source Apache 2.0. Soporta Ollama, LM Studio, y endpoints OpenAI-compatible.
- **Precio:** Gratis.
- **Soberanía:** ✅ Configurable para 100% local.
- **Lai vs Cline:** Cline tiene aprobación humana paso a paso para herramientas (más seguro para acciones destructivas). Lai tiene AutoMode con auto-evaluación de tarea pero sin aprobación paso a paso. Cline es mejor para usuarios que quieren control granular; lai es mejor para usuarios que quieren automatización sin intervención y calidad de código integrada.

### 3.5 Aider
- **Qué es:** CLI de pair programming git-native. Lee/escribe archivos y hace commits automáticos.
- **Modelo:** Open source Apache 2.0. Soporta cualquier modelo vía API o local.
- **Precio:** Gratis.
- **Soberanía:** ✅ Configurable para 100% local.
- **Lai vs Aider:** Aider es git-native (commits automáticos, rollback fácil). Lai no hace commits automáticos. Aider es mejor para pair programming con git; lai es mejor para asistente de código con calidad integrada y RAG.

### 3.6 Tabby
- **Qué es:** Servidor de IA para código auto-hospedado con plugins para IDE.
- **Modelo:** Open source. Self-hosted en la infraestructura del usuario.
- **Precio:** Gratis (self-hosted) / Enterprise (pago).
- **Soberanía:** ✅ Todo en infraestructura propia.
- **Lai vs Tabby:** Tabby es un servidor centralizado para equipos con plugins de IDE. Lai es un CLI individual. Tabby es mejor para equipos que quieren un servidor compartido; lai es mejor para desarrolladores individuales que quieren algo simple y sin servidor.

### 3.7 Sourcegraph Cody
- **Qué es:** Asistente de código con búsqueda semántica de codebase y contexto de repositorio.
- **Modelo:** Freemium. Self-hosted posible (Sourcegraph Enterprise).
- **Precio:** Gratis (individual) / $59/user/mes (enterprise).
- **Soberanía:** ✅ Self-hosted posible.
- **Lai vs Cody:** Cody tiene búsqueda semántica de codebase más madura (contexto de repositorio grande). Lai tiene RAG básico. Cody es mejor para codebases enormes; lai es mejor para simplicidad y costo cero.

### 3.8 Tabnine
- **Qué es:** Asistente de código con enfoque en privacidad y entrenamiento privado.
- **Modelo:** Freemium. Self-hosted posible para enterprise.
- **Precio:** Gratis (básico) / $39/user/mes (pro).
- **Soberanía:** ✅ Self-hosted posible para enterprise.
- **Lai vs Tabnine:** Lai es 100% gratis y open source sin niveles de pago. Tabnine tiene versión gratuita limitada y versión de pago. Lai no requiere cuenta ni cloud.

---

## 4. Lo que lai tiene y las herramientas comunes no

Estas son capacidades distintivas de lai que ninguna de las herramientas comunes ofrece de forma integrada:

1. **Regla 0 estricta:** Cero dependencia de cloud, API keys o cuentas. Garantizado por diseño, no configurable.
2. **Calidad de código integrada:** ruff, mypy, bandit, radon, vulture en pre-commit local y `make check`/`make quality`.
3. **CI local 100% sin cloud:** `scripts/ci_local.sh` — pipeline CI completo sin cuentas GitHub/GitLab ni cloud.
4. **OCR integrado:** `lai ocr <imagen>` con tesseract local.
5. **System One:** Marco de decisiones (choice/noul/score) integrado.
6. **AutoMode con auto-evaluación:** El agente evalúa por sí mismo si la tarea está completada.
7. **MCP servidor stdio:** 23 herramientas expuestas como servidor MCP.
8. **Costo cero:** Totalmente gratuito, sin niveles de pago, sin enterprise.

---

## 5. Lo que las herramientas comunes tienen y lai no

1. **Integración con IDE:** Autocomplete inline, chat sidebar, comandos de menú en VS Code/JetBrains (Cursor, Copilot, Continue, Cline, Cody, Tabnine).
2. **Búsqueda semántica de codebase madura:** Indexación completa de repositorios grandes con retrieval avanzado (Cody, Continue, Tabby).
3. **Aprobación humana paso a paso:** Control granular sobre cada acción del agente (Cline).
4. **Commits automáticos git:** Rollback fácil mediante git (Aider).
5. **Interfaz gráfica:** IDE completo o extensión visual (Cursor, Copilot, Cody, Tabnine).
6. **Soporte comercial enterprise:** Garantía, SLA, soporte técnico (Tabby, Cody, Tabnine enterprise).
7. **Modelos frontier:** Acceso a GPT-5, Claude Opus 4.7, etc. mediante suscripción (Copilot, Cursor).

---

## 6. Conclusión honesta

**lai no es un reemplazo directo de las herramientas comunes.** Es una herramienta diferente con un enfoque distinto:

- **lai** es para desarrolladores que priorizan **soberanía local garantizada, costo cero, calidad de código integrada y CI sin cloud**. Es CLI-only, sin interfaz gráfica, sin soporte comercial.

- **Las herramientas comunes** (Copilot, Cursor, Continue, Cline, Aider, Tabby, Cody, Tabnine) son para desarrolladores que priorizan **integración con IDE, modelos frontier, interfaz gráfica, búsqueda de codebase madura y/o soporte comercial**, y están dispuestos a pagar o a usar cloud.

**No son mutuamente excluyentes:** Un desarrollador puede usar Cursor o Copilot para el trabajo diario con integración IDE, y lai para tareas específicas donde necesita 100% local, calidad de código integrada, OCR o CI local sin costo.

Para un usuario individual o pequeño equipo con presupuesto cero y requisitos de privacidad estrictos, **lai es una opción sólida y única** en su combinación de características. Para un equipo enterprise que necesita soporte comercial, modelos frontier y integración IDE, las herramientas comunes son más adecuadas.

---

## 7. Metodología

Esta comparación se basa en:
- Código fuente de `lai` (commit `e413fe4`, rama `master`).
- Documentación pública y precios de cada herramienta (accedidos vía web search el 2026-11-22).
- Pruebas ejecutadas localmente: `make ci-quick` (119 passed, 3 skipped), `ruff check` (0 errores), `mypy` (0 errores).

**Limitación:** Algunos precios y capacidades de herramientas de terceros se inferen de documentación pública sin haber sido probadas directamente en este entorno. Los precios pueden cambiar.

