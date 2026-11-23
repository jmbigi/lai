# Arquitectura Técnica de lai — Especificación de Implementación

**Estado:** Baseline (deriva de la revisión estratégica "10 mejoras de alto impacto")
**Implementación objetivo:** Rust (`lai_rust/`) — Python (`lai.py`, `lai_tools.py`, `lai_automode.py`) queda como legacy en mantenimiento.
**Principio rector (Regla 0):** ninguna dependencia obligatoria de cloud, API keys, cuentas externas o software propietario. Todo el harness debe funcionar con modelos locales servidos por llama.cpp/Ollama.

---

## 1. Objetivo y alcance

Convertir `lai` desde un asistente conversacional basado en LLM hacia un **Software Engineering Harness**: una infraestructura de ejecución determinista, extensible, verificable y segura que permite aprovechar diferentes LLM locales sin delegarles el control del sistema.

La hipótesis arquitectónica central es que **una mejora sustancial del harness puede incrementar la eficacia de un mismo modelo** sin sustituirlo por otro más grande o costoso. Esta hipótesis debe comprobarse experimentalmente (sección 12), no asumirse.

Esta especificación define:

- La arquitectura de módulos objetivo y su relación con el código existente.
- Los contratos concretos (firmas Rust) de los módulos nuevos y los refactorizados.
- El modelo de datos del runtime (TaskSpec, PlanStep, Checkpoint, VerificationResult…).
- La estrategia de verificación, memoria, contexto, políticas y benchmarks.
- El plan de implementación por fases con criterios de aceptación verificables.

**Fuera de alcance (en esta versión):** auto-mejora automática del núcleo, aprendizaje por refuerzo, adopción obligatoria de frameworks externos (LangGraph), memoria vectorial obligatoria.

---

## 2. Línea base: auditoría del estado actual

### 2.1 Inventario de módulos (`lai_rust/src/`)

| Módulo | Estado | Rol actual |
|---|---|---|
| `main.rs` | OK (CLI) | 40+ subcomandos vía clap |
| `llm.rs` | OK | Cliente HTTP OpenAI-compatible, `is_local_url()` (Regla 4), streaming, tool calls |
| `tools.rs` | OK | `ToolRegistry` con ~50 herramientas, `call_tool()`, esquemas JSON |
| `harness.rs` | **Incompleto** | ExecutionLoop con estados INTAKE→REPORT, checkpoints, RecoveryManager — pero no compila contra el resto |
| `policy.rs` | **Roto** | PolicyEngine con allowlist, protección de rutas, detección de secretos — errores de sintaxis y API desalineada con harness |
| `repomap.rs` | **Falta** | Declarado en `main.rs:21` y usado por `harness.rs`, sin archivo |
| `context.rs` | **Falta** | Declarado en `main.rs:22` y usado por `harness.rs`, sin archivo |
| `quality.rs` | OK (1710 líneas) | `Verifier`, `QualityReport`, checks estáticos, análisis de pytest |
| `telemetry.rs` | OK | Spans, eventos, `record_tool_call/llm_call/repair_cycle/checkpoint/policy_decision` |
| `completion.rs` | OK | `ProjectIndex` con parsing `syn` (Rust) y heurístico (Python), símbolos, goto, `cmd_repomap` |
| `rag.rs` | OK (básico) | RAG local con indexación por directorio, búsqueda híbrida |
| `session.rs` | OK | Sesiones, historial, trim por tokens |
| `agents.rs` | OK (básico) | `MultiAgentOrchestrator` (critique/review/plan/execute/architect), memoria clave-valor |
| `automode.rs` | OK | Plan + ReAct + Reflection |
| `safety.rs` | OK | Rutas protegidas, allowlist de comandos, confirmaciones, retry |
| `deguro.rs` | OK | Descubrimiento y comparación de modelos locales/remotos |
| `config.rs` | OK | Config global/local, cost summary |
| `services.rs` | OK | Gestión de llama.cpp/gateway/ollama |

### 2.2 Estado del build (crítico)

`cargo check` falla actualmente con 7 errores. Bloqueantes conocidos:

