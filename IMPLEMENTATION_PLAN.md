# Plan de Implementación: Arquitectura lai con 10 Herramientas Estratégicas

## Estado Actual
El proyecto `lai_rust` tiene una implementación mínima:
- `main.rs` - CLI entry point
- `commands.rs` - 3 comandos básicos (chat, code, status)
- `llm.rs` - Cliente HTTP para endpoints locales
- `services.rs` - Gestión de servicios (llama.cpp, gateway, ollama)

Faltan la mayoría de módulos descritos en AGENTS.md (tools, automode, agents, session, thinking, config, etc.)

---

## Arquitectura Objetivo (por adaptadores)

```
lai_rust/
├── Cargo.toml
└── src/
    ├── main.rs                    # CLI entry point
    ├── commands.rs                # 25+ comandos CLI
    ├── llm.rs                     # LLM client (HTTP, streaming, tool calling)
    ├── services.rs                # Service management
    ├── tools.rs                   # ToolRegistry con 50+ tools
    ├── automode.rs                # AutoMode: Plan + ReAct + Reflection
    ├── agents.rs                  # Multi-agent orchestrator
    ├── session.rs                 # Session management
    ├── thinking.rs                # Thinking extraction/display
    ├── config.rs                  # Configuration management
    │
    ├── core/
    │   ├── contracts.rs           # Contratos obligatorios (ResultStatus, Evidence, etc.)
    │   ├── execution.rs           # ExecutionEngine nativo
    │   ├── planner.rs             # Planificador nativo
    │   ├── policies.rs            # Policy Engine
    │   └── metrics.rs             # Métricas internas
    │
    ├── orchestration/
    │   ├── native/                # Motor de ejecución propio
    │   └── adapters/
    │       └── langgraph/         # Adapter LangGraph (Fase 9)
    │
    ├── planning/
    │   ├── dag.rs                 # DAG Engine nativo (Fase 7)
    │   ├── scheduling.rs          # Scheduler
    │   └── adapters/
    │       └── dagent/            # Adapter DAGent (Fase 7)
    │
    ├── memory/
    │   ├── episodic.rs
    │   ├── semantic.rs
    │   ├── temporal.rs
    │   └── adapters/
    │       ├── mem0/              # Adapter Mem0 (Fase 6)
    │       └── graphiti/          # Adapter Graphiti (Fase 6)
    │
    ├── retrieval/
    │   ├── fts5.rs                # FTS5/BM25 (existente en repomap)
    │   ├── embeddings.rs          # Embeddings locales
    │   ├── fusion.rs              # RRF
    │   ├── reranking.rs           # Cross-encoder opcional
    │   └── hybrid.rs              # Hybrid RAG (Fase 4)
    │
    ├── code_analysis/
    │   ├── ast.rs                 # AST parsing (Tree-sitter)
    │   ├── symbols.rs             # Symbol graph
    │   ├── cpg.rs                 # Code Property Graphs
    │   └── adapters/
    │       └── joern/             # Adapter Joern + codebadger (Fase 5)
    │
    ├── verification/
    │   ├── contracts.rs           # Contratos de verificación
    │   ├── formal.rs              # Dafny integration
    │   ├── model_checking.rs      # TLA+/TLC
    │   ├── neural.rs              # α,β-CROWN, Marabou, ERAN (Fase 10)
    │   └── result_classifier.rs   # Clasificador de resultados
    │
    ├── testing/
    │   ├── unit.rs
    │   ├── property.rs            # Hypothesis + fast-check (Fase 3)
    │   └── mutation.rs            # Mutation testing (Fase 3)
    │
    ├── security/
    │   ├── integrity.rs           # Verificación de integridad
    │   ├── sandbox.rs             # Sandbox (bubblewrap)
    │   ├── permissions.rs         # Sistema de permisos
    │   ├── policies.rs            # OPA/Rego policies
    │   └── mcp_gateway.rs         # MCP Hangar integration (Fase 1)
    │
    └── observability/
        ├── events.rs              # Event system
        ├── metrics.rs             # Metrics collection
        ├── audit.rs               # Audit log
        └── adapters/
            └── langfuse.rs        # Langfuse adapter (Fase 2)
```

