# lai - Local AI Code Assistant

**lai** es una CLI de asistencia para código que usa LLMs locales (llama.cpp, Ollama) para revisar, reparar y mejorar proyectos de software. Sigue una regla estricta: toda generación de código usa modelos locales; las APIs de la nube solo se permiten para configurar lai misma.

---

## Regla 0 (Regla de Oro)

> Cualquier feature, arquitectura o dependencia que introduzca dependencia obligatoria de cloud, API keys, cuentas externas o software propietario será rechazada salvo aprobación explícita y documentada del operador.

---

## Tabla de contenidos

1. [Arquitectura](#arquitectura)
2. [Instalación y primera ejecución](#instalacion-y-primera-ejecucion)
3. [CLI y comandos](#cli-y-comandos)
4. [Herramientas integradas (ToolRegistry)](#herramientas-integradas-toolregistry)
5. [MCP (Model Context Protocol)](#mcp-model-context-protocol)
6. [System One (cliente de decisiones)](#system-one-cliente-de-decisiones)
7. [Stress testing](#stress-testing)
8. [CI local 100% open source](#ci-local-100-open-source)
9. [Calidad de código](#calidad-de-codigo)
10. [Testing y cobertura](#testing-y-cobertura)
11. [Reglas de seguridad](#reglas-de-seguridad)
12. [Variables de entorno](#variables-de-entorno)
13. [Estructura de archivos](#estructura-de-archivos)
14. [Roadmap y áreas débiles](#roadmap-y-areas-debiles)
15. [Versionado](#versionado)

---

## Arquitectura

```
lai.py                    # Punto de entrada único (~4581 líneas)
  ├── LAI                 # Clase principal: chat, historial, costo, tokens
  ├── ToolRegistry        # 23+ herramientas de análisis y reparación
  ├── ToolCalling         # Llamada a herramientas nativas (llama.cpp)
  ├── MCP                 # Servidor stdio JSON-RPC 2.0
  ├── SystemOne           # Cliente de decisiones (choice/noul/score)
  └── StressTest          # 10 suites de prueba de estrés

Servicios locales:
  llama.cpp   :127.0.0.1:8080   Qwen3-8B-Q4_K_M, ctx 32768, ngl 99
  gateway-go  :127.0.0.1:8081   Reverse proxy con caché, compresión, filtro
  ollama      :127.0.0.1:11434   Backend alternativo

Gateway auth: Authorization: Bearer open (siempre válido, solo loopback)
```

### Flujo de AutoMode

1. El usuario pide una tarea en lenguaje natural.
2. AutoMode usa llama.cpp native function calling (no heurísticas de keyword).
3. Selecciona la herramienta adecuada del `ToolRegistry`.
4. Ejecuta, valida y itera hasta completar la tarea.
5. Máximo de iteraciones configurable (`--max-iterations`).

---

## Instalación y primera ejecución

```bash
# 1. Instalar paquete en modo editable
make install          # pip install -e .

# 2. Instalar dependencias de desarrollo
make dev-install      # pip install -e .[dev]

# 3. Verificar estado de servicios
make smoke            # python3 lai.py status / tools / review

# 4. Ejecutar versión
lai version           # o python3 lai.py version

# 5. REPL interactivo (default)
lai                   # entra en modo interactivo, lang=es por defecto
```

### Requisitos del sistema

- Python 3.10+
- llama.cpp servidor (puerto 8080) o Ollama (puerto 11434)
- gateway-go (puerto 8081) recomendado
- `black`, `mypy`, `bandit`, `ruff`, `pytest` para desarrollo

---

## CLI y comandos

| Comando | Descripción |
|---------|-------------|
| `lai` | REPL interactivo (idioma es por defecto) |
| `lai version` | Muestra la versión |
| `lai status` | Estado de servicios, costo y tokens |
| `lai tools` | Lista herramientas disponibles |
| `lai review <archivo>` | Revisa código de un archivo |
| `lai fix <archivo>` | Corrige problemas en un archivo |
| `lai code <prompt>` | Genera código desde prompt |
| `lai auto <tarea>` | Modo automático con herramientas |
| `lai think <prompt>` | Modo pensamiento profundo |
| `lai search <patron>` | Busca TODO/FIXME/XXX/HACK |
| `lai find <patron>` | Busca patrones en código |
| `lai test <archivo>` | Ejecuta tests de un archivo |
| `lai init <tipo> <nombre>` | Inicializa un proyecto |
| `lai project <tipo> <nombre>` | Crea proyecto completo |
| `lai config` | Muestra configuración |
| `lai install` | Instala dependencias de proyecto |
| `lai update` | Actualiza dependencias |
| `lai exec <cmd>` | Ejecuta comando de shell |
| `lai doctor` | Diagnostica el entorno |
| `lai clean` | Limpia cachés y artifacts |
| `lai serve` | Inicia servidor HTTP local (127.0.0.1:8082) |
| `lai models` | Lista modelos disponibles |
| `lai mcp` | Gestión del servidor MCP |
| `lai system-one` | Cliente de decisiones System One |
| `lai completion <shell>` | Genera autocomplete (bash/zsh/fish) |

### Flags globales

- `-l / --lang <es\|en>` : Idioma de salida (default: es)
- `--max-iterations N` : Límite de iteraciones AutoMode

---

## Herramientas integradas (ToolRegistry)

El `ToolRegistry` expone 23+ herramientas para análisis y reparación de código:

1. `review` - Revisión de código
2. `fix` - Corrección automática
3. `code` - Generación de código
4. `find_unused_code` - Detección de código muerto
5. `find_todos` - Búsqueda de pendientes (TODO/FIXME)
6. `check_security` - Análisis de seguridad básica
7. `find_duplicates` - Detección de código duplicado
8. `format` - Formateo de código
9. `type_check` - Verificación de tipos (mypy)
10. `complexity` - Complejidad ciclomática (radon)
11. `dead_code` - Código muerto (vulture)
12. `security_scan` - Escaneo de seguridad (bandit)
13. `mcp_server` - Servidor MCP
14. `system_one` - Decisiones System One
15. `stress_test` - Pruebas de estrés
16. `grep` - Búsqueda de patrones
17. `lint` - Linting con ruff
18. `imports` - Análisis de importaciones
19. `docstrings` - Generación de docstrings
20. `tests` - Generación de tests
21. `refactor` - Refactorización
22. `performance` - Análisis de rendimiento
23. `dependencies` - Gestión de dependencias

---

## MCP (Model Context Protocol)

**Archivo:** `scripts/mcp_server.py`

Servidor stdio JSON-RPC 2.0 que expone las herramientas de lai a clientes MCP (Claude Code, LM Studio, Open WebUI, etc.).

### 23 herramientas expuestas

Herramientas de análisis, revisión, generación, formateo, seguridad, tipos, complejidad, dead code, búsqueda, tests, init, project, config, install, update, exec, doctor, clean, serve, models, status, version, system-one, stress.

### Uso

```bash
# Iniciar servidor MCP (stdio)
python3 scripts/mcp_server.py

# O vía CLI
lai mcp
```

### Configuración de cliente MCP

```json
{
  "mcpServers": {
    "lai": {
      "command": "python3",
      "args": ["/path/to/lai/scripts/mcp_server.py"]
    }
  }
}
```

---

## System One (cliente de decisiones)

**Archivo:** `scripts/system_one_client.py`

Cliente para el endpoint `/v1/systemone` que soporta tres tipos de decisión:

| Tipo | Descripción |
|------|-------------|
| `choice` | Selección de una opción entre varias |
| `noul` | Decisión binaria (sí/no) |
| `score` | Puntuación en una escala |

### Uso

```bash
lai system-one --action decide --type choice --state init \
  --question "¿Qué modelo usar?" --options "qwen3,llama3" --scale 10
```

**Estado actual:** No hay servidor `/v1/systemone` corriendo localmente. El comando `lai system-one --help` funciona pero la ejecución end-to-end no está validada.

---

## Stress testing

**Archivo:** `scripts/stress_test.py`

10 suites de prueba de estrés:

| Suite | Descripción |
|-------|-------------|
| `concurrency` | Pruebas de concurrencia (c=1,4,8) |
| `stamina` | Pruebas de resistencia prolongada |
| `fault_injection` | Inyección de fallos |
| `large_context` | Contexto grande |
| `streaming_ttft` | Time-to-first-token en streaming |
| `security_key_flood` | Flood de claves de seguridad |
| `circuit_breaker` | Rompe-circuitos |
| ... | 4 suites más |

### Uso

```bash
python3 scripts/stress_test.py --suite concurrency --levels 1,4,8 --total 12 --trials 1
```

Genera un reporte JSON con latencia, TTFT, p50, p99, p95, máximo, mínimo.

---

## CI local 100% open source

**Archivo:** `scripts/ci_local.sh`

Pipeline de CI completo que no requiere:
- Cuentas de GitHub/GitLab/Bitbucket
- API keys
- Conexión a internet
- Servicios de cloud

### Targets de Makefile

| Target | Descripción |
|--------|-------------|
| `make ci` | CI completo (obligatorios + opcionales informativos) |
| `make ci-quick` | Solo checks obligatorios (compile, lint, test) |
| `make check` | Compile + ruff + pytest |
| `make quality` | Gates obligatorios + opcionales (format, type, coverage, security, complexity, dead code) |
| `make lint` | ruff check |
| `make format` | ruff check --fix + black |
| `make type` | mypy |
| `make test` | pytest |

### Estructura del pipeline CI

```
1. Compile:  py_compile lai.py           [OBLIGATORIO]
2. Lint:     ruff check lai.py tests/    [OBLIGATORIO]
3. Test:     pytest tests/               [OBLIGATORIO]
4. Format:   ruff format --check         [OPCIONAL, informativo]
5. Format:   black --check               [OPCIONAL, informativo]
6. Type:     mypy lai.py tests/          [OPCIONAL, informativo]
7. Coverage: pytest --cov=lai            [OPCIONAL, informativo]
8. Security: bandit -c .bandit lai.py    [OPCIONAL, informativo]
9. Complexity: radon cc                  [OPCIONAL, informativo]
10. Dead code: vulture                   [OPCIONAL, informativo]
11. Stress:  stress_test.py              [OPCIONAL, si gateway arriba]
```

Los checks opcionales nunca bloquean el CI (solo son informativos). El código de salida solo considera fallos obligatorios.

### Pre-commit (`.pre-commit-config.yaml`)

Hooks 100% locales (`language: system`), sin repos remotos de GitHub:

- ruff, ruff-format, black, mypy, bandit, radon-cc, vulture

Instalación: `pre-commit install`

---

## Calidad de código

### Estándares

- Python 3.10+, indentación 4 espacios, líneas máx 100 caracteres
- Ruff para linting (ver `pyproject.toml`)
- Type hints encouraged pero no enforzados everywhere
- Docstrings para funciones públicas
- **No agregar comentarios a menos que se pida** (regla de estilo del agente)

### mypy (`pyproject.toml`)

Configurado para ser permisivo en `lai.py` (archivo grande y dinámico):

```toml
[tool.mypy]
warn_return_any = false
warn_unused_configs = true
disallow_untyped_defs = false
ignore_missing_imports = true
no_implicit_optional = false
strict = false
disable_error_code = ["no-any-return", "arg-type", "call-overload", ...]
```

### bandit (`.bandit`)

Checks skippeados (falsos positivos/intencionales en herramienta de desarrollo):

- B104: hardcoded bind all interfaces (0.0.0.0 en LOCAL_HOSTS es intencional)
- B307: eval (uso intencional de `eval()` para ejecución de código Python aislado)
- B602: subprocess con shell=True (comando arbitrario del usuario, intencional, marcado con `# nosec B602`)
- B603, B607: subprocess sin shell / path parcial (herramientas de desarrollo que llaman a ruff/black/mypy/etc.)
- B108: hardcoded /tmp (uso intencional de bwrap con /tmp bind)
- B110: try-except-pass (patrón común en herramientas de reparación)
- B404: import subprocess (necesario para ejecución de comandos)

---

## Testing y cobertura

### Ejecutar tests

```bash
make test           # pytest tests/ -v
make ci-quick       # compile + lint + test
pytest tests/ -v    # directamente
```

### Cobertura actual

| Métrica | Valor |
|---------|-------|
| Total statements | 2198 |
| Cubiertos | ~613 |
| Cobertura | ~28% |
| Target | 35%+ |

**Cómo subir la cobertura:** Escribir más tests con mocks para las funciones de `lai.py` que aún no están cubiertas (ver lista de funciones no cubiertas en el reporte de `--cov-report=term-missing`).

### Archivos de test

- `tests/test_lai.py` - Tests originales
- `tests/test_lai_extra.py` - Tests adicionales con mocks (87 tests totales)

---

## Reglas de seguridad

1. **Loopback only:** `serve` bind a `127.0.0.1:8082`; gateway bind a `127.0.0.1:8081`
2. **is_local_url():** Valida endpoints antes de cualquier generación (Regla 4)
3. **No secrets:** Nunca exponer o loggear secrets/keys
4. **No cloud:** Todo por defecto es local; cuentas en la nube solo para configurar lai
5. **Gateway auth:** `Bearer open` - siempre válido, inseguro para producción (solo loopback)
6. **AutoMode:** Solo en repositorios de confianza

---

## Variables de entorno

| Variable | Descripción | Default |
|----------|-------------|---------|
| `LAI_LLAMA_MODEL` | Path al modelo .gguf para llama.cpp | (requerido para `lai start`) |
| `LAI_CONTEXT` | Contexto del modelo | 32768 |
| `LAI_NGL` | GPU layers | 99 |
| `EDITOR` | Editor para `lai config` | vim |

---

## Estructura de archivos

```
/home/kubernetes/curso-ia/modelo-local/
├── lai.py                    # Punto de entrada único (~4581 líneas)
├── pyproject.toml            # Configuración del proyecto, entry point
├── Makefile                  # Targets de build, test, quality, ci
├── VERSION                   # Versión (0.0.2)
├── .pre-commit-config.yaml   # Hooks pre-commit 100% locales
├── .bandit                   # Configuración de bandit (skips)
├── scripts/
│   ├── ci_local.sh           # Pipeline CI 100% local
│   ├── quality_gates.sh      # Gates de calidad obligatorios + opcionales
│   ├── mcp_server.py         # Servidor MCP stdio JSON-RPC 2.0 (23 tools)
│   ├── system_one_client.py  # Cliente de decisiones System One
│   └── stress_test.py        # 10 suites de prueba de estrés
├── tests/
│   ├── test_lai.py           # Tests originales
│   └── test_lai_extra.py     # Tests adicionales (mocks)
├── gateway-go/               # Go reverse proxy (puerto 8081)
├── DOCS/                     # Documentación consolidada
├── *.md                      # Documentación dispersa (14+ archivos)
└── AGENTS.md                 # Instrucciones para agentes de IA
```

---

## Roadmap y áreas débiles

| Área | Calificación (0-100) | Estado | Próximos pasos |
|------|---------------------|--------|----------------|
| Calidad de código | 95 | ✅ | Mantener |
| CI local | 95 | ✅ | Mantener |
| Pruebas unitarias | 80 | ⚠️ | Añadir más tests con mocks |
| Seguridad (bandit) | 85 | ✅ | Mantener .bandit actualizado |
| Cobertura de código | 28 | ❌ | Escribir tests para subir a 35%+ |
| Documentación | 60 | ⚠️ | Consolidar en DOCS_REFERENCE.md (este archivo) |
| Integración MCP | 70 | ⚠️ | Validar con cliente real (Claude Code, etc.) |
| System One | 40 | ❌ | Levantar servidor /v1/systemone local |
| OCR | 10 | ❌ | Definir alcance e implementar |
| Arquitectura | 65 | ⚠️ | Considerar split lai.py en módulos |

---

## Versionado

- Versión en `VERSION` y `pyproject.toml`
- `scripts/version.sh` para tagging (usar con precaución)
- Gateway versión en `gateway-go/config.yaml` y `main.go`
- Última versión: 0.0.2

---

## Desarrollo workflow

```bash
# Run quality checks
make check          # compile + ruff + pytest
make lint           # ruff only
make format         # ruff --fix + black
make type           # mypy
make test           # pytest

# Run lai
python3 lai.py status
python3 lai.py auto "tarea" --max-iterations 3
python3 lai.py serve --port 8082

# Build gateway
make gateway-build

# CI local
make ci             # Todo el pipeline
make ci-quick       # Solo obligatorios
```

---

## Notas importantes

- **Nunca hacer commit a menos que el usuario lo solicite explícitamente.**
- **No generar URLs a menos que sean para ayudar con programación.**
- **Todo el código generado por `lai` debe usar modelos locales (Regla 4).**
- **Los stubs en `~/.local/bin` pueden estar rotos; usar `python3 -m <herramienta>` en su lugar.**
