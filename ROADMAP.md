# LAI v5.0 - Roadmap y Tareas Pendientes

## Estado Actual: ✅ COMPLETADO (Core)

### Implementado (Commit 0a649b6)
- **10 Herramientas Estratégicas** como módulos Rust
- **Framework de Benchmarks** con contratos obligatorios
- **CLI `lai bench`** funcional
- **Infraestructura Docker** completa
- **Tests unitarios** (17 passing)

---

## 📋 TAREAS PENDIENTES

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

## 🔗 RECURSOS

- **Repositorio:** https://github.com/jmbigi/lai
- **Docker Hub:** ghcr.io/ggerganov/llama.cpp, ollama/ollama, langfuse/langfuse, etc.
- **Documentación:** AGENTS.md, ARQUITECTURA_TECNICA_LAI.md
- **Benchmarks:** standard_benchmarks::all_benchmarks()

---

*Última actualización: 2026-11-23*
*Commit: 0a649b6*