1. **`repomap.rs` ausente** — `harness.rs` usa `RepoMap { files, summary(), find_files() }` y `FileInfo { path }`.
2. **`context.rs` ausente** — `harness.rs` usa `ContextEngine` con `new/build_repo_map/add_files/summary/build_context_for_step/current_context/snapshot` y el tipo `Context`.
3. **`policy.rs` — regex rotos (líneas 93-98):** las raw strings `r"...[\"']..."` terminan en el `\"` porque en raw strings la comilla no se escapa. Solución: usar `r#"..."#`.
4. **`policy.rs` — API desalineada con `harness.rs`:**
   - `harness.rs` llama `policy_engine.check(&Operation::ToolCall{..}, &task_id)` (sync) y espera `PolicyDecision { allowed, reason, policy }`.
   - `policy.rs` ofrece `async fn check(&self, tool_name: &str, args: &HashMap<String, Value>) -> Result<PolicyDecision>` con enum `Allow | AllowWithModification | Deny(String)`, y no define `Operation` ni `Permission`.
5. **`harness.rs` — imports de telemetry incompletos:** usa `record_tool_call`, `record_llm_call`, `record_repair_cycle` sin importarlos.
6. **`harness.rs` — `timeout(..).await??` sobre `llm.chat()`:** `chat()` devuelve `ChatResult` (no `Result`); hay que manejar el timeout y el campo `error` del resultado.
7. **`tools.rs` — falta `call()`:** `harness.rs` llama `tools.call(name, args: Value) -> Result<Value>`; solo existe `call_tool(name, HashMap) -> Result<String>`.
8. **`policy.rs` — falta `Default`:** `HarnessConfig::default()` (harness.rs:179) usa `PolicyEngine::default()`.

**Nota de diseño:** el bug más grave detectado en `harness.rs:471` es que `auto_approve` (modo YOLO) permitiría ejecutar llamadas **denegadas** por el PolicyEngine (`if !decision.allowed && !self.config.auto_approve`). La especificación corregida (sección 8) establece que **Deny es incondicional**; `auto_approve` solo convierte `Confirm` en ejecución.

**Conclusión de la auditoría:** la Fase 0 no es opcional; el resto del plan se construye sobre un build verde.

---

## 3. Principios de diseño

1. **Determinismo donde importa.** Las decisiones del LLM son propuestas; el runtime decide si son permitidas, las ejecuta y verifica el resultado. Ningún prompt es una frontera de seguridad.
2. **Mínimo privilegio por herramienta.** Cada herramienta recibe solo los permisos necesarios; todas las operaciones pasan por el PolicyEngine (sección 8).
3. **Verificación externa obligatoria.** Una tarea no se declara completada sin verificaciones ejecutadas y superadas (sección 6).
4. **Contexto presupuestado.** El contexto se compone por prioridad y se limita por tokens; nunca se envía el repositorio completo (sección 7).
5. **Sin dependencias obligatorias de terceros.** SQLite+FTS5 para memoria estructurada; Chroma/vectorial es una capa opcional; LangGraph es referencia, no dependencia; MCP es adaptador, no núcleo.
6. **Recuperabilidad.** Toda modificación es transaccional: checkpoint antes/después, rollback posible.
7. **Evaluación continua.** Ningún cambio arquitectónico se adopta sin benchmark comparable (sección 12).
8. **Regla 0 ( sovereignty).** Sin cloud, API keys ni cuentas externas obligatorias.

---

## 4. Arquitectura de módulos objetivo