---

## Fases de Implementación

### FASE 1: MCP Hangar + Integridad Propia (CRÍTICA)
**Objetivo**: Proteger la superficie de herramientas externas

**Entregables**:
1. `src/security/integrity.rs` - Verificación de integridad (hashes, firmas, procedencia)
2. `src/security/mcp_gateway.rs` - Integración MCP Hangar
3. `src/security/policies.rs` - Policy Engine con OPA/Rego
4. `src/core/contracts.rs` - Contratos obligatorios (ResultStatus, Evidence, CapabilityAdapter)
5. Tests de manipulación y controles de seguridad

**Dependencias nuevas**:
- `opa-wasm` o `rego` para policies
- `sha2`, `ring` para criptografía
- Cliente MCP Hangar (HTTP)

**Criterios de salida**:
- Integridad comprobada antes de cada ejecución de herramienta
- Discrepancias bloqueadas (no degradación silenciosa)
- Permisos explícitos por herramienta
- Auditoría completa inmutable
- Tests de manipulación pasan

---

### FASE 2: Langfuse (CRÍTICA)
**Objetivo**: Observabilidad y línea base de métricas

**Entregables**:
1. `src/observability/events.rs` - Sistema de eventos estructurados
2. `src/observability/metrics.rs` - Colección de métricas
3. `src/observability/audit.rs` - Log de auditoría inmutable
4. `src/observability/adapters/langfuse.rs` - Adapter Langfuse
5. Integración en ExecutionEngine, ToolRegistry, LLM client

**Campos obligatorios por evento**:
```
run_id, task_id, agent_id, tool_id, tool_definition_hash,
policy_decision, verification_level, verification_result,
tokens_input, tokens_output, latency_ms, exit_status
```

**Métricas fundamentales**:
- Coste por tarea
- Tokens por tarea
- Tasa de éxito real vs verificado
- Falsos positivos/negativos
- Errores por herramienta
- Fallos de seguridad
- Tiempo de recuperación
- Consumo de recursos

**Criterios de salida**:
- Todas las ejecuciones registradas desde el inicio
- Dashboard Langfuse funcional autoalojado
- Métricas base establecidas

---

### FASE 3: Hypothesis + Mutation Testing (MUY ALTA)
**Objetivo**: Aumentar capacidad de descubrir errores

**Entregables**:
1. `src/testing/property.rs` - Property-based testing con Hypothesis (Python) / proptest (Rust)
2. `src/testing/mutation.rs` - Mutation testing integration
3. Integración en CI para parsers, serializadores, normalizadores, validadores, permisos

**Aplicaciones prioritarias**:
1. Parsers (AST, config)
2. Serializadores
3. Normalizadores de rutas
4. Validadores
5. Gestión de estados
6. Sistemas de permisos
7. Cálculos y transformaciones

**Mutaciones objetivo**:
- `<=` → `<`
- Eliminación de validaciones
- Inversión de condiciones
- Alteración de límites
- Omisión de comprobaciones de permisos

**Criterios de salida**:
- Detectar mutaciones relevantes
- Reproducir contraejemplos minimizados
- Suite detecta >80% mutaciones críticas

---

### FASE 4: Hybrid RAG (MUY ALTA)
**Objetivo**: Mejorar recuperación de contexto y evidencia

**Entregables**:
1. `src/retrieval/embeddings.rs` - Embeddings locales (sentence-transformers via Python bridge o ONNX)
2. `src/retrieval/fusion.rs` - Reciprocal Rank Fusion (RRF)
3. `src/retrieval/reranking.rs` - Cross-encoder opcional
4. `src/retrieval/hybrid.rs` - Pipeline completo híbrido

**Arquitectura**:
```
QUERY → Normalization → [FTS5/BM25] + [Embeddings] → RRF → Candidate Ranking → Cross-Encoder (opt) → Relevance Threshold → Evidence Validation → CONTEXT
```

