# Comparación honesta: lai vs Kimicode

**Fecha:** 2026-11-22  
**Objetivo:** Comparación veraz entre `lai` (CLI de asistente de código local-first) y `Kimicode` (servicio de API alojado de LLM para código, endpoint OpenAI-compatible).

> Kimicode es un servicio real (kimicodeapi.com, kimicode.top, kimicode.cc). Esta comparación se basa en su documentación pública y en el código fuente de `lai` (commit `e413fe4`).

---

## 1. Resumen ejecutivo

| Aspecto | lai | Kimicode |
|---------|-----|----------|
| **Naturaleza** | CLI + librería Python (software local) | Servicio de API alojado (SaaS) |
| **Modelo de negocio** | Open source gratuito | Pago por uso ($0.15/1M input, $0.50/1M output) |
| **Inferencia** | llama.cpp / Ollama local (100% en tu máquina) | Servidores de Kimicode (cloud) |
| **Datos de código** | Nunca salen de tu máquina | Se envían a servidores de Kimicode |
| **Cuenta requerida** | ❌ No | ✅ Sí (prepaid credit) |
| **API key obligatoria** | ❌ No | ✅ Sí |
| **Cloud obligatorio** | ❌ No (regla 0 estricta) | ✅ Sí |
| **RAG semántico sobre código** | ✅ `scripts/rag_local.py` (local, básico) | ❌ No (solo API de chat) |
| **OCR integrado** | ✅ tesseract (`lai ocr`) | ❌ No |
| **Calidad de código** | ✅ ruff, mypy, bandit, CI local | ❌ No |
| **AutoMode autónomo** | ✅ Con auto-evaluación | ❌ No (es solo una API) |
| **MCP servidor** | ✅ 23 herramientas stdio | ❌ No |
| **Soberanía de datos** | ✅ 100% local | ❌ Los datos salen de la máquina |
| **Precio** | $0 | ~$0.17 por sesión de 40 llamadas |

---

## 2. ¿Qué es cada uno?

### lai
- **CLI de asistente de código local-first.** Escrito en Python.
- Regla 0: sin dependencia obligatoria de cloud, API keys ni cuentas externas.
- Todo corre en la máquina del usuario vía llama.cpp o Ollama.
- Comandos: `code`, `review`, `auto`, `fix`, `ocr`, `rag`, `system-one`, `tools`, `status`.
- Incluye RAG semántico local, historial de sesiones, OCR con tesseract, AutoMode con auto-evaluación, calidad de código integrada.

### Kimicode
- **Servicio de API alojado** (kimicodeapi.com, kimicode.top, kimicode.cc).
- No es el API oficial de Kimi/Moonshot; es un servicio independiente que aloja un modelo open-weight.
- Endpoint OpenAI-compatible (`POST /v1/chat/completions`, `GET /v1/models`).
- Precio: $0.15 por 1M input tokens, $0.50 por 1M output tokens (prepaid credit, sin suscripción).
- Promesa: salida "sin censura" para temas legales de adulto/ficción/controversiales (solo bloquea contenido sexual con menores).
- Soporte: streaming, JSON mode, function calling/tools.
- Contexto: 100,000 tokens.

---

## 3. Matriz detallada de capacidades

| Capacidad | lai | Kimicode |
|-----------|-----|----------|
| **Tipo** | Software CLI | Servicio API alojado |
| **Licencia** | Open source | Propietario (servicio) |
| **Inferencia local** | ✅ llama.cpp / Ollama | ❌ No (servidores remotos) |
| **Cuenta requerida** | ❌ No | ✅ Sí |
| **API key obligatoria** | ❌ No | ✅ Sí |
| **Código sale de tu máquina** | ❌ No | ✅ Sí (se envía a Kimicode) |
| **Costo** | $0 | Pago por uso |
| **Precio input tokens** | $0 | $0.15 / 1M |
| **Precio output tokens** | $0 | $0.50 / 1M |
| **Modelo por defecto** | Qwen3-8B-Q4_K_M | Modelo open-weight único (no especificado) |
| **Contexto** | 32,768 (llama.cpp) | 100,000 tokens |
| **Streaming** | Depende del servidor local | ✅ |
| **JSON mode** | Depende del servidor local | ✅ |
| **Function calling / tools** | ✅ ToolRegistry (45+ herramientas) | ✅ |
| **RAG semántico código** | ✅ Local (numpy + sentence-transformers) | ❌ No |
| **Historial de sesiones** | ✅ JSON en `~/.local-ai/` | ❌ No |
| **OCR integrado** | ✅ tesseract | ❌ No |
| **Calidad de código (lint/typing/sec)** | ✅ ruff, mypy, bandit | ❌ No |
| **CI local 100% sin cloud** | ✅ `scripts/ci_local.sh` | ❌ No |
| **AutoMode agente autónomo** | ✅ Con auto-evaluación | ❌ No (solo API) |
| **System One decisiones** | ✅ (choice/noul/score) | ❌ No |
| **MCP servidor stdio** | ✅ 23 herramientas | ❌ No |
| **Soberanía de datos** | ✅ 100% local | ❌ Datos en servidores de Kimicode |
| **Censura/filtros** | Tú controlas todo | "Sin censura" para temas legales adultos |