```
┌─────────────────────────────────────────────────────────────────────┐
│  INTERFACES: CLI (main.rs) · MCP adapters (mcp.rs) · HTTP API (serve)│
├─────────────────────────────────────────────────────────────────────┤
│  ORQUESTACIÓN                                                        │
│   harness.rs        ExecutionLoop (INTAKE→…→REPORT) + Recovery      │
│   planner.rs        Generación/validación de PlanStep (DAG)         │
│   spec.rs           Spec-Driven Development (TaskSpec ↔ SPEC.md)    │
│   verifier.rs       TestPipelineVerifier + baseline + regresiones    │
├─────────────────────────────────────────────────────────────────────┤
│  CONTEXTO Y CONOCIMIENTO                                             │
│   repomap.rs        Índice estructural (AST, símbolos, imports)     │
│   context.rs        ContextEngine: selección y presupuesto           │
│   memory.rs         Memoria persistente (SQLite+FTS5) + destilación  │
│   rag.rs            Búsqueda semántica (capa opcional)              │
├─────────────────────────────────────────────────────────────────────┤
│  GOBERNANZA                                                          │
│   policy.rs         PolicyEngine (guardrails deterministas)          │
│   safety.rs         Sandbox (bwrap), rutas protegidas, confirmación │
│   telemetry.rs      Observabilidad, trazas, trayectorias            │
├─────────────────────────────────────────────────────────────────────┤
│  CAPACIDADES BASE                                                    │
│   llm.rs (cliente) · tools.rs (ToolRegistry) · session.rs · config.rs│
│   quality.rs (checks/verificadores) · services.rs · agents.rs · …   │
└─────────────────────────────────────────────────────────────────────┘
```

Flujo de dependencias (sin ciclos entre capas): Interfaces → Orquestación → Contexto/Conocimiento → Gobernanza → Capacidades base. `harness.rs` consume `policy/context/repomap/tools/llm/telemetry`; `policy.rs` no depende de orquestación.

**Módulos nuevos:** `repomap.rs`, `context.rs`, `memory.rs`, `planner.rs`, `spec.rs`, `verifier.rs`, `bench.rs`, `mcp.rs`, `trajectory.rs` (parte de telemetry).
**Módulos refactorizados:** `policy.rs` (contrato), `tools.rs` (`call()`), `harness.rs` (gestión de resultados), `agents.rs` (perfiles/DAG), `commands.rs` (enrutar `auto`/`fix`/`code` por el harness).

---

## 5. Contratos de los módulos bloqueantes

Esta sección define las firmas exactas necesarias para restaurar el build y estabilizar el núcleo. Todo tipo público lleva `Serialize/Deserialize` cuando se persiste.

### 5.1 `repomap.rs` (nuevo)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,           // relativo al root
    pub language: String,       // "python" | "rust" | "go" | "other"
    pub size_bytes: u64,
    pub symbols: Vec<SymbolInfo>,
    pub imports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,           // "function" | "struct" | "class" | "method" | "import"
    pub line: usize,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoMap {
    pub root: PathBuf,
    pub files: Vec<FileInfo>,
    pub edges: Vec<(String, String)>, // (importador, importado)
}

impl RepoMap {
    /// Resumen compacto para prompts (archivos, nº símbolos, hotspots).
    pub fn summary(&self) -> String;
    /// Coincidencia por ruta o patrón (acepta "scope" del TaskSpec).
    pub fn find_files(&self, pattern: &str) -> Vec<String>;
    /// Símbolos por nombre (para LOCATE).
    pub fn find_symbols(&self, name: &str) -> Vec<&SymbolInfo>;
}

/// Construye el mapa desde `root`. Reutiliza `completion::ProjectIndex`
/// (parsing syn para Rust, heurístico para Python).
pub fn build_repo_map(root: &Path) -> Result<RepoMap>;
```

Notas: `completion.rs` ya implementa indexación de símbolos; `repomap.rs` extrae y generaliza esa lógica. `cmd_repomap` (CLI) se reimplementa sobre este módulo.

### 5.2 `context.rs` (nuevo)

```rust
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub files: Vec<String>,     // rutas incluidas
    pub notes: Vec<String>,     // hallazgos del INSPECT
    pub budget_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct ContextEngine {
    inner: Arc<ContextInner>,   // debe ser Clone: ExecutionContext lo clona
}

impl ContextEngine {
    pub fn new(working_dir: PathBuf) -> Result<Self>;

    /// Construye/actualiza el RepoMap del working_dir.
    pub async fn build_repo_map(&self, dir: &Path) -> Result<RepoMap>;

    /// Añade archivos relevantes al contexto de trabajo.
    pub async fn add_files(&self, files: &[String]) -> Result<()>;

    /// Resumen del contexto actual para prompts de planificación.
    pub fn summary(&self) -> String;

    /// Contexto para un paso del plan (objetivo, archivos, dependencias).
    pub async fn build_context_for_step(&self, ctx: &ExecutionContext, step: &PlanStep) -> Result<String>;

