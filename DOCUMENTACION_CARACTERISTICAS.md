# Documentación de Características de lai

**lai** - Local AI Code Assistant  
Versión: actual  
Fecha: 2026-11-22

> lai existe para impulsar el uso de IAs locales, abiertas y soberanas.  
> Regla 0: sin dependencia obligatoria de cloud, API keys, cuentas externas ni software propietario.

---

## 1. Visión general

`lai` es un CLI de asistente de código 100% local-first escrito en Python. Ejecuta modelos de lenguaje abiertos (Qwen3-8B por defecto) servidos localmente vía **llama.cpp** o **Ollama**. No requiere cuentas en la nube, ni API keys, ni software propietario.

---

## 2. Comandos disponibles

| Comando | Descripción |
|---------|-------------|
| `lai chat <texto>` | Chat con el modelo local |
| `lai interactive` | Terminal interactivo continuo (sesión tipo REPL con historial) |
| `lai code <desc> <archivo>` | Generar código Python con refinamiento iterativo |
| `lai review <archivo>` | Revisar código con RAG semántico |
| `lai auto <meta>` | Modo autónomo con auto-evaluación de tarea |
| `lai fix <archivo>` | Corregir problemas de código automáticamente |
| `lai repair <archivo>` | Alias de fix |
| `lai run <script>` | Ejecutar un script Python |
| `lai serve --port 8080` | Iniciar servidor API local |
| `lai status` | Estado de servicios (llama.cpp, gateway, ollama) |
| `lai models` | Listar modelos disponibles |
| `lai tools` | Listar herramientas del ToolRegistry |
| `lai search <patrón>` | Buscar código/archivos |
| `lai find <ruta>` | Encontrar archivos |
| `lai think <problema>` | Modo thinking (razonamiento profundo) |
| `lai ocr <imagen>` | OCR con tesseract local |
| `lai mcp` | Servidor MCP (23 herramientas vía stdio) |
| `lai system-one ...` | Sistema de decisiones (choice/noul/score) |
| `lai rag <consulta>` | Búsqueda semántica local con RAG |
| `lai agents <acción>` | Sub-agentes especializados (multi-agente) |
| `lai version` | Mostrar versión |
| `lai completion` | Generar script de completado de shell |

---

## 3. Mejoras principales implementadas

### 3.1 Modo YOLO global (`--yolo`)

- **Qué hace**: Desactiva todas las aprobaciones humanas para herramientas sensibles, permitiendo autonomía total.
- **Cómo usarlo**: `lai <comando> --yolo`
- **Comportamiento**: Cuando `--yolo` está activo, `ToolRegistry.disable_approval()` se llama automáticamente. Sin `--yolo`, las aprobaciones están activadas por defecto (estilo Cline).
- **Archivo**: `lai.py`

### 3.2 Terminal interactivo continuo (`lai interactive`)

- **Qué hace**: Sesión tipo REPL con historial de conversación, comandos especiales y búsqueda RAG integrada.
- **Comandos especiales**:
  - `/exit`, `/quit` - Salir
  - `/help` - Ayuda
  - `/clear` - Limpiar historial
  - `/history` - Ver historial
  - `/rag <consulta>` - Búsqueda semántica local
  - `/index` - Indexar directorio actual en RAG
  - `/model` - Mostrar modelo actual
  - `/temp <n>` - Cambiar temperatura
  - `/stream` - Alternar streaming on/off
- **Archivo**: `lai.py` (función `interactive_cmd`)

### 3.3 Aprobación humana paso a paso (estilo Cline)

- **Qué hace**: Antes de ejecutar herramientas sensibles (escribir archivos, ejecutar comandos, aplicar parches, ejecutar Python, git), pide confirmación al usuario.
- **Herramientas protegidas**: `write_file`, `run_command`, `apply_patch`, `run_python`, `fix_imports`, `fix_syntax`, `fix_formatting`, `fix_types`, `auto_fix`, `fix_missing_init`, `fix_circular_imports`, `fix_relative_imports`, `normalize_imports`, `git_init`, `git_add`, `git_commit`, `git_rollback`.
- **Métodos**: `enable_approval()`, `disable_approval()`, `is_approval_enabled()`.
- **Archivo**: `lai_tools.py`

### 3.4 Herramientas Git (estilo Aider)

- **Qué hace**: Permite hacer commits automáticos, rollback, y ver estado/diff del repositorio.
- **Herramientas**:
  - `git_init(path)` - Inicializar repositorio git
  - `git_add(path)` - Agregar archivos al staging
  - `git_commit(message, path)` - Hacer commit
  - `git_rollback(path)` - Deshacer último commit (reset --hard HEAD~1)
  - `git_status(path)` - Mostrar estado
  - `git_diff(path)` - Mostrar diff no staged
- **Archivo**: `lai_tools.py`

