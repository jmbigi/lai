# LAI v5.0 - Roadmap y Tareas Pendientes

> **Aviso de vigencia (2026-11-26).** Este roadmap describe el stack
> `docker-compose` de la fase v5.0 (llama.cpp, gateway, langfuse, joern,
> codebadger, mem0, graphiti, neo4j, Dafny, TLA+, Marabou, ERAN…). **Ese stack
> no forma parte del lai actual**: el proyecto se apoya en modelos locales vía
> Ollama/llama.cpp y en verificación determinista (compilador, linter, tests).
> Las tareas de abajo se conservan como registro histórico y **no** son
> objetivos comprometidos. El trabajo realmente pendiente está en
> `swebench/README.md` (corrida piloto) y en `DOCS/CAMBIOS_2026-11-26_PENDIENTES.md`.

## Estado Actual: ✅ COMPLETADO (Core)

### Implementado (Commit 0a649b6)
- **10 Herramientas Estratégicas** como módulos Rust
- **Framework de Benchmarks** con contratos obligatorios
- **CLI `lai bench`** funcional
- **Infraestructura Docker** completa
- **Tests unitarios** (17 passing)

---

## 📋 TAREAS PENDIENTES (histórico — ver aviso de vigencia arriba)

> Ninguna de estas tareas se ha ejecutado. Requieren el stack `docker-compose`
> descrito arriba, que no está en uso.

### 1. Tests de Integración con Servicios Reales
**Prioridad: ALTA** | **Estimación: 2-3 días**

```bash
# Levantar stack completo
docker-compose up -d

# Verificar servicios
docker-compose ps

# Ejecutar tests de integración
cd lai_rust && cargo test --test infrastructure -- --ignored
```

**Servicios a validar:**
- [ ] **llama.cpp** (puerto 8080) - inferencia LLM local
- [ ] **gateway** (puerto 8081) - routing y MCP
- [ ] **ollama** (puerto 11434) - alternativa LLM
- [ ] **langfuse** (puerto 3000) - observabilidad
- [ ] **mcp-hangar** (puerto 8084) - seguridad MCP
- [ ] **joern** (puerto 8082) - CPG analysis
- [ ] **codebadger** (puerto 8083) - vulnerability scanning
- [ ] **prometheus** (puerto 9090) - métricas
- [ ] **grafana** (puerto 3001) - dashboards
- [ ] **mem0** (puerto 8000) - memoria vectorial
- [ ] **graphiti** (puerto 8001) - memoria temporal + neo4j
- [ ] **neo4j** (puertos 7474/7687) - grafo de conocimiento
- [ ] **dafny** - verificación formal
- [ ] **tla/tlc** - model checking
- [ ] **alpha-beta-crown** - verificación neuronal
- [ ] **marabou** - verificación neuronal SMT
- [ ] **eran** - verificación neuronal abstract interpretation

### 2. Tests con Dependencias Externas
**Prioridad: ALTA** | **Estimación: 1-2 días**

Instalar y configurar:
- [ ] **Dafny 4.x** - `dotnet tool install -g dafny`
- [ ] **TLA+ Toolbox / TLC** - Java + tla2tools.jar
- [ ] **Joern** - ya en docker, validar CPG generation
- [ ] **Mem0/Graphiti** - APIs en docker, testear memory adapters
- [ ] **Langfuse** - configurar proyecto, testear trace ingestion
- [ ] **Z3/CVC5** - SMT solvers para verificación

### 3. Performance Tuning basado en Benchmarks
**Prioridad: MEDIA** | **Estimación: 1-2 semanas**

Ejecutar benchmarks completos:
```bash
# Todos los benchmarks
lai bench all --iterations 20 --output results.json --format json

# Benchmarks específicos
lai bench run lai.retrieval.hybrid --iterations 50
lai bench run lai.code_analysis.cpg --iterations 20
lai bench run lai.testing.property --iterations 30
```

**Métricas objetivo:**
- [ ] **Latencia p50 < 100ms** para Hybrid RAG retrieval
- [ ] **Latencia p95 < 500ms** para Hybrid RAG retrieval
- [ ] **CPG generation < 30s** para proyectos medianos
- [ ] **Property testing < 10s** para funciones simples
- [ ] **Memory usage < 512MB** en benchmarks estándar
- [ ] **Success rate > 95%** en todos los benchmarks