    /// Contexto actual completo (para verificación/repair).
    pub async fn current_context(&self) -> Result<String>;

    /// Snapshot serializable (para checkpoints).
    pub async fn snapshot(&self) -> Result<serde_json::Value>;
}
```

Nota de acoplamiento: `build_context_for_step` referencia `crate::harness::{ExecutionContext, PlanStep}`; Rust permite referencias circulares entre módulos del mismo crate. Alternativa si se quiere desacoplar: recibir `&TaskSpec` — en ese caso se ajusta la llamada en `harness.rs`.

### 5.3 `policy.rs` (refactor de contrato)

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    ToolCall { tool: String, args: serde_json::Value },
    WriteFile { path: String, content: String },
    RunCommand { command: String, cwd: Option<String> },
    Git { action: String, args: serde_json::Value },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission { Allow, Deny, Confirm }

#[derive(Debug, Clone)]
pub struct PolicyDecision {
    pub allowed: bool,     // true solo si Allow o Confirm-aprobado
    pub reason: String,
    pub policy: String,    // id de la regla que decidió (para telemetría)
}

impl Default for PolicyEngine { /* config por defecto */ }

impl PolicyEngine {
    /// Punto único de autorización. Síncrono (std::sync::Mutex interno).
    pub fn check(&self, op: &Operation, task_id: &str) -> PolicyDecision;
    /// Confirmación interactiva (usa safety::confirm_dangerous_operation).
    pub fn confirm(&self, decision: &PolicyDecision) -> bool;
}
```

La lógica interna existente se conserva: presupuestos de sesión, frontera de workspace, rutas protegidas, allowlist de comandos, bloqueo de egress, detección de secretos y bucles. Se corrigen los regex con `r#"..."#`.

### 5.4 `tools.rs` (añadir wrapper)

```rust
impl ToolRegistry {
    /// Versión JSON para el harness: Value -> HashMap -> call_tool -> Value.
    pub async fn call(&self, name: &str, args: serde_json::Value) -> Result<serde_json::Value>;
}
```

`call_tool` se mantiene para MCP y clientes existentes.

### 5.5 `llm.rs` (ajuste menor)

`harness.rs` debe tratar `ChatResult` como resultado, no como `Result`:

```rust
let result = timeout(self.config.llm_timeout, self.llm.chat(&prompt, false, true)).await?;
if let Some(err) = result.error { return Err(anyhow!("LLM error: {err}")); }
```

---

## 6. Modelo de datos del runtime

```rust
// harness.rs (existente, se mantiene)
pub enum ExecutionState { Intake, Inspect, Plan, Locate, Edit, Verify, Repair, Report, Completed, Failed, Cancelled }

pub struct TaskSpec {                 // SPEC.md ↔ JSON (sección 9)
    pub id: String, pub task_type: String,
    pub objective: String, pub current_state: String, pub problem: Option<String>,
    pub scope: Vec<String>, pub constraints: Vec<String>,
    pub affected_files: Vec<String>, pub dependencies: Vec<String>,
    pub acceptance_criteria: Vec<String>, pub required_tests: Vec<String>,
    pub risks: Vec<String>, pub created_at: String, pub updated_at: String,
}

pub struct PlanStep { /* id, phase, description, tool, arguments, expected_outcome, dependencies, completed, result, error */ }

pub struct Checkpoint { /* id, task_id, timestamp, state, step_index, description, files_modified, context_snapshot, plan_snapshot */ }

pub struct VerificationResult { pub verifier: String, pub passed: bool, pub details: String, pub artifacts: Vec<String> }
```

Añadidos:

```rust
pub struct Budget {                   // límites explícitos por tarea
    pub max_iterations: usize,        // default 10
    pub max_tool_turns: usize,        // default 20
    pub max_wall_secs: u64,           // default 3600
    pub max_tokens: u64,
    pub max_usd_cents: u64,
}

pub struct Trajectory {               // auto-mejora (sección 13)
    pub task_id: String, pub model: String, pub harness_version: String,
    pub spec: TaskSpec, pub plan: Vec<PlanStep>,
    pub actions: Vec<ActionRecord>,   // herramienta, args hash, resultado, duración
    pub verifications: Vec<VerificationResult>,
    pub outcome: Outcome,             // Completed | Failed | Cancelled | Timeout
    pub cost_usd: f64, pub duration_ms: u64,
}
```