### 3.5 RAG semántico maturo (chunking inteligente + búsqueda híbrida)

- **Qué hace**: Indexación semántica de código y documentación con búsqueda por similitud de coseno + TF-IDF keyword.
- **Nuevas funcionalidades**:
  - **Chunking inteligente**: Divide código en bloques semánticos (funciones, clases) respetando la estructura del código.
  - **Búsqueda híbrida**: Combina similitud coseno (vector) + TF-IDF (keyword) con pesos configurables.
  - **Indexación incremental**: Solo indexa archivos nuevos o modificados (detección por hash SHA-256).
  - **Watcher de archivos**: Monitorea cambios en directorio en tiempo real (requiere `watchdog`).
- **Comandos**:
  - `lai rag <consulta> --chunked` - Indexar con chunking inteligente
  - `lai rag <consulta> --hybrid` - Búsqueda híbrida vector+keyword
  - `lai rag <consulta> --index --chunked` - Indexar y buscar con chunking
- **Archivo**: `scripts/rag_local.py`

### 3.6 Multi-agente especializado (sub-agentes)

- **Qué hace**: Orquesta múltiples sub-agentes especializados para resolver tareas complejas de forma colaborativa.
- **Sub-agentes disponibles**:
  - `code` - Generación de código
  - `review` - Revisión de código (bugs, seguridad, rendimiento)
  - `test` - Generación de pruebas unitarias
  - `doc` - Documentación técnica
  - `fix` - Corrección de errores/debugging
  - `arch` - Arquitectura de software
- **Acciones**:
  - `lai agents list` - Listar sub-agentes
  - `lai agents run --agent <nombre> --task <tarea>` - Ejecutar un sub-agente
  - `lai agents pipeline --task "code:tarea1,review:tarea2"` - Pipeline de agentes
  - `lai agents collaborative --goal <meta>` - Flujo colaborativo code→review→test→doc
- **Archivo**: `scripts/lai_agents.py`

### 3.7 Soporte multi-proveedor de modelos

- **Qué hace**: Permite conectar a diferentes proveedores de modelos, no solo llama.cpp/Ollama.
- **Proveedores soportados**: `ollama`, `llama-cpp`, `openai`, `anthropic`, `gemini`, `custom`.
- **Argumentos**:
  - `--provider <nombre>` - Proveedor de modelo (default: ollama)
  - `--api-key <clave>` - API key para proveedores remotos
  - `--allow-remote` - Permitir endpoints no locales (bypassa Regla 4)
- **Archivo**: `lai.py`

### 3.8 Calidad de código integrada

- **Qué hace**: Verificación automática de calidad con ruff, mypy, bandit, radon y vulture.
- **CI local 100%**: `scripts/ci_local.sh` ejecuta pipeline CI sin cuentas de cloud ni API keys.
- **Pre-commit local**: `.pre-commit-config.yaml` con hooks `language: system` (sin GitHub remotos).
- **Archivos**: `scripts/ci_local.sh`, `.pre-commit-config.yaml`, `Makefile`, `pyproject.toml`, `.bandit`

### 3.9 OCR integrado

- **Qué hace**: Reconocimiento óptico de caracteres local con tesseract.
- **Comando**: `lai ocr <imagen> [--lang spa+eng] [--output archivo]`
- **Archivo**: `lai_tools.py` (herramientas `ocr_image`, `ocr_image_best`, `ocr_screenshot`)

### 3.10 System One (toma de decisiones)

- **Qué hace**: Marco de decisiones (choice/noul/score) para decisiones estructuradas.
- **Comando**: `lai system-one --state <estado> --type <tipo> --question <pregunta> --options <opciones>`
- **Archivo**: `lai.py` (función `system_one_cmd`)

---

## 4. ToolRegistry (45+ herramientas)

El `ToolRegistry` expone todas las herramientas como un servidor MCP estándar y como funciones Python directamente invocables:

**Categorías**:
- Operaciones de archivos: `read_file`, `write_file`, `list_dir`, `find_files`, `repo_map`
- Ejecución: `run_command`, `run_python`, `run_tests`, `python_eval`
- Análisis de código: `lint_file`, `format_code`, `check_types`, `analyze_complexity`, `find_unused_imports`, `find_unused_code`, `find_todos`, `check_security`, `check_dead_code`, `analyze_dependencies`
- Validación: `validate_syntax`, `validate_imports`, `validate_types`, `validate_project`
- Auto-reparación: `fix_imports`, `fix_syntax`, `fix_formatting`, `fix_types`, `auto_fix`
- Análisis de proyecto: `analyze_project`, `get_project_structure`, `find_duplicates`, `check_circular_imports`
- Reparación: `fix_missing_init`, `fix_circular_imports`, `fix_relative_imports`, `normalize_imports`
- Calidad: `run_quality_check`, `generate_type_stubs`, `check_docstrings`
- OCR/Vision: `ocr_image`, `ocr_image_best`, `ocr_screenshot`
- Git: `git_init`, `git_add`, `git_commit`, `git_rollback`, `git_status`, `git_diff`

