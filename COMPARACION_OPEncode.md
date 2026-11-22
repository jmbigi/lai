# Comparación honesta: lai vs OpenCode

**Fecha:** 2026-11-22  
**Objetivo:** Comparación veraz entre `lai` (CLI de asistente de código local-first) y `OpenCode` (agente de código terminal de código abierto, MIT).

> OpenCode es un proyecto real (opencode.dev, GitHub). Esta comparación se basa en su documentación pública y en el código fuente de `lai` (commit `e413fe4`).

---

## 1. Resumen ejecutivo

| Aspecto | lai | OpenCode |
|---------|-----|----------|
| **Naturaleza** | CLI + librería Python (asistente de código) | Agente de código terminal (MIT) |
| **Interfaz** | CLI (`lai code`, `lai review`, `lai auto`, ...) | Terminal interactivo (chat + comandos) |
| **Modelo por defecto** | Qwen3-8B-Q4_K_M (llama.cpp/Ollama) | Configurable (75+ proveedores) |
| **Proveedores soportados** | llama.cpp, Ollama (local-only por defecto) | Ollama, LM Studio, llama.cpp, OpenAI, Anthropic, etc. |
| **Cloud por defecto** | ❌ No (regla 0: local-first estricto) | ✅ Sí, configurable (opcional local) |
| **RAG semántico sobre código** | ✅ `scripts/rag_local.py` (básico, numpy) | ✅ (indexación de repositorio) |
| **Memoria persistente** | ✅ JSON en `~/.local-ai/` | ✅ Sesiones persistentes |
| **AutoMode / agente autónomo** | ✅ Con auto-evaluación de tarea | ✅ Agente con tool-calling |
| **Calidad de código integrada** | ✅ ruff, mypy, bandit, pre-commit local | ❌ No |
| **OCR integrado** | ✅ tesseract (`lai ocr`) | ❌ No |
| **System One (decisiones)** | ✅ (choice/noul/score) | ❌ No |
| **MCP servidor stdio** | ✅ 23 herramientas | ✅ (cliente MCP) |
| **CI local 100% sin cloud** | ✅ `scripts/ci_local.sh` | ❌ No (es software, no CI) |
| **Precio** | Gratis (open source) | Gratis (open source, MIT) |
| **Soberanía de datos** | ✅ 100% local por defecto | ✅ Si se configura con endpoint local |

---

## 2. ¿Qué es cada uno?

### lai
- **CLI de asistente de código local-first.** Escrito en Python (~2857 líneas en `lai.py` + módulos separados).
- Regla 0: sin dependencia obligatoria de cloud, API keys ni cuentas externas. Todo por defecto es local.
- Comandos: `code`, `review`, `auto`, `fix`, `review`, `serve`, `ocr`, `tools`, `rag`, `system-one`, `status`.
- Incluye RAG semántico básico, historial de sesiones, OCR con tesseract, AutoMode con auto-evaluación, System One para decisiones, calidad de código (ruff/mypy/bandit/pre-commit local).

### OpenCode
- **Agente de código terminal de código abierto** (licencia MIT). Desarrollado como alternativa local a herramientas tipo Claude Code/Cursor.
- Terminal interactivo con chat, permite leer/escribir archivos, ejecutar comandos, y usar tools.
- Soporta 75+ proveedores de modelos: Ollama, LM Studio, llama.cpp, OpenAI, Anthropic, Google, etc.
- Se conecta a cualquier endpoint OpenAI-compatible, incluyendo servidores locales (Ollama en 11434, llama.cpp en 8080).
- No impone cloud: el usuario elige el endpoint. Pero no es local-by-default estricto como lai.

---

## 3. Matriz detallada de capacidades

| Capacidad | lai | OpenCode |
|-----------|-----|----------|
| **CLI nativa** | ✅ | ✅ |
| **Terminal interactivo** | ❌ (comandos disparo y listo) | ✅ (sesión interactiva continua) |
| **RAG semántico sobre código** | ✅ Básico (cosine + sentence-transformers) | ✅ Indexación de repositorio |
| **Memoria persistente entre sesiones** | ✅ JSON (`~/.local-ai/`) | ✅ Sesiones |
| **Memoria vectorial semántica automática** | ❌ No | ✅ (retrieval sobre código indexado) |
| **AutoMode agente autónomo** | ✅ Con auto-evaluación | ✅ Tool-calling loop |
| **Aprobación humana para herramientas** | ❌ No | ✅ (opcional, paso a paso) |
| **Multi-agente** | ❌ No | ❌ No |
| **Calidad de código (lint/typing/security)** | ✅ ruff, mypy, bandit, radon, vulture | ❌ No |
| **Pre-commit 100% local** | ✅ | ❌ No |
| **CI local sin cuentas/cloud** | ✅ `scripts/ci_local.sh` | ❌ No |
| **OCR integrado** | ✅ tesseract | ❌ No |
| **System One (decisiones choice/noul/score)** | ✅ | ❌ No |
| **MCP servidor (exponer tools)** | ✅ 23 herramientas stdio | ✅ Cliente MCP (conectar a servidores) |
| **MCP cliente (usar servidores externos)** | ❌ No | ✅ |
| **Modelos soportados** | llama.cpp, Ollama (local) | 75+ (incluyendo cloud) |
| **Local-by-default estricto** | ✅ Regla 0 | ⚠️ Configurable |
| **Requiere Python** | ✅ Sí | ❌ No (binary/Node o Go) |
| **Soporte comercial** | ❌ Comunidad | ❌ Comunidad |
| **Soberanía de datos** | ✅ 100% local por defecto | ✅ Si endpoint es local |