Persistencia de trayectorias: `telemetry.rs` → JSONL en `LAI_LOG_DIR` o `.lai/trajectories/`.

---

## 7. Context Engineering (context.rs)

Composición del contexto por prioridad (sección transversal del plan estratégico):

1. Solicitud actual del usuario.
2. TaskSpec (objetivo + criterios de aceptación).
3. Estado del RepoMap (resumen, archivos calientes).
4. Archivos directamente relacionados (del `find_relevant_files`).
5. Dependencias relevantes (imports del RepoMap).
6. Memoria persistente pertinente (sección 10).
7. Resultados de herramientas (truncados con referencia).

Reglas:

- Presupuesto por defecto: 60% del contexto del modelo (configurable en `config.rs`).
- Truncamiento explícito: cada sección declara su coste; se recorta desde el final.
- De-duplicación por hash de contenido.
- `summary()` produce texto compacto apto para prompts de planificación.

---

## 8. Policy Engine y aislamiento (gobernanza)

### 8.1 Superficies de control

| Superficie | Reglas |
|---|---|
| Archivos | Dentro del workspace; rutas protegidas (`.git/`, `lai_rust/src/`, credenciales); límite de escrituras por archivo; detección de secretos en contenido/parche |
| Comandos | Allowlist (`cargo`, `pytest`, `ruff`, `git`…); bloqueo de patrones destructivos (`rm -rf /`, `dd`, `mkfs`…); bloqueo de egress si `block_network_egress` |
| Red | Negociado por defecto para herramientas; `curl/wget/nc/ssh…` bloqueados salvo allowlist explícita |
| Ejecución | `Budget` (tokens, tiempo, operaciones); detección de bucles (misma herramienta+args N veces); bloqueo de escrituras repetidas |
| Git | `push --force` bloqueado; rutas git dentro del workspace |

### 8.2 Regla crítica de autorización

```
decisión = policy.check(op, task_id)
match decisión:
  Deny     → rechazo incondicional (ni YOLO ni auto lo saltan)
  Confirm  → auto_approve|yolo: ejecutar; si no: confirmación interactiva
  Allow    → ejecutar
```

`harness.rs:471` actual (`!decision.allowed && !auto_approve` → skip) **se corrige**: `auto_approve` jamás convierte un Deny en ejecución.

### 8.3 Aislamiento

- `run_command` y `run_python` se ejecutan bajo sandbox (bubblewrap) cuando está disponible (`safety.rs`), con fallback a allowlist estricta.
- Modos de ejecución: `safe` (solo lectura), `interactive` (confirma sensibles), `auto` (autonomía dentro de políticas), `ci` (reproducible, sin confirmaciones).
- El modo `auto` nunca desactiva el PolicyEngine.

---

## 9. Spec-Driven Development (spec.rs)

Flujo: `description → intake_phase (LLM) → TaskSpec validado → SPEC.md → aprobación (interactivo) → ejecución`.

- `spec.rs` define `pub fn generate_spec(llm, description, task_type) -> Result<TaskSpec>` y `pub fn spec_to_markdown(&TaskSpec) -> String`.
- Validación estructural del JSON del LLM: campos obligatorios, tipos, no vacío; reintento con corrección (máx. 2).
- Persistencia: `.lai/specs/<task_id>.md` (fuente legible) + JSON embebido.
- El TaskSpec es el contrato de aceptación: el Verifier comprueba cada `acceptance_criteria` de forma independiente.
- Confirmación proporcional al riesgo: cambios destructivos o fuera de scope requieren aprobación explícita; lectura/análisis no.

---

## 10. Verificación independiente (verifier.rs)

`verifier.rs` envuelve `quality.rs` (checks estáticos, análisis de pytest) y añade pipeline con línea base:

```
EDIT → VERIFY → DIAGNOSE → REPAIR → VERIFY
```

### 10.1 Pipeline