---

## 5. Comparación con otras herramientas

| Característica | lai | Copilot | Cursor | Cline | Aider | Cody |
|----------------|-----|---------|--------|-------|-------|------|
| 100% local por defecto | ✅ | ❌ | ⚠️ | ✅ | ✅ | ⚠️ |
| Sin API keys obligatorias | ✅ | ❌ | ❌ | ✅ | ✅ | ⚠️ |
| Sin cuenta requerida | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ |
| Gratis (sin niveles de pago) | ✅ | ❌ | ❌ | ✅ | ✅ | ⚠️ |
| Calidad de código integrada | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| CI local sin cloud | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| OCR integrado | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Aprobación humana paso a paso | ✅ | ❌ | ⚠️ | ✅ | ❌ | ❌ |
| Commits automáticos git | ✅ | ❌ | ❌ | ❌ | ✅ | ❌ |
| RAG semántico sobre código | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Multi-agente especializado | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Terminal interactivo | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| MCP servidor stdio | ✅ | ❌ | ❌ | ✅ | ❌ | ❌ |

---

## 6. Regla 0 (Soberanía local)

> *Cualquier feature, arquitectura o dependencia que introduzca dependencia obligatoria de cloud, API keys, cuentas externas o software propietario será rechazada salvo aprobación explícita y documentada del operador.*

- Todo el procesamiento ocurre en la máquina del usuario.
- Gateway bind a loopback (127.0.0.1).
- `is_local_url()` valida endpoints antes de cualquier generación.
- `--allow-remote` es opt-in explícito para bypassar la Regla 4.
- Ningún dato sale de la máquina salvo decisión explícita del operador.

---

## 7. Instalación y uso rápido

```bash
# Instalar dependencias
pip install -e .

# Verificar estado de servicios
lai status

# Chat interactivo
lai interactive

# Generar código
lai code "crear una función factorial" factorial.py

# Revisar código con RAG
lai review lai.py --index --chunked --hybrid

# Modo autónomo
lai auto "crear un REST API con FastAPI" --yolo

# Sub-agentes colaborativos
lai agents collaborative --goal "crear un sistema de autenticación"

# OCR de imagen
lai ocr documento.png --lang spa+eng
```

---

## 8. Arquitectura del proyecto

```
lai/
├── lai.py                    # CLI principal, clase LAI, todos los comandos
├── lai_tools.py              # ToolRegistry con 45+ herramientas
├── lai_automode.py           # AutoMode con auto-evaluación de tarea
├── scripts/
│   ├── rag_local.py          # RAG semántico local (chunking + búsqueda híbrida)
│   ├── lai_agents.py         # Sub-agentes especializados (multi-agente)
│   ├── context_history.py    # Gestión de sesión/historial/contexto
│   ├── ci_local.sh           # Pipeline CI 100% local
│   └── mcp_server.py         # Servidor MCP (stdio)
├── tests/                    # Pruebas (119 passed, 3 skipped)
├── Makefile                  # Targets: ci, ci-quick, check, quality, lint, format, type, test
├── pyproject.toml            # Configuración del proyecto
├── .pre-commit-config.yaml   # Hooks pre-commit 100% locales
├── .bandit                   # Configuración de bandit
└── scripts/ci_local.sh       # Pipeline CI local
```

---

## 9. Limitaciones conocidas

1. **RAG**: El chunking es básico (no usa tree-sitter para parseo AST-aware). La búsqueda híbrida usa TF-IDF simple, no BM25 sofisticado.
2. **Multi-agente**: Los sub-agentes no comparten memoria vectorial entre sí; cada uno tiene su propio system prompt.
3. **Autocompletado**: No hay autocompletado inline tipo Cursor/Copilot en el terminal.
4. **LSP**: No hay integración completa con Language Server Protocol para navegación de símbolos.
5. **Hardware**: Requiere máquina con suficiente RAM/VRAM para ejecutar modelos locales (Qwen3-8B-Q4_K_M necesita ~6-8GB VRAM).

---

## 10. Hoja de ruta (próximas mejoras)

- [ ] Integración tree-sitter para chunking AST-aware en RAG
- [ ] Autocompletado de código en terminal (LSP + completado)
- [ ] Integración LSP completa para navegación de símbolos
- [ ] Memoria vectorial compartida entre sub-agentes
- [ ] Soporte para más lenguajes de programación en chunking
- [ ] Benchmarks de calidad de código generado

---

*Documentación generada para la versión actual de lai. Toda la información refleja el código fuente en el repositorio.*