**Benchmark obligatorio**:
- Corpus: repositorio lai + proyectos test
- Consultas: técnicas, decisiones, bugs, dependencias
- Métricas: Recall@5, Recall@10, MRR@10, nDCG@10, Latency p50/p95
- Comparación: FTS5 vs FTS5+Emb vs FTS5+Emb+RRF vs Full Hybrid

**Criterios de salida**:
- Mejora estadísticamente defendible en benchmark propio
- Costes aceptables (latencia, memoria, tokens)

---

### FASE 5: Joern + codebadger (ALTA)
**Objetivo**: Análisis semántico profundo y flujo de datos

**Entregables**:
1. `src/code_analysis/cpg.rs` - Code Property Graph interface
2. `src/code_analysis/adapters/joern.rs` - Adapter Joern
3. Integración MCP codebadger
4. Queries CPG para: call graphs, data-flow, taint tracking, program slicing

**Regla fundamental**: NO sustituir FTS5, AST, symbol graph, índice incremental. Joern = capa adicional bajo demanda.

**Validación**: Frontends por lenguaje individualmente.

**Criterios de salida**:
- Análisis de flujo de datos funcional
- Detección de vulnerabilidades vía CPG
- Ejecución bajo demanda (no en cada operación)

---

### FASE 6: Memoria Persistente (ALTA)
**Objetivo**: Conservar conocimiento histórico verificable

**Entregables**:
1. `src/memory/episodic.rs` - Memoria episódica (conversaciones, ejecuciones, decisiones, errores)
2. `src/memory/semantic.rs` - Memoria semántica (arquitectura, deps, constraints, preferencias)
3. `src/memory/temporal.rs` - Memoria temporal (qué/cuándo/por qué cambió)
4. `src/memory/adapters/mem0.rs` - Adapter Mem0
5. `src/memory/adapters/graphiti.rs` - Adapter Graphiti (grafos temporales)

**Tres niveles**:
- Nivel 1: Episódica
- Nivel 2: Semántica
- Nivel 3: Temporal (Graphiti especializado)

**Criterio clave**: La fuente de verdad = repositorio + specs + registros verificables. Memoria = complemento.

**Benchmark propio**: Recuperación de decisiones técnicas, contratos, deps, errores, cambios históricos.

**Criterios de salida**:
- Recuperación correcta con procedencia
- Resolución de contradicciones
- Detección de información obsoleta
- Eliminación de recuerdos inválidos
- No adoptar ambos simultáneamente - comparar primero

---

### FASE 7: Planificación DAG (ALTA)
**Objetivo**: Planificación incremental basada en evidencia

**Entregables**:
1. `src/planning/dag.rs` - DAG Engine nativo
2. `src/planning/scheduling.rs` - Scheduler con paralelismo controlado
3. `src/planning/adapters/dagent.rs` - Adapter DAGent (referencia)

**Nodo DAG**:
```
id, objetivo, dependencias, entradas, salidas, estado, presupuesto, timeout, evidencias, criterio_aceptacion
```

**Reglas**:
1. No ejecutar sin dependencias satisfechas
2. No operaciones incompatibles simultáneas
3. No escrituras concurrentes sin aislamiento
4. No perder trazabilidad
5. No aceptar sin validación
6. No continuar si dependencia crítica falla
7. Replanificación con nueva evidencia
8. Registro reproducible

**Implementación**: Primero DAG nativo sencillo, luego comparar con DAGent.

**Criterios de salida**:
- Tareas complejas divididas en operaciones controladas
- Paralelismo seguro
- Replanificación funcional

---

### FASE 8: Verificación Formal (MEDIA-ALTA)
**Objetivo**: Contratos matemáticos en componentes críticos

**Entregables**:
1. `src/verification/formal.rs` - Dafny integration
2. `src/verification/model_checking.rs` - TLA+/TLC integration
3. `src/verification/contracts.rs` - Especificaciones y obligaciones de prueba

**Componentes críticos a verificar**:
- Gestión de permisos
- Máquina de estados
- Planificador
- Gestión de transacciones
- Serialización
- Integridad
- Sistemas de concurrencia

**Distribución responsabilidades**:
| Componente | Responsabilidad |
|---|---|
| LLM | Proponer specs y anotaciones |
| Dafny | Verificar obligaciones |
| Z3 | Resolver condiciones (backend) |
| TLA+ | Modelar protocolos/concurrencia |
| TLC | Explorar espacio estados |
| VerificationEngine | Interpretar resultados, aplicar contratos |

**Regla**: El modelo NUNCA declara prueba demostrada. Evidencia = verificador.

**Criterios de salida**:
- Módulos críticos con specs verificadas
- Pipeline LLM→Dafny→Z3 funcional
- Model checking TLA+ para concurrencia

---

### FASE 9: LangGraph (MEDIA)
**Objetivo**: Evaluar alternativa de orquestación

**Entregables**:
1. `src/orchestration/adapters/langgraph.rs` - Adapter LangGraph
2. Comparativa: tokens/tarea, latencia, transiciones, reintentos, recuperaciones

**Regla**: NO sustituir ExecutionEngine nativo inicialmente. Usar para:
1. Experimentar flujos complejos
2. Modelar interrupciones/recuperación
3. Comparar topologías
4. Evaluar estado persistente
5. Identificar patrones para motor propio

**Contrato**: LangGraph no modifica políticas, permisos, criterios, resultados de verificación, estados terminales.

**Criterios de salida**:
- Comparativa documentada con métricas
- Decisión: mantener nativo o migrar

---

### FASE 10: Verificación Neuronal (CONDICIONAL)
**Objetivo**: Verificar componentes neuronales cuando existan

**Entregables**:
1. `src/verification/neural.rs` - Interface unificada
2. Adapters: α,β-CROWN, Marabou, ERAN
3. Carga modelos compatibles, specs propiedades, límites tiempo/memoria, contraejemplos

**Condición**: Solo si lai tiene componentes neuronales propios que verificar.

**Herramientas**:
- α,β-CROWN: Propagación cotas + branch-and-bound (VNN-COMP winner)
- Marabou: SMT + constraints, contraejemplos concretos
- ERAN: Interpretación abstracta, robustez

**Criterios de salida**:
- Verificación propiedades redes neuronales
- Contraejemplos generados
- Integración en pipeline de verificación

---

## Contratos Obligatorios (Transversales)

### 5.1 Contrato de Ejecución (CapabilityAdapter)
```rust
trait CapabilityAdapter {
    fn preflight(&self, request: &Request) -> Result<PreflightResult>;
    async fn execute(&self, request: &Request) -> Result<ExecutionResult>;
    fn validate(&self, result: &ExecutionResult) -> ValidationResult;
    fn get_metrics(&self, execution_id: &str) -> Metrics;
    fn health_check(&self) -> HealthStatus;
}
```

### 5.2 Contrato de Resultados (ResultStatus)
```rust
enum ResultStatus {
    Verified { verifier_id, verifier_version, spec_hash, source_hash, proof_scope, timestamp, assumptions, artifact },
    Passed,
    Failed { counterexample },
    CounterexampleFound,
    Partial,
    Unknown,
    Timeout,
    ResourceLimit,
    Blocked { reason },
}
```
**Reglas**: No convertir Timeout→Verified, Unknown→Passed, Partial→Success, Failed→Success.

### 5.3 Contrato de Integridad
Antes de usar herramienta:
1. Verificar identidad
2. Comprobar versión
3. Validar fingerprint
4. Comprobar procedencia
5. Validar permisos
6. Verificar aislamiento
7. Comprobar límites ejecución

Si falla control obligatorio → BLOQUEAR. No degradar silenciosamente.

### 5.4 Contrato de Medición
Cada adaptador registra:
```
tool_version, definition_hash, execution_time, cpu_time, memory_peak,
tokens_input, tokens_output, network_io, result_status, verification_status,
error_count, timeout_count, resource_limit_count
```
Campos N/A = explícitamente N/A, no cero.

---

## Benchmark Framework (Transversal)

`lai bench capabilities` - Benchmark específico por capacidad

**Dimensiones**: Calidad, Verificación, Seguridad, Rendimiento, Eficiencia, Recuperación, Robustez, Fiabilidad, Mantenimiento, Autonomía

**Regla de admisión**: Herramienta entra en producción SOLO si:
1. Supera tests funcionales
2. Cumple contratos seguridad
3. Resultados reproducibles
4. No degrada garantías existentes
5. Mejora medible O capacidad nueva necesaria
6. Mantenimiento y versiones controladas
7. Mecanismo de desactivación
8. Sin dependencias innecesarias

---

## Dependencias por Fase (Cargo.toml)

### Fase 1 (Seguridad)
```toml
opa-wasm = "0.3"
rego = "0.1"
sha2 = "0.10"
ring = "0.17"
mcp-client = "0.1"  # hypothetical
```

### Fase 2 (Observabilidad)
```toml
langfuse = "0.1"  # hypothetical Rust client
opentelemetry = "0.27"
opentelemetry-otlp = "0.20"
tracing = "0.1"
tracing-subscriber = "0.3"
```

### Fase 3 (Testing)
```toml
proptest = "1.0"  # Rust property testing
mutest = "0.1"    # mutation testing
```

### Fase 4 (Hybrid RAG)
```toml
onnxruntime = "0.2"  # embeddings
tokenizers = "0.15"
```

### Fase 5 (Code Analysis)
```toml
tree-sitter = "0.5"
tree-sitter-python = "0.5"
tree-sitter-rust = "0.5"
```

### Fase 6 (Memory)
```toml
redis = "0.23"  # para Mem0/Graphiti backend
surreal-db = "0.1"  # graph DB para Graphiti
```

### Fase 7 (Planning)
```toml
petgraph = "0.6"  # DAG implementation
```

### Fase 8 (Verification)
```toml
# Dafny/TLA+ via subprocess - no Rust deps directos
```

### Fase 9 (LangGraph)
```toml
# LangGraph via Python bridge or HTTP
```

### Fase 10 (Neural Verification)
```toml
# α,β-CROWN, Marabou, ERAN via subprocess/HTTP
```

---

## Orden de Implementación Inmediato

1. **Estructura base**: Crear directorios `core/`, `orchestration/`, `planning/`, `memory/`, `retrieval/`, `code_analysis/`, `verification/`, `testing/`, `security/`, `observability/`
2. **Contratos**: `core/contracts.rs` - Tipos base compartidos
3. **Fase 1**: `security/integrity.rs`, `security/mcp_gateway.rs`, `security/policies.rs`
4. **Fase 2**: `observability/` completo con Langfuse adapter
5. **Fase 3**: `testing/property.rs`, `testing/mutation.rs`
6. **Fase 4**: `retrieval/` completo
7. **Fase 5**: `code_analysis/cpg.rs`, `code_analysis/adapters/joern.rs`
8. **Fase 6**: `memory/` completo con adapters
9. **Fase 7**: `planning/dag.rs`, `planning/scheduling.rs`
10. **Fase 8**: `verification/formal.rs`, `verification/model_checking.rs`
11. **Fase 9**: `orchestration/adapters/langgraph.rs`
12. **Fase 10**: `verification/neural.rs` + adapters
13. **Benchmark**: `benches/capabilities.rs` + runner
14. **Integración**: Actualizar `commands.rs` con 25+ comandos, `tools.rs` con 50+ tools, `automode.rs`, `agents.rs`, `session.rs`, `thinking.rs`, `config.rs`

---

## Riesgos y Mitigación

| Riesgo | Mitigación |
|---|---|
| Scope creep | Fases estrictas, criterios de salida obligatorios |
| Dependencias externas inestables | Adapters con interface estable, fallbacks nativos |
| Complejidad excesiva | Empezar simple, medir, iterar |
| Performance | Benchmarks en cada fase, budgets estrictos |
| Mantenibilidad | Contratos obligatorios, tests de integración |

---

## Próximos Pasos Inmediatos

1. ✅ Crear estructura de directorios
2. ✅ Definir `core/contracts.rs` 
3. 🔄 Implementar Fase 1 (Seguridad - MCP Hangar + Integridad)
4. 🔄 Implementar Fase 2 (Observabilidad - Langfuse)
5. ...

---

*Documento vivo - actualizar conforme avance la implementación*