1. Sintaxis (py_compile / rustc --emit=metadata).
2. Análisis estático (ruff / clippy según lenguaje).
3. Type checking (mypy / cargo check).
4. Tests unitarios + integración.
5. Regresión: comparar con baseline (resultados de tests previamente guardados).
6. Criterios de aceptación del TaskSpec (verificación independiente por criterio).
7. Efectos secundarios: diff de archivos modificados vs scope declarado.

### 10.2 Contrato

```rust
pub struct Baseline { pub task_key: String, pub tests: HashMap<String, TestOutcome>, pub saved_at: String }

pub struct TestPipelineVerifier { /* inyecta: llm (criterios), config, working_dir */ }

impl TestPipelineVerifier {
    pub fn new(...) -> Self;
    pub async fn verify(&self, ctx: &ExecutionContext) -> Result<Vec<VerificationResult>>;
    pub fn save_baseline(&self, baseline: &Baseline) -> Result<()>;
    pub fn compare_with_baseline(&self, current: &HashMap<String, TestOutcome>) -> Vec<Regression>;
}
```

### 10.3 Clasificación de fallos

El verificador distingue: error de código · error de entorno · dependencia ausente · test defectuoso · verificación no ejecutada · resultado inconcluso. Solo el primero activa el ciclo de reparación automática.

### 10.4 SBFL (opcional, fase posterior)

Si hay cobertura de ejecución suficiente, Ochiai/Tarantula para localizar fallos. Se implementa como refinamiento del diagnóstico, no como requisito.

---

## 11. Memoria persistente (memory.rs)

### 11.1 Tipos de memoria

| Tipo | Contenido | Almacenamiento |
|---|---|---|
| Semántica | Arquitectura, convenciones, decisiones, dependencias | SQLite tabla `memories` + FTS5 |
| Episódica | Tareas ejecutadas, errores, soluciones, resultados | Tabla `episodes` + trayectorias JSONL |
| De trabajo | Objetivo, plan, estado, archivos del contexto | `context.rs` (en memoria) |
| Documental | `PROJECT.md`, `ARCHITECTURE.md`, `CONVENTIONS.md`, `DECISIONS.md`, `KNOWN_ISSUES.md` | Archivos del repo (fuente de verdad) |

### 11.2 Esquema mínimo (rusqlite + FTS5, sin dependencias externas)

```sql
CREATE TABLE memories (
  id INTEGER PRIMARY KEY, kind TEXT, project TEXT,
  created_at TEXT, source TEXT, confidence REAL,
  version INTEGER, valid_until TEXT,
  content TEXT, metadata TEXT
);
CREATE VIRTUAL TABLE memories_fts USING fts5(content, content='memories', content_rowid='id');
```

### 11.3 API

```rust
pub struct MemoryEntry { /* id, kind, project, source, confidence, content, metadata */ }
pub struct Distillation { /* extrae candidatos de una Trajectory */ }

pub struct MemoryStore { /* path al .lai/memory.db */ }

impl MemoryStore {
    pub fn open(path: &Path) -> Result<Self>;
    pub fn remember(&self, entry: MemoryEntry) -> Result<()>;
    pub fn recall(&self, query: &str, project: Option<&str>, k: usize) -> Result<Vec<MemoryEntry>>; // FTS5
    pub fn distill(&self, trajectory: &Trajectory) -> Result<Vec<MemoryEntry>>; // destilación post-tarea
}
```

### 11.4 Destilación

Tras completar una tarea, un proceso (LLM local, prompt dedicado) identifica conocimiento persistente: decisiones, convenciones, errores recurrentes. Cada entrada guarda origen, fecha, proyecto, evidencia (task_id), confianza y vigencia. **No se indexan todas las conversaciones**; solo conocimiento destilado. La búsqueda combina FTS5 (siempre) y `rag.rs` (opcional, cuando aporte valor medible).

---

## 12. Benchmarks y evaluación continua (bench.rs)

### 12.1 Principio

Los benchmarks empiezan en la Fase 1 (junto al harness), no al final. Sin línea base no se puede validar ninguna decisión arquitectónica. Conjuntos de prueba separados de los datos de ajuste.

### 12.2 Suite