---

## 4. Análisis por área

### 4.1 Enfoque y filosofía

**lai:** Regla 0 estricta — software gratuito, open source, sin cloud obligatorio, sin API keys, sin cuentas. Diseñado para soberanía total. El modelo corre en la máquina del usuario vía llama.cpp o Ollama.

**OpenCode:** Código abierto MIT, agente terminal flexible. El usuario elige el proveedor de modelo (local o cloud). No impone cloud, pero tampoco lo prohíbe ni lo garantiza por defecto. Más orientado a ser un "agente de programación" generalista en terminal.

**Veredicto:** Lai es más estricto en soberanía local. OpenCode es más flexible pero requiere configuración deliberada para mantenerse 100% local.

### 4.2 Asistente de código vs agente terminal

**lai:** Herramienta especializada en asistencia de código con comandos específicos (`code`, `review`, `auto`, `fix`, `ocr`). Incluye calidad de código integrada (ruff, mypy, bandit).

**OpenCode:** Agente terminal generalista. Lee/escribe archivos, ejecuta comandos, usa tools. No tiene comandos especializados de código ni linting integrado.

**Veredicto:** Lai gana como asistente de código especializado. OpenCode es más general pero menos enfocado en calidad de código.

### 4.3 RAG y memoria

**lai:** `scripts/rag_local.py` con `sentence-transformers` + `numpy`. Búsqueda por similitud de coseno. Multi-colección. Persistencia JSON + `.npy`. **Limitación:** no hay indexación automática incremental ni watcher.

**OpenCode:** Indexación de repositorio con retrieval semántico. Memoria de sesiones persistentes. Más maduro en RAG para código que lai.

**Veredicto:** OpenCode tiene RAG más maduro para código. Lai tiene RAG funcional pero básico.

### 4.4 Calidad de código y CI

**lai:** Fortaleza clara. `scripts/ci_local.sh` (CI 100% local sin cloud), pre-commit local (ruff, black, mypy, bandit, radon, vulture), `make check`, `make quality`.

**OpenCode:** No incluye herramientas de calidad de código ni CI. Es un agente, no un linter.

**Veredicto:** Lai destaca claramente en esta área. OpenCode no aplica.

### 4.5 OCR

**lai:** `lai ocr <imagen>` con tesseract local. Soporta `--lang` y `--output`.

**OpenCode:** No tiene OCR integrado.

**Veredicto:** Lai es superior en OCR.

### 4.6 Modelo y proveedores

**lai:** llama.cpp + Ollama. Modelo por defecto Qwen3-8B-Q4_K_M. Local-only por regla 0.

**OpenCode:** 75+ proveedores. Soporta Ollama, LM Studio, llama.cpp, OpenAI, Anthropic, Google, etc. El usuario configura el endpoint.

**Veredicto:** OpenCode soporta más modelos (incluyendo cloud). Lai es más limitado pero garantiza 100% local.

---

## 5. Limitaciones honestas

### De lai
1. **RAG básico:** Sin indexación automática incremental ni búsqueda híbrida sofisticada.
2. **Sin multi-agente:** AutoMode es un solo agente.
3. **CLI-only:** No tiene terminal interactivo continuo ni IDE integrado.
4. **Sin aprobación humana para herramientas:** Ejecuta tools sin pedir confirmación paso a paso.
5. **Requiere Python y dependencias:** No es un binary standalone.
6. **Cobertura de tests:** ~31%, lejos del ideal para producción.

### De OpenCode
1. **No es local-by-default estricto:** Puede conectar a cloud si el usuario lo configura. No tiene una "regla 0" equivalente.
2. **Sin calidad de código integrada:** No tiene ruff, mypy, bandit, ni CI local.
3. **Sin OCR:** No incluye OCR integrado.
4. **Sin System One:** No tiene marco de decisiones choice/noul/score.
5. **Soporte comunitario:** Sin garantía comercial.

---

## 6. Conclusión honesta

**lai y OpenCode son herramientas diferentes con superposición parcial:**

- **lai** es un CLI de asistente de código local-first con reglas de soberanía estrictas, calidad de código integrada (ruff/mypy/bandit/CI local), OCR, RAG sobre código, AutoMode con auto-evaluación y System One. Ideal para desarrolladores que priorizan soberanía local, calidad de código y costo cero.

- **OpenCode** es un agente de código terminal generalista, flexible con 75+ proveedores de modelo, sesión interactiva continua, RAG de repositorio y memoria de sesiones. Ideal para desarrolladores que quieren un agente tipo Claude Code pero con opción de modelos locales.

**No son mutuamente excluyentes:** Un usuario puede usar OpenCode para trabajo agente interactivo en terminal y lai para comandos especializados de código, calidad de código, OCR y CI local.

Para un usuario que prioriza **soberanía local garantizada, calidad de código integrada y CI sin cloud**, lai es la opción más alineada. Para un usuario que prioriza **agente terminal interactivo con muchos proveedores de modelo**, OpenCode es más adecuado.

---

## 7. Metodología

Esta comparación se basa en:
- Código fuente de `lai` (commit `e413fe4`, rama `master`).
- Documentación pública de OpenCode (accedida vía web search el 2026-11-22).
- Pruebas ejecutadas localmente: `make ci-quick` (119 passed, 3 skipped), `ruff check` (0 errores), `mypy` (0 errores).

**Limitación:** Algunas capacidades de OpenCode se inferen de su documentación pública sin haber sido probadas directamente en este entorno.