### 4. Mejoras de Arquitectura
**Prioridad: MEDIA** | **Estimación: 2-4 semanas**

- [ ] **Implementar CapabilityAdapter real** para cada herramienta (reemplazar MockAdapter)
- [ ] **Integrar MemoryStore** con Mem0/Graphiti reales
- [ ] **Conectar VerificationEngine** con Dafny/TLA+ reales
- [ ] **Integrar CodeAnalysis** con Joern/Codebadger reales
- [ ] **Conectar Observability** con Langfuse real
- [ ] **Implementar MCP Hangar** client real
- [ ] **Integrar NeuralVerification** con α,β-CROWN/Marabou/ERAN reales

### 5. Hardening y Producción
**Prioridad: BAJA** | **Estimación: 1-2 semanas**

- [ ] **Rate limiting** en gateway
- [ ] **Autenticación/Autorización** en API
- [ ] **TLS/SSL** para todos los servicios
- [ ] **Backup/restore** para bases de datos (neo4j, langfuse-db, prometheus)
- [ ] **Log aggregation** (Loki/ELK)
- [ ] **Alerting** (Alertmanager + reglas)
- [ ] **CI/CD pipeline** (GitHub Actions/GitLab CI)
- [ ] **Documentación API** (OpenAPI/Swagger)

### 6. Features Avanzadas
**Prioridad: BAJA** | **Estimación: 2-4 semanas**

- [ ] **OPA/Rego policies** - políticas declarativas
- [ ] **Tree-sitter** - parsing incremental multilenguaje
- [ ] **OpenTelemetry** - instrumentación estándar
- [ ] **SyGuS/CEGIS** - síntesis de programas
- [ ] **LibAFL** - fuzzing basado en cobertura
- [ ] **Mutmut** - mutation testing para Python
- [ ] **Z3/CVC5/Why3** - verificación deductiva avanzada

---

## 🚀 COMANDOS ÚTILES

```bash
# Levantar todo
docker-compose up -d

# Ver logs
docker-compose logs -f [servicio]

# Ejecutar tests unitarios
cd lai_rust && cargo test --lib --test core --test benchmarks

# Ejecutar benchmarks
cd lai_rust && cargo run -- bench all --iterations 20 --output benchmark-results.json

# Benchmark específico
cd lai_rust && cargo run -- bench run lai.retrieval.hybrid --iterations 50

# Listar benchmarks disponibles
cd lai_rust && cargo run -- bench list

# Detener todo
docker-compose down

# Limpiar todo (incluye volúmenes)
docker-compose down -v
```

---

## 📊 MÉTRICAS DE ÉXITO

| Métrica | Target | Estado |
|---------|--------|--------|
| Core compilation | 0 errors | ✅ |
| Unit tests passing | 17/17 | ✅ |
| Integration tests | 0/10 | ⏳ |
| External deps tests | 0/15 | ⏳ |
| Benchmark latency p50 | < 100ms | ⏳ |
| Benchmark success rate | > 95% | ⏳ |
| Memory usage | < 512MB | ⏳ |
| Docker stack health | 22/22 healthy | ⏳ |

---

## 📋 REGLAS DE ADMISIÓN (Sección 7.3 - Obligatorio)

Una herramienta **SOLO** entra en producción cuando:

- [ ] 1. Supera las pruebas funcionales
- [ ] 2. Cumple los contratos de seguridad (Sección 5)
- [ ] 3. Tiene resultados reproducibles (múltiples ejecuciones)
- [ ] 4. No degrada garantías existentes (fail-closed, verification)
- [ ] 5. Demuestra mejora medible O capacidad nueva necesaria (benchmark vs baseline)
- [ ] 6. Tiene mantenimiento y versiones controladas (semver, changelog)
- [ ] 7. Dispone de mecanismo de desactivación (feature flag, kill switch)
- [ ] 8. No introduce dependencias innecesarias (minimalismo)