---

## 4. Análisis por área

### 4.1 Naturaleza: software vs servicio

**lai:** Software que descargas y ejecutas. Tú controlas todo: el modelo, el servidor, los datos, las herramientas.

**Kimicode:** Servicio al que te suscribes y envías tu código. Ellos controlan el modelo y la infraestructura; tú pagas por uso.

**Veredicto:** Son categorías diferentes. lai es una herramienta local; Kimicode es un servicio cloud.

### 4.2 Soberanía y privacidad

**lai:** Regla 0 estricta. Nada sale de tu máquina. Gateway bind a loopback. `is_local_url()` valida endpoints.

**Kimicode:** Tu código y prompts se envían a servidores de Kimicode. Aunque prometen "sin censura", tus datos salen de tu entorno.

**Veredicto:** Lai gana claramente en soberanía y privacidad. Kimicode no puede competir en este aspecto por diseño.

### 4.3 Costo

**lai:** Cero. Sin suscripción, sin pago por token, sin cuenta.

**Kimicode:** Pago por uso. ~$0.17 por sesión típica de 40 llamadas. Sin suscripción pero con crédito prepago.

**Veredicto:** Lai es mucho más barato (gratis). Kimicode tiene costo variable según uso.

### 4.4 Capacidades de asistente de código

**lai:** Comandos específicos para código (`code`, `review`, `auto`, `fix`), RAG sobre código, historial de sesiones, OCR, calidad de código, AutoMode con auto-evaluación, MCP servidor.

**Kimicode:** Solo una API de chat. No tiene comandos de código, no tiene RAG, no tiene historial, no tiene OCR, no tiene calidad de código integrada. Es solo un modelo al que le envías mensajes.

**Veredicto:** Lai es un asistente de código completo. Kimicode es solo una API de modelo.

### 4.5 Modelo y contexto

**lai:** Qwen3-8B-Q4_K_M por defecto. Contexto 32K. Puedes cambiar el modelo.

**Kimicode:** Un solo modelo open-weight (no especificado cuál). Contexto 100K. No eliges el modelo.

**Veredicto:** Lai te da elección de modelo. Kimicode tiene más contexto por defecto pero un solo modelo impuesto.

---

## 5. Limitaciones honestas

### De lai
1. **Requiere hardware propio:** Necesitas RAM/VRAM para ejecutar modelos locales.
2. **RAG básico:** Sin indexación automática incremental ni búsqueda híbrida sofisticada.
3. **Sin multi-agente:** AutoMode es un solo agente.
4. **CLI-only:** No tiene interfaz gráfica ni IDE integrado.
5. **Soporte comunitario:** Sin garantía comercial.

### De Kimicode
1. **Es cloud, no local:** Tus datos salen de tu máquina. No cumple regla 0.
2. **Requiere cuenta y pago:** No es gratuito; necesitas crédito prepago.
3. **Solo es una API:** No es un asistente de código con herramientas integradas.
4. **Un solo modelo:** No eliges qué modelo usar.
5. **No tiene RAG, OCR, calidad de código, AutoMode, MCP:** Son solo capacidades de chat.
6. **Servicio de terceros:** Dependes de su disponibilidad y políticas.

---

## 6. Conclusión honesta

**lai y Kimicode no compiten en la misma categoría:**

- **lai** es software de asistente de código local-first, gratuito, CLI, con regla 0 estricta (100% local, sin cloud, sin cuenta, sin API key). Ideal para desarrolladores que priorizan soberanía, privacidad y costo cero.

- **Kimicode** es un servicio de API alojado de LLM para código. Ideal para desarrolladores que quieren un modelo rápido sin instalar nada, están dispuestos a pagar por uso y no les importa que su código salga de su máquina.

**Un usuario de Kimicode puede usar la API con cualquier herramienta OpenAI-compatible, incluido lai si se configurara como cliente** (aunque lai está diseñado para modelos locales, no para APIs remotas por regla 0).

Para un usuario que prioriza **soberanía local garantizada, privacidad y costo cero**, lai es la opción correcta. Para un usuario que prioriza **conveniencia, no instalar nada y está dispuesto a pagar y enviar código a la cloud**, Kimicode es una opción.

---

## 7. Metodología

Esta comparación se basa en:
- Código fuente de `lai` (commit `e413fe4`, rama `master`).
- Documentación pública de Kimicode (kimicodeapi.com, kimicode.top, kimicode.cc, accedida vía web search el 2026-11-22).
- Pruebas ejecutadas localmente: `make ci-quick` (119 passed, 3 skipped), `ruff check` (0 errores), `mypy` (0 errores).

**Limitación:** Algunas capacidades de Kimicode se inferen de su documentación pública sin haber sido probadas directamente en este entorno. Los precios pueden cambiar.

