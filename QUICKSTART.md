# lai Quickstart — 5 minutos a tu primer agente local

> **lai** = Local AI Code Assistant. 100% local, 0€, sin cuentas, sin claves API.
> Todo corre en tu máquina: modelo (ollama/llama.cpp), gateway, tools, UI.

---

## ✅ Requisitos previos

| Componente | Mínimo | Recomendado |
|------------|--------|-------------|
| RAM        | 8 GB   | 16–32 GB    |
| GPU VRAM   | 4 GB   | 8–24 GB     |
| Disco      | 5 GB   | 20 GB       |
| OS         | Linux/macOS/WSL2 | Linux nativo |

> **GPU NVIDIA**: `nvidia-smi` debe mostrar tu VRAM.  
> **Apple Silicon**: memoria unificada cuenta como VRAM.  
> **Solo CPU**: funciona pero lento (usa modelo 1.5B–7B).

---

## ⚡ Instalación rápida (3 comandos)

```bash
# 1. Clona y compila
git clone https://github.com/jmbigi/lai.git
cd lai/lai_rust
cargo install --path . --locked

# 2. Instala ollama (gestor de modelos locales)
curl -fsSL https://ollama.com/install.sh | sh

# 3. ¡Wizard automático!
lai init --auto
```

**Eso es todo.** El wizard:
1. Detecta tu hardware (GPU, RAM, CPU)
2. Elige el mejor modelo para tu máquina
3. Lo descarga vía ollama
4. Configura gateway + lai
5. Crea `AGENTS.md` en tu proyecto
6. Verifica que todo funciona

---

## 🎯 Primer uso: modo autónomo

```bash
cd tu-proyecto
lai auto "añade tests unitarios para src/auth.rs"
```

Verás al agente:
- Leer el código existente
- Planear los tests
- Escribirlos
- Ejecutarlos (`cargo test` / `pytest`)
- Iterar hasta que pasen

---

## 🖥 Interfaz de terminal (TUI)

```bash
lai tui
```

**4 paneles, atajos globales:**

| Panel | Atajo | Qué hace |
|-------|-------|----------|
| 📁 Archivos | `Ctrl+1` | Árbol navegante, expande/colapsa, abre archivos |
| 💬 Chat | `Ctrl+2` | Conversación con el modelo, `i` para escribir |
| 📝 Diff | `Ctrl+3` | Ver cambios propuestos (+/-/contexto) |
| 🔧 Tools | `Ctrl+4` | Log de llamadas a herramientas (éxito/fallo) |

**Otros:** `Ctrl+H` = ayuda · `Ctrl+C` = salir

---

## 🔌 MCP Server — usa lai desde Cursor/Cline/Claude Code

**Opción A: stdio (Claude Code)**
```bash
# En tu config de Claude Code:
"mcpServers": {
  "lai": { "command": "lai", "args": ["mcp", "--stdio"] }
}
```

**Opción B: TCP (Cursor, Cline, etc.)**
```bash
# Terminal 1
lai mcp --port 8083

# En Cursor/Cline settings:
"mcpServers": {
  "lai": { "url": "http://127.0.0.1:8083" }
}
```

Ahora tienes **todas las tools de lai** (read_file, write_file, run_command, grep, git, etc.) disponibles en tu editor.

---

## 📊 Benchmark local (SWE-bench style)

```bash
# Tests unitarios del framework
cargo test --test swe_bench

# Benchmark real contra tu modelo (requiere ollama corriendo)
# Ejecuta 10 tareas de dificultad Easy→Expert
cargo test --test swe_bench -- --nocapture 2>&1 | tail -30
```

Compara modelos guardando resultados:
```bash
# Los resultados se guardan como benchmark_<modelo>_<timestamp>.json
# Para comparar dos modelos:
cargo run --example compare_bench -- benchmark_qwen3-coder-7b_*.json benchmark_gemma4-9b_*.json
```

---

## 🛠 Comandos principales

| Comando | Qué hace |
|---------|----------|
| `lai init [--auto]` | Wizard de configuración inicial |
| `lai doctor` | Diagnóstico de instalación |
| `lai auto "tarea"` | Agente autónomo completo |
| `lai chat` | Chat interactivo con tools |
| `lai review .` | Revisa código del proyecto |
| `lai status` | Estado de servicios (ollama, gateway, lai serve) |
| `lai tui` | Interfaz visual de terminal |
| `lai mcp [--stdio/--port]` | Servidor MCP para editores |
| `lai bench-swe --model X` | Benchmark coding tasks |

---

## 📁 Estructura de un proyecto con lai

```
tu-proyecto/
├── AGENTS.md          # ← lai lo lee automáticamente (objetivos, reglas, stack)
├── .lai/              # ← memoria, checkpoints, logs (gitignoreado)
├── src/
│   └── ...
└── Cargo.toml / pyproject.toml / package.json
```

**AGENTS.md** es el contexto vivo del proyecto. lai lo parsea y extrae:
- Objetivos y prioridades
- Stack tecnológico
- Patrones de código detectados
- Roles implicados

---

## 🔧 Configuración avanzada

Archivos en `~/.config/lai/`:
- `config.yaml` — modelo, URLs, límites, tools permitidas
- `gateway.yaml` — cache semántico, circuit breaker, upstreams

Ejemplo `config.yaml`:
```yaml
model: "qwen3-coder:30b"
base_url: "http://127.0.0.1:8081/v1"
allow_remote: false

auto_mode:
  max_iterations: 20
  max_cost_usd: 0.0
  require_approval: false
```

---

## 🐛 Troubleshooting rápido

| Problema | Solución |
|----------|----------|
| `ollama: connection refused` | `ollama serve` en otra terminal |
| `GPU out of memory` | `lai init` → elige modelo menor (7B→1.5B) |
| `lai: command not found` | `export PATH="$HOME/.cargo/bin:$PATH"` |
| `permission denied` en tools | Revisa `allowed_paths` en config.yaml |
| Modelo no responde | `lai doctor` → verifica puertos 8080/8081/8082/11434 |

---

## 🎓 Siguientes pasos

1. **Lee `AGENTS.md`** de tu proyecto — lai lo usa como contexto
2. **Prueba `lai tui`** — la forma más rápida de ver qué hace el agente
3. **Conecta tu editor** via MCP — tools de lai en Cursor/Cline/Claude Code
4. **Ejecuta benchmark** — conoce los límites reales de tu modelo local
5. **Contribuye** — PRs bienvenidos en GitHub

---

## 📜 Licencia y filosofía

**MIT / Apache-2.0** — Código abierto, para siempre.

> **Regla 0**: lai existe para impulsar IAs locales, abiertas y soberanas.  
> Sin claves API, sin cuentas cloud, sin telemetría externa.  
> Tu código y tus datos **nunca salen de tu máquina**.

---

**¿Listo?** `lai init --auto` y a programar con tu agente local. 🚀