**Regla de oro:** Una herramienta que no demuestra valor medible en `lai bench` permanece experimental o se descarta.

---

## 🚫 QUÉ NO HACER (Sección 9 - Prohibido)

| ❌ Decisión Prohibida | ✅ Alternativa Correcta |
|---|---|
| Sustituir ExecutionEngine por LangGraph | LangGraph como alternativa experimental (Phase 9) |
| Instalar Mem0 y Zep/Graphiti simultáneamente | Benchmark propio primero, elegir uno |
| Reemplazar FTS5 por embeddings | Hybrid RAG (FTS5 + embeddings + RRF) |
| Usar LLM como verificador | Verificadores externos: Dafny, TLC, α,β-CROWN, Hypothesis |
| Considerar Hypothesis = Dafny | Hypothesis = testing (evidencia experimental), Dafny = verificación formal |
| Ejecutar todas las herramientas MCP directo | MCP Hangar + digest pinning + integrity checks |
| Ejecutar análisis CPG en cada operación | Bajo demanda, cacheado |
| Adoptar DAGent como dependencia crítica | DAG nativo + DAGent como referencia |
| Incorporar verificadores neuronales sin modelos | Solo cuando existan componentes NN que verificar |
| Medir éxito por respuestas del LLM | Métricas objetivas: latency, tokens, verification rate, error rate |

---

## 🏗️ PRINCIPIOS ARQUITECTÓNICOS (Sección 4)

- **Adapter Pattern:** Cada herramienta externa tiene un `CapabilityAdapter` 
- **Contratos Internos Estables:** `CapabilityAdapter` trait + `ExecutionRequest/Result`
- **Fail-Closed:** Si integridad no verificada → bloquear ejecución
- **Verificación Acotada:** `ResultStatus` distingue `VERIFIED` (formal) vs `PASSED` (testing)
- **Integridad de Herramientas:** fingerprint + executable hash + container hash + deps versions
- **Medición Obligatoria:** Cada adapter registra métricas completas (Sección 5.4)
- **Autoridad del Núcleo:** LLM propone, núcleos deterministas deciden

---

## 🔒 CONTRATOS OBLIGATORIOS (Sección 5)

### 5.1 Contrato de Ejecución (`CapabilityAdapter`)
```rust
trait CapabilityAdapter {
    fn capability_id(&self) -> String;
    fn tool_version(&self) -> String;
    fn definition_hash(&self) -> String;
    async fn health_check(&self) -> HealthCheckResult;
    async fn preflight(&self, request: &ExecutionRequest) -> PreflightResult;
    async fn execute(&self, request: ExecutionRequest) -> ExecutionResult;
    async fn validate(&self, result: &ExecutionResult) -> ValidationResult;
    async fn get_metrics(&self, execution_id: &str) -> Option<ExecutionMetrics>;
}
```

### 5.2 Estados de Resultado (`ResultStatus`)
| Estado | Significado | Es Éxito? |
|---|---|---|
| `VERIFIED` | Verificado formalmente (Dafny, TLC, α,β-CROWN) | ✅ |
| `PASSED` | Test/check pasó (Hypothesis, lint, type-check) | ✅ |
| `FAILED` | Fallo reproducible | ❌ |
| `COUNTEREXAMPLE_FOUND` | Contraejemplo válido (Hypothesis, Marabou) | ❌ |
| `PARTIAL` | Completado parcialmente | ⚠️ |
| `UNKNOWN` | Evidencia insuficiente | ⚠️ |
| `TIMEOUT` | Presupuesto temporal agotado | ⚠️ |
| `RESOURCE_LIMIT` | Límite de recursos alcanzado | ⚠️ |
| `BLOCKED` | Condición obligatoria impidió continuar | ⚠️ |
| `ERROR` | Error interno | ❌ |

**Regla:** `TIMEOUT` ≠ `VERIFIED`, `UNKNOWN` ≠ `PASSED`, `PARTIAL` ≠ `SUCCESS`

### 5.3 Contrato de Integridad (Pre-ejecución)
1. Verificar identidad (nombre, versión, instance_id)
2. Validar fingerprint definición (SHA-256)
3. Validar hash ejecutable/imagen contenedor
4. Verificar versiones dependencias fijadas
5. Validar permisos explícitos
6. Verificar aislamiento (filesystem, red)
7. Comprobar límites ejecución (CPU, memoria, timeout)