- **Interna (prioritaria):** `benchmarks/tasks/*.yaml` con tareas representativas (comprender repo desconocido, localizar error, corregir regresión, implementar feature, refactor, tipado, rendimiento, dependencias, vulnerabilidades, multi-archivo). Cada tarea: descripción, repo fixture, criterios verificables, tests de referencia.
- **Externa (referencia):** SWE-bench Verified, Terminal-Bench — se documentan pero no se comparan sus porcentajes entre sí.

### 12.3 Métricas obligatorias

| Dimensión | Métrica |
|---|---|
| Calidad | Tasa de tareas resueltas |
| Fiabilidad | Regresiones introducidas |
| Precisión | Falsos positivos |
| Autonomía | Tareas completadas sin intervención |
| Eficiencia | Tokens por tarea |
| Coste | USD por tarea |
| Rendimiento | Tiempo de ejecución |
| Estabilidad | Variabilidad entre ejecuciones |
| Seguridad | Operaciones no autorizadas bloqueadas |
| Recuperación | Recuperación ante fallos |

### 12.4 Comando

`lai bench [--suite internal|external] [--model M] [--config C] [--out results/]` → resultados versionados (JSON) comparables entre modelos, prompts, configuraciones, versiones del harness y estrategias de memoria.

---

## 13. Auto-mejora controlada (fase final)

Tres conceptos separados: **memoria** (almacenar experiencias) ≠ **adaptación** (ajustar estrategias) ≠ **aprendizaje** (mejora demostrable).

Procedimiento obligatorio antes de adoptar cualquier cambio de políticas:

1. Identificar oportunidad (análisis offline de trayectorias).
2. Formular hipótesis.
3. Proponer modificación (política, orden de herramientas, contexto…).
4. Ejecutar `lai bench` sobre la suite interna.
5. Comparar con la línea base.
6. Evaluar regresiones.
7. Aprobar/rechazar y registrar la decisión.

El agente **nunca** modifica su propio núcleo sin pasar este control. La implementación Rust parte de `trajectory.rs` (registro) + análisis offline; el equivalente Python actual (`lai_self_improve.py`) queda como referencia.

---

## 14. Estrategia de pruebas

| Nivel | Alcance | Herramienta |
|---|---|---|
| Unit | Cada módulo (policy, repomap, context, memory, verifier) | `cargo test` (`#[cfg(test)]`) |
| Contrato | harness ↔ policy ↔ tools ↔ llm (los errores actuales son de contrato) | Tests en `harness.rs` con PolicyEngine real y ToolRegistry stub |
| Integración | Ciclo completo con LLM simulado (servidor OpenAI-compatible de juguete) | `wiremock`/axum local + fixtures |
| Regresión | Suite `benchmarks/tasks/` en cada cambio del harness | `lai bench` + comparación contra baseline versionada |
| Aceptación | Criterios de la sección 15 | Script `scripts/acceptance.sh` |

Todo test debe ser determinista y offline (Regla 0): el LLM se simula; nunca se llama a la nube.

---

## 15. Plan de implementación por fases

| Fase | Componentes | Prioridad | Dependencias |
|---|---|---|---|
| **0** | Restaurar build verde: `repomap.rs` + `context.rs` mínimos; fix regex `policy.rs`; contrato `PolicyEngine` (Operation/Permission/PolicyDecision/Default); `tools.rs::call`; imports y `ChatResult` en `harness.rs`; corregir bug `auto_approve` (Deny incondicional) | **Inmediata** | — |
| 1 | Consolidar ExecutionLoop: `lai auto/fix/code` enrutados por harness; PolicyEngine en **todas** las operaciones; checkpoints + `lai recover --task`; **baseline de `lai bench`** con suite mínima | Inmediata | 0 |
| 2 | RepoMap completo (AST multi-lenguaje, edges); ContextEngine con presupuesto; auditoría del ToolRegistry | Inmediata | 1 |
| 3 | `verifier.rs` (TestPipelineVerifier + baseline + regresiones + clasificación de fallos); ciclo repair cerrado | Inmediata | 1 |
| 4 | Spec-Driven Development (`spec.rs`, SPEC.md, confirmación proporcional) | Alta | 1 |
| 5 | Memoria persistente (`memory.rs`, SQLite+FTS5, destilación); rag.rs opcional | Alta | 2 |
| 6 | Roles especializados como perfiles del mismo runtime + DAG de tareas (agents.rs refactor) | Media-alta | 1-4 |
| 7 | Revisión adversarial (`lai review --adversarial`, 5 etapas, filtrado de falsos positivos) | Media-alta | 2, 3 |
| 8 | MCP coherente: Core → Tool Registry → adaptadores MCP (`mcp.rs`); schemas/validación/límites por herramienta | Media | 2 |
| 9 | Benchmarks ampliados + comparativas versionadas (`bench.rs`, `lai bench`) | Alta | 1 |
| 10 | Auto-mejora controlada (trayectorias + análisis offline + adopción gateada) | Largo plazo | 9 |

Regla de oro: **cada fase termina con build verde, `cargo test` en verde y benchmark comparable**. Los benchmarks arrancan en la Fase 1.

---

## 16. Criterios de aceptación

| Área | Criterio verificable |
|---|---|
| Build | `cargo check && cargo test` en verde en `main` (门禁 de cada fase) |
| Ejecución | Toda operación de herramienta pasa por `PolicyEngine::check` (test de contrato) |
| Seguridad | Ninguna operación `Deny` se ejecuta ni en modo `--yolo`/`auto` (test dedicado) |
| Contexto | En tareas de prueba, el RepoMap identifica los componentes relevantes (≥ objetivo fijado tras baseline) |
| Planificación | Tareas complejas producen `PlanStep` con dependencias válidas (DAG sin ciclos) |
| Edición | Todo cambio queda registrado y es revertible (`lai recover`) |
| Verificación | Ninguna tarea se declara completada sin verificaciones externas superadas |
| Reparación | Fallos generan diagnóstico accionable clasificado (código/entorno/test) |
| Memoria | `recall` recupera conocimiento previo con origen y confianza trazables |
| Revisión | Cada defecto reportado incluye evidencia reproducible; falsos positivos < objetivo fijado tras baseline |
| Evaluación | Cada versión compara métricas contra baseline versionada |
| Auto-mejora | Ningún cambio de políticas se adopta sin superar `lai bench` sin regresiones |
| Regla 0 | Ningún test ni comando del harness requiere red externa (salvo modelos locales) |

Los umbrales numéricos se fijan **después** de medir la línea base real (Fase 1); fijarlos antes no sería riguroso.

---

## 17. Riesgos y decisiones abiertas

1. **Acoplamiento `harness.rs` ↔ `policy.rs`/`context.rs`:** los contratos de la sección 5 resuelven la compilación; si el ciclo `context → harness` resulta problemático, se desacopla pasando `&TaskSpec` en lugar de `&ExecutionContext`.
2. **`harness.rs` verifica todos los archivos del repo en cada iteración** (líneas 572-582): costoso e incorrecto si el repo crece. Decisión: verificar solo archivos modificados + tests; el barrido completo queda para `lai test`/`bench`.
3. **`rustc --emit=metadata` por archivo** no es type-check real de crate: sustituir por `cargo check` cuando el target sea el proyecto completo.
4. **Multiagente:** `agents.rs` existe pero el plan lo relega a perfiles dentro de un runtime; su valor debe demostrarse con benchmarks (Fase 6/7) antes de justificar procesos independientes.
5. **Memoria vectorial:** `rag.rs` y Chroma son opcionales; la decisión de activarlos depende del volumen real de conocimiento destilado, no de la moda.
6. **Contaminación de benchmarks externos:** se documentan versiones y condiciones; las decisiones se toman con la suite interna.
7. **Legacy Python:** `lai.py`/`lai_automode.py` se mantienen solo para paridad; toda evolución va a `lai_rust/`.

---

## 18. Referencias

- `DOCUMENTACION_REGLAS.md` — Regla 0, 4 y 5 (fuente de verdad de reglas).
- `AGENTS.md` — Arquitectura Rust y workflow de desarrollo.
- `DOCS/REVISION_2026.md` — Revisión técnica y research.
- Código: `lai_rust/src/harness.rs` (ExecutionLoop), `policy.rs` (guardrails), `quality.rs` (verificación), `completion.rs` (índice de símbolos), `telemetry.rs` (observabilidad).