### 5.4 Contrato de Medición (Por ejecución)
```text
tool_version, definition_hash, execution_time, cpu_time, memory_peak,
tokens_input, tokens_output, network_io, result_status, verification_status,
error_count, timeout_count, resource_limit_count
```

---

## 🎯 PLAN DE IMPLEMENTACIÓN PRIORIZADO (Sección 6)

| Fase | Incorporaciones | Prioridad | Criterio de Salida |
|---|---|---|---|
| 1 | **MCP Hangar + Integridad** | Crítica | Integridad 100%, discrepancias bloqueadas, auditoría completa |
| 2 | **Langfuse** | Crítica | Línea base: coste, latencia, herramientas, errores, verificaciones |
| 3 | **Hypothesis + Mutation Testing** | Muy Alta | Detectar mutaciones relevantes, reproducir contraejemplos |
| 4 | **Hybrid RAG** | Muy Alta | Mejora estadísticamente significativa vs FTS5 baseline |
| 5 | **Joern + codebadger** | Alta | CPG generation, taint tracking, vulnerability detection |
| 6 | **Memoria (Mem0/Graphiti)** | Alta | Benchmark propio: decisiones, contratos, dependencias |
| 7 | **Planificación DAG** | Alta | DAG nativo + comparación vs DAGent |
| 8 | **Dafny + TLA+** | Media-Alta | Módulos críticos: permisos, estado, planificador, transacciones |
| 9 | **LangGraph** | Media | Solo si reduce tokens/latencia SIN reducir verification rate |
| 10 | **α,β-CROWN / Marabou / ERAN** | Condicional | Cuando existan componentes NN que verificar |

---

## 📊 MÉTRICAS DE BENCHMARK (Sección 7.1)

| Dimensión | Métricas |
|---|---|
| **Calidad** | Exactitud, Recall@K, MRR, nDCG |
| **Verificación** | % propiedades verificadas, verification rate |
| **Seguridad** | Vulnerabilidades, controles incumplidos, integrity failures |
| **Rendimiento** | Latencia p50, p95, p99, throughput |
| **Eficiencia** | Tokens/tarea, CPU/tarea, memory/tarea |
| **Recuperación** | Recall@5, Recall@10, MRR@10, nDCG@10 |
| **Robustez** | Comportamiento entradas adversariales, fuzzing |
| **Fiabilidad** | Reproducibilidad, recovery rate, MTTR |
| **Mantenimiento** | Complejidad ciclomática, dependencias, LOC |
| **Autonomía** | % tareas sin intervención, human-in-the-loop rate |

**Comparación Obligatoria:** Mismo corpus, mismas consultas, mismos criterios, versiones fijadas, configuraciones documentadas, conjuntos de evaluación separados.

**Significancia Estadística:** p-value < 0.05, Cohen's d effect size, intervalos de confianza 95%.

---

## 🛠️ HERRAMIENTAS ADICIONALES A EVALUAR (Sección 8)

| Tecnología | Aplicación | Valor | Fase |
|---|---|---|---|
| **Z3** | SMT solving, constraints | Muy Alto | 6 |
| **CVC5** | SMT, logical reasoning | Muy Alto | 6 |
| **Why3** | Deductive verification | Muy Alto | 6 |
| **Semgrep** | Static analysis, security rules | Alto | 5 |
| **Tree-sitter** | Incremental parsing multi-lang | Alto | 6 |
| **OpenTelemetry** | Standard instrumentation | Muy Alto | 5 |
| **OPA/Rego** | Declarative policies | Muy Alto | 4/5 |
| **SyGuS/CEGIS** | Program synthesis | Muy Alto | 6 |
| **LibAFL** | Coverage-guided fuzzing | Alto | 6 |
| **Mutmut** | Mutation testing Python | Alto | 3 |

**Tres prioritarias:** OPA/Rego (políticas), Tree-sitter (parsing), OpenTelemetry (instrumentación).

---

## 🔗 RECURSOS