# LAI — Arquitectura de un agente autónomo de ingeniería de software

**Versión 4.0 — Especificación técnica final**
**Enfoque:** ingeniería determinista, autonomía controlada, seguridad de cadena de suministro, verificabilidad acotada y eficiencia medida.

> Esta especificación (v4.0) sustituye como fuente de verdad a la línea base `ARQUITECTURA_TECNICA_LAI.md` (v3.0/baseline), cuyas correcciones se incorporan aquí. La auditoría del estado real del código se mantiene en el documento baseline.

## 0. Correcciones factuales que obligan a revisar la propuesta anterior

La versión 3.0 partía de premisas que la evidencia disponible en 2026 ya no sostiene en los mismos términos. Corregirlas no invalida la dirección arquitectónica —la refuerza—, pero cambia el peso relativo de algunos componentes y elimina la dependencia de un benchmark que dejó de ser una referencia fiable.

### 0.1. SWE-bench Verified y SWE-bench Pro: ninguno es una "vara de medir" estable

OpenAI dejó de reportar resultados sobre SWE-bench Verified en febrero de 2026 y publicó en julio del mismo año una auditoría que documenta contaminación por exposición pública del repositorio, pruebas demasiado estrechas o demasiado amplias, y dificultades para hacer robusto el scoring automatizado. La propia OpenAI recomendó entonces SWE-bench Pro como alternativa. Sin embargo, la misma auditoría —y el trabajo posterior de SWE-Bench Pro V2— estima que aproximadamente el **30 % de las tareas del split público de SWE-bench Pro estaban rotas**, lo que obligó a reparar o retirar 480 de las 731 tareas originales mediante auditorías agénticas, adjudicación experta y reimplementación ciega.

La diferencia de dificultad entre ambos benchmarks es real y relevante: los mejores sistemas resuelven **menos del 45 % del conjunto público de SWE-bench Pro y menos del 20 % del conjunto propietario**, frente a más del 70 % en el antiguo Verified. Pero esa diferencia no convierte a Pro en una medida de corrección. **Conclusión operativa para `lai`:** ningún benchmark público puede usarse como criterio de aceptación de una tarea individual ni como prueba de capacidad general. El BenchmarkEngine debe tratar los resultados externos como **hipótesis a auditar**, no como evidencia.

### 0.2. SemLoc: los porcentajes de la versión 3.0 correspondían a un benchmark distinto

La versión 3.0 citaba "42,8 % de Acc@1" para SemLoc. Ese dato es correcto para **SemFault-250**, donde Ochiai SBFL obtiene un 6,4 % de Acc@1. Sin embargo, en **SemBugs** —el conjunto principal del trabajo— SemLoc alcanza **81,4 % de Acc@1, 91,5 % de Acc@3 y 93,4 % de Acc@5**, mientras que el LLM directo se queda en un 50,0 % de Acc@1. La diferencia entre ambas cifras no es un error de SemLoc: refleja que el rendimiento depende críticamente del conjunto de evaluación y de la presencia de "colapsos de cobertura".

Esto importa para `lai` porque una cifra aislada de localización de fallos no autoriza a incorporar SemLoc como mecanismo central. **SemLoc es una señal auxiliar prometedora, no una garantía.** Su valor arquitectónico real está en el principio que lo sustenta: convertir el razonamiento del LLM en **restricciones ejecutables y verificables** ancladas a estructuras del código, en lugar de aceptar una predicción de línea sin evidencia runtime. Ese principio sí debe adoptarse; el componente concreto, solo después de reproducir resultados en los proyectos de `lai`.

### 0.3. La seguridad de MCP tiene controles concretos que la versión 3.0 no especificaba

OWASP publicó en 2026 una guía específica para MCP que identifica **9 riesgos clave y 12 áreas de control**. El control más importante que faltaba en la versión anterior es el **hash de la definición completa de la herramienta al fijarla (pinning)**: si una definición cambia —aunque sea un cambio de descripción—, debe requerir una nueva evaluación de confianza. La especificación MCP vigente además exige el parámetro de recurso RFC 8707 y **prohíbe explícitamente el token passthrough**.

Un dato empírico adicional: en un benchmark de **1.348 casos de tool poisoning sobre 45 servidores MCP en producción**, el modelo más expuesto alcanzó una **tasa de éxito de ataque del 72,8 %**, y ningún modelo probado rechazó más del 3 % de las instrucciones envenenadas. Esto confirma la tesis central de la arquitectura —el LLM no es un control de seguridad— pero con una magnitud que hace inaceptable cualquier diseño que dependa de la alineación del modelo para la integridad de las herramientas.

## 1. Principios arquitectónicos (revisados)

Los diez principios de la versión 3.0 se mantienen, con dos precisiones que la evidencia anterior obliga a añadir.

| Principio | Regla | Precisión v4.0 |
|---|---|---|
| Determinismo | El control de ejecución no depende de decisiones probabilísticas del LLM. | Sin cambios. |
| Seguridad | Ninguna acción elude el motor de políticas. | Añadir: **fail-closed en la verificación de integridad de herramientas**; una definición no fijada por hash no se ejecuta. |
| Verificabilidad | Ningún resultado se declara correcto sin evidencia externa. | Añadir: la evidencia externa debe especificar **qué nivel de verificación se ejecutó y cuál no**, no solo "tests pasaron". |
| Reversibilidad | Todo cambio debe poder identificarse y revertirse. | Sin cambios. |
| Observabilidad | Cada acción genera una traza auditable. | Sin cambios. |
| Aislamiento | Cada tarea trabaja en un espacio de ejecución controlado. | Especificar mecanismo Linux: bubblewrap + cgroups v2 como opción por defecto, no como única opción. |
| Simplicidad | No incorporar complejidad sin demostrar su utilidad. | Reforzado: ningún componente nuevo entra sin **medición contra línea base en `lai bench`**. |
| Persistencia | Conservar estado y conocimiento útil entre sesiones. | Sin cambios. |
| Honestidad | Diferenciar hechos, inferencias, hipótesis y resultados comprobados. | Añadir: distinguir **"supera las verificaciones ejecutadas"** de **"correcto"**. |
| Medición | Toda mejora debe demostrar beneficios cuantificables. | Añadir: los benchmarks públicos son **hipótesis**, no unidades de progreso directamente comparables entre sí. |

## 2. Arquitectura general (sin cambios estructurales; con contratos más estrictos)

La topología de capas de la versión 3.0 es correcta y no requiere rediseño. Lo que cambia es el **contrato entre capas**: cada componente debe declarar qué evidencia produce y qué incertidumbre deja sin resolver.

```text
USUARIO → LAI INTERFACE → TASK MANAGER → EXECUTION ENGINE (FSM)
                                              |
              +---------------+---------------+---------------+
              |               |               |               |
              v               v               v               v
          PLANNER         REPO MAP       CONTEXT BUILDER   MEMORY STORE
          (SPEC)          (AST+index)    (presupuesto)      (procedencia)
              |               |               |               |
              +---------------+---------------+---------------+
                                              |
                                              v
                                     TOOL REGISTRY
                                     (hash fijado, schema)
                                              |
                                              v
                                     POLICY ENGINE
                                     (fail-closed)
                                              |
                                              v
                                     EXECUTION SANDBOX
                                     (bwrap + cgroups v2)
                                              |
                                              v
                                     CHANGE ENGINE
                                     (checkpoint + diff)
                                              |
                                              v
                                     VERIFICATION ENGINE
                                     (niveles explícitos)
                                              |
                              +---------------+---------------+
                              |                               |
                              v                               v
                          SUCCESS                    DIAGNOSE / REPAIR
                              |                               |
                              v                               v
                          EVIDENCE                      (límite estricto)
```

## 3. Las diez mejoras estratégicas (versión revisada)

### MEJORA 1 — ExecutionEngine: máquina de estados determinista

**Prioridad: CRÍTICA. Sin cambios en el diseño de estados de la v3.0.**

La corrección necesaria es de **contrato de aceptación**. La versión 3.0 exigía "100 ejecuciones sin transiciones ilegales". Eso es necesario pero insuficiente: una FSM puede no tener transiciones ilegales y aun así **declarar SUCCESS sin verificación real**. El criterio debe incluir:

- 100 ejecuciones sin transiciones ilegales.
- 100 ejecuciones en las que **ningún estado SUCCESS se alcanza sin que el VerificationEngine haya emitido un veredicto de nivel ≥ 5** (tests existentes) o una justificación explícita de por qué ese nivel no aplica.
- Toda transición a `ABORTED` registra la condición exacta de bloqueo (presupuesto, estancamiento, reparación agotada).

**Añadido operativo:** el estado `AUTHORIZE` debe fallar cerrado si el `ToolRegistry` no puede verificar el hash de la definición de la herramienta invocada. Esto conecta directamente con la Mejora 7 y con la evidencia de MCPTox: un modelo que acepta el 72,8 % de instrucciones envenenadas no puede ser la última línea de defensa de la integridad de las herramientas.

### MEJORA 2 — RepoMap: comprensión estructural

**Prioridad: CRÍTICA. La elección tecnológica de la v3.0 está respaldada.**

Tree-sitter para parsing incremental y SQLite con FTS5 para búsqueda textual es el patrón que emplean las implementaciones maduras de indexación de código para agentes en 2026: `codebase-indexer-py` almacena símbolos extraídos con tree-sitter en un grafo SQLite comprimido con WAL + FTS5; `srclight` combina FTS5, tree-sitter y embeddings sobre SQLite y expone el índice mediante MCP, "reemplazando docenas de llamadas grep/glob con búsquedas estructuradas precisas".

**Lo que debe corregir de la v3.0:** la regla "no incorporar embeddings inicialmente" es correcta como decisión de simplicidad, pero no debe convertirse en dogma. La evidencia de estos proyectos muestra que la combinación de **BM25 (FTS5) y búsqueda vectorial fusionadas mediante Reciprocal Rank Fusion** es viable en local y no requiere un servicio externo. La regla revisada es: **FTS5 y grafo de símbolos primero; embeddings solo si `lai bench` demuestra mejora en tasa de localización o reducción de llamadas a herramientas sin aumentar el coste de mantenimiento del índice.**

**Criterio de aceptación revisado:** además de reducir archivos inspeccionados, RepoMap debe demostrar que **el índice incremental se actualiza correctamente tras un cambio de archivo** sin reconstruir todo el grafo. Un índice estructural desactualizado es peor que un grep: produce confianza injustificada.

### MEJORA 3 — ContextBuilder: contexto como presupuesto, no como acumulación

**Prioridad: CRÍTICA. Sin cambios en la jerarquía.**

Se mantiene la regla central: **la evidencia reciente de ejecución y verificación tiene prioridad sobre las inferencias almacenadas en memoria.** La precisión que añade la evidencia de 2026 es que el contexto no solo se contamina por exceso de información, sino por **contenido adversarial**. Todo output de herramienta —incluido el de un servidor MCP aparentemente fiable— debe tratarse como dato no confiable hasta que el PolicyEngine lo valida contra el esquema de salida fijado. El ContextBuilder no debe insertar en el prompt resultados de herramientas que no hayan pasado esa validación.

**Contrato de honestidad:** cuando el ContextBuilder omite información relevante por presupuesto, debe registrar esa omisión en la traza. Un SUCCESS obtenido con contexto truncado no es equivalente a un SUCCESS con contexto completo, y el sistema debe poder distinguirlo.

### MEJORA 4 — Planner: especificaciones verificables

**Prioridad: ALTA. Sin cambios estructurales.**

La única adición necesaria es que el `SPEC.md` debe declarar **qué niveles de verificación están en alcance y cuáles están fuera de alcance**. Una especificación que no dice "no verificado" está sobreprometiendo. El formato mínimo por operación debe incluir un campo `verification_scope` con los niveles aplicables de la tabla de la Mejora 5.

### MEJORA 5 — VerificationEngine: niveles explícitos y veredicto acotado

**Prioridad: CRÍTICA. Esta es la mejora que más se beneficia de las correcciones factuales.**

El pipeline de 10 niveles de la v3.0 es correcto. Lo que debe corregir es la **semántica del veredicto final**.

La regla "si alguna verificación no pudo ejecutarse, el resultado debe ser `PARTIALLY_VERIFIED` o `BLOCKED`, nunca `SUCCESS`" es necesaria pero no suficiente. Un proyecto puede no tener tests de integración; eso no lo hace automáticamente `BLOCKED`. La regla revisada distingue tres situaciones:

1. **Verificación requerida por la especificación y no ejecutada** → `BLOCKED` (la tarea no puede declararse resuelta).
2. **Verificación fuera de alcance declarada en la especificación** → se registra en el veredicto como `OUT_OF_SCOPE`, no como fallo, pero **el veredicto global no puede ser `SUCCESS` pleno**: debe ser `SUCCESS_WITH_DECLARED_GAPS`.
3. **Verificación requerida, ejecutada y superada** → contribuye a `SUCCESS` en su nivel.

Esto evita dos errores simétricos: declarar éxito sin evidencia, y bloquear tareas por la ausencia de verificaciones que el proyecto nunca tuvo.

**Sobre SemLoc:** debe integrarse, si se integra, como **señal de diagnóstico en el DiagnosticEngine**, no como parte del veredicto de verificación. Su principio —restricciones ejecutables ancladas al código— es valioso, pero sus resultados (81,4 % Acc@1 en SemBugs, 42,8 % en SemFault-250) dependen del benchmark y no sustituyen una prueba de regresión.

### MEJORA 6 — DiagnosticEngine y RepairEngine

**Prioridad: ALTA. Se mantienen los controles de la v3.0 (máximo 3–5 reparaciones, no modificar tests para ocultar fallos, no desactivar verificaciones).**

La corrección necesaria: el RepairEngine **no debe poder modificar la especificación** (`SPEC.md`) ni los criterios de aceptación. Si el diagnóstico concluye que la especificación es incorrecta, la acción correcta es **detener la reparación y elevar la tarea a revisión humana**, no reescribir el objetivo para que el cambio actual parezca correcto. La v3.0 lo insinuaba; la v4.0 lo convierte en invariante del runtime.

### MEJORA 7 — PolicyEngine y SandboxManager: controles concretos, no invocaciones genéricas

**Prioridad: CRÍTICA. Esta sección es la que más se concreta respecto a la v3.0.**

#### 7.1. PolicyEngine

El modelo de autorización `REQUEST → VALIDATE → CHECK_POLICY → CHECK_RISK → EXECUTE → AUDIT` se mantiene. Se añaden tres invariantes:

- **Fail-closed en integridad de herramientas.** Si el hash de la definición de la herramienta no coincide con el hash fijado, la operación se deniega. OWASP 2026 recomienda hashear la definición completa, no solo el nombre o la versión.
- **Scopes con expiración automática.** La elevación de privilegios por "scope creep" (MCP02 en el OWASP MCP Top 10) se mitiga con scopes de mínima privilegio y caducidad automática, no con revisión manual periódica.
- **Sin token passthrough.** Cuando se usen servidores MCP remotos, el cliente debe enviar el parámetro de recurso RFC 8707 y no reenviar tokens de otro contexto.

#### 7.2. SandboxManager

La v3.0 mencionaba bubblewrap, namespaces y cgroups sin especificar la relación entre ellos. La implementación de referencia en 2026 es **bubblewrap para aislamiento de namespaces (PID, user, mount, network, IPC) + cgroups v2 para límites de recursos (memoria, PIDs, CPU)**, sin contenedores ni VMs, con `/workspace` y `/tmp` escribibles y red deshabilada por defecto. Esta es exactamente la combinación que emplean sandboxes de agentes en producción (`deepagents-sandbox` la describe como "similar al enfoque del sandbox Linux de Codex").

**Advertencia operativa que la v3.0 no incluía:** en Ubuntu 24.04 y posteriores, AppArmor restringe los user namespaces no privilegiados y puede impedir que bubblewrap funcione sin configuración adicional. El SandboxManager debe **detectar esta condición al arrancar y fallar cerrado** (denegar ejecución con sandbox) en lugar de ejecutar sin aislamiento. Un sandbox que no puede aplicarse no es un sandbox.

**Sobre la clasificación de operaciones:** se mantiene la tabla de cuatro niveles. La única precisión es que "instalar dependencias" debe clasificarse como **Alto** solo si la instalación está restringida a un lockfile verificado; si no hay verificación de integridad de la cadena de suministro, debe clasificarse como **Crítico** y requerir aprobación explícita.

### MEJORA 8 — ToolRegistry y MCP: integridad como requisito de ejecución

**Prioridad: ALTA. Se eleva a CRÍTICA en lo que respecta a verificación de integridad.**

El ToolRegistry de la v3.0 ya exigía "hash de definición". La v4.0 convierte ese requisito en **precondición de ejecución**:

| Campo | Requisito v4.0 |
|---|---|
| `definition_hash` | Hash de la definición completa (nombre, esquema de entrada, esquema de salida, descripción, permisos). |
| `pinned_version` | Versión fijada; cambio de versión = nueva evaluación. |
| `scope_expiry` | Fecha de caducidad del scope; por defecto, corta. |
| `provider_identity` | Identidad verificable del proveedor del servidor MCP. |
| `output_schema_validated` | La salida debe validarse contra el esquema antes de entrar en el contexto. |

La razón de elevar esto a crítico es empírica: los ataques de tool poisoning no requieren modificar el código del servidor; pueden operar modificando la descripción de una herramienta para que el modelo la invoque de forma maliciosa. Si el runtime no verifica la definición **antes de cada ejecución** (o contra un hash fijado que invalide automáticamente), el modelo es el único control, y la evidencia de MCPTox muestra que ese control es insuficiente.

### MEJORA 9 — MemoryStore: procedencia y invalidación

**Prioridad: ALTA. Sin cambios estructurales en las cuatro categorías (semántica, episódica, de trabajo, documental).**

La adición necesaria es una regla de **invalidación por cambio de herramienta**. Cuando una definición de herramienta cambia (nuevo hash), toda memoria episódica que registre "esta herramienta produjo este resultado" debe marcarse como **potencialmente inválida**, no eliminarse silenciosamente. El campo `invalidated_by` de la v3.0 ya existía; la v4.0 especifica que un cambio de `definition_hash` es un evento de invalidación de primer nivel.

**Regla de aprendizaje:** la "adaptación de parámetros operativos" (segunda operación de aprendizaje en la v3.0) solo puede aplicarse si existe un benchmark interno que demuestre que la adaptación mejora la métrica objetivo **sin degradar la tasa de verificación**. Ajustar parámetros para resolver más tareas a costa de verificar menos es una regresión disfrazada de mejora.

### MEJORA 10 — BenchmarkEngine: de "puntuación" a "evidencia de decisión"

**Prioridad: CRÍTICA. Esta es la mejora que más cambia respecto a la v3.0.**

La v3.0 trataba los benchmarks externos como referencias y el benchmark interno como medida de progreso. La evidencia de 2026 obliga a invertir esa relación: **el benchmark interno es la única medida que `lai` puede controlar y auditar; los benchmarks públicos son hipótesis externas que deben auditarse antes de usarse.**

#### 10.1. SWE-bench Pro en `lai bench`: uso correcto

SWE-bench Pro **no debe usarse como criterio de aceptación por tarea**. Puede usarse como **conjunto de tareas de regresión de alto nivel** para comparar versiones del harness, siempre que:

- Se registre la **versión exacta del split** utilizado (V2 tiene 642 tareas, tras reparar/retirar 480 de las 731 originales).
- Se registre que el propio benchmark tiene **tareas auditadas y tareas retiradas**; una puntuación sobre V2 no es comparable con una puntuación sobre el split original.
- Se use el **protocolo endurecido** (acceso de red restringido al endpoint del modelo, regrading en contenedor limpio) para evitar que el agente recupere fixes upstream.

#### 10.2. SWE-bench Verified: eliminación como referencia de capacidad

Debe eliminarse de la tabla de benchmarks de capacidad. Puede conservarse únicamente como **referencia histórica** para comparar con literatura anterior, con una nota explícita de que OpenAI documentó su contaminación y dejó de usarlo.

#### 10.3. Métricas internas obligatorias (sin cambios) más dos nuevas

Se mantienen las métricas de la v3.0 (tasa de resolución, regresiones, porcentaje verificado, tokens, coste, latencia, invocaciones, estabilidad, acciones bloqueadas, recuperación). Se añaden:

| Métrica nueva | Definición |
|---|---|
| **Tasa de verificación efectiva** | Porcentaje de tareas con veredicto `SUCCESS` que tienen al menos una verificación de nivel ≥ 5 ejecutada y superada. |
| **Tasa de integridad de herramientas** | Porcentaje de invocaciones de herramientas cuya definición fue verificada por hash antes de ejecutarse. Debe ser **100 %**; cualquier valor inferior es un fallo de seguridad, no una métrica de calidad. |

#### 10.4. Metodología experimental

Se mantiene el registro obligatorio de modelo, versión, configuración, commit, dataset, hash de entorno, presupuesto, evidencia y repeticiones. Se añade una regla de **tres repeticiones mínimas** en cualquier comparación que involucre un LLM como componente, no solo en tareas "no deterministas": la variabilidad entre ejecuciones es una propiedad del sistema completo, no de un subconjunto de tareas.

## 4. Mejoras transversales adicionales (revisadas)

### 4.1. Aislamiento por tarea

Se mantiene el uso de Git worktrees. La precisión necesaria: el aislamiento de filesystem no aísla efectos de red ni procesos huérfanos. El SandboxManager debe garantizar que al cerrar una tarea **no queden procesos en ejecución** con acceso al workspace, incluso si el proceso padre terminó. Un worktree revertido con un proceso hijo todavía escribiendo no es un rollback.

### 4.2. Change Engine transaccional

Se mantiene el flujo `validar estado → diff → checkpoint → aplicar → verificar integridad`. La adición: **el checkpoint debe incluir el estado del índice RepoMap**, no solo los archivos. Si se revierte un cambio pero el índice sigue reflejando el estado modificado, el siguiente ciclo de contexto trabaja sobre una representación falsa del repositorio.

### 4.3. Observabilidad

Se mantienen los campos de evento de la v3.0. Se añade obligatoriamente:

- `tool_definition_hash`: hash de la definición de la herramienta invocada.
- `policy_decision`: `ALLOW` / `DENY` / `APPROVAL_REQUIRED`.
- `verification_levels_attempted` y `verification_levels_passed`.

Sin estos tres campos, una traza no permite responder la pregunta central de la arquitectura: **¿esta acción estuvo autorizada y su resultado fue verificado?**

### 4.4. Concurrencia

Se mantienen los bloqueos por workspace y la detección de conflictos. La precisión: dos tareas que comparten el mismo índice RepoMap pero operan sobre worktrees distintos **no pueden indexar concurrentemente** sin coordinación, porque el índice es un recurso compartido. El bloqueo debe ser por par `(repo, índice)` para escritura de índice, y por `(repo, worktree)` para cambios de archivo.

## 5. Hoja de ruta (ajustada)

El orden de fases de la v3.0 es correcto. Se ajustan los criterios de salida para que reflejen las correcciones anteriores.

### Fase 0 — Auditoría y línea base

Criterio de salida añadido: **la línea base debe incluir una medición de la tasa de integridad de herramientas** (aunque sea 0 % en el estado inicial). Sin esa medición, no se puede demostrar la mejora de la Fase 5.

### Fase 1 — Runtime y seguridad (prioridad máxima)

Se mantiene. Criterio de salida añadido: **detección de AppArmor/namespaces no privilegiados y fallo cerrado** del SandboxManager cuando bubblewrap no puede aplicarse.

### Fase 2 — Comprensión del repositorio

Se mantiene. Criterio de salida añadido: **el índice incremental se invalida correctamente tras cambios de archivo y los símbolos eliminados no aparecen en consultas posteriores**.

### Fase 3 — Modificaciones y verificación

Se mantiene. Criterio de salida añadido: **el veredicto `SUCCESS_WITH_DECLARED_GAPS` se emite correctamente cuando hay verificaciones fuera de alcance**, y no se confunde con `SUCCESS` pleno en las métricas internas.

### Fase 4 — Planificación y memoria

Se mantiene. Criterio de salida añadido: **un cambio de `definition_hash` de herramienta invalida las memorias episódicas dependientes**, verificable mediante un test de invalidación.

### Fase 5 — Especialización e interoperabilidad

Se mantiene. Criterio de salida añadido: **100 % de invocaciones de herramientas con `definition_hash` verificado**; cualquier invocación sin hash fijado se bloquea y se registra como incidente de seguridad.

### Fase 6 — Auto-mejora experimental

Se mantiene. Criterio de salida añadido: **ninguna propuesta de auto-mejora puede modificar el PolicyEngine, el VerificationEngine ni el ToolRegistry sin aprobación humana explícita**. La auto-mejora puede optimizar parámetros de planificación, contexto y reparación; no puede modificar los mecanismos de control.

## 6. Criterios globales de aceptación (tabla revisada)

| Área | Condición de aceptación v4.0 |
|---|---|
| Ejecución | FSM sin transiciones ilegales; **ningún SUCCESS sin verificación de nivel ≥ 5 o gaps declarados**. |
| Seguridad | 100 % de acciones controladas por políticas; **100 % de invocaciones con `definition_hash` verificado**. |
| Aislamiento | Tareas sin interferencia; **fallo cerrado si el sandbox no puede aplicarse**. |
| Reversibilidad | Checkpoint y rollback probado; **incluye invalidación del índice RepoMap**. |
| Comprensión | Referencias estructurales verificables; **índice incremental consistente tras cambios**. |
| Planificación | Especificaciones con `verification_scope` explícito. |
| Verificación | Evidencia externa obligatoria; **distinción entre `SUCCESS`, `SUCCESS_WITH_DECLARED_GAPS` y `BLOCKED`**. |
| Diagnóstico | Clasificación reproducible; **SemLoc u otras señales auxiliares no sustituyen pruebas de regresión**. |
| Reparación | Reintentos limitados; **prohibido modificar `SPEC.md` o criterios de aceptación**. |
| Memoria | Procedencia, confianza e invalidación; **invalidación por cambio de hash de herramienta**. |
| Observabilidad | Trazas con `tool_definition_hash`, `policy_decision` y niveles de verificación. |
| Eficiencia | Coste y latencia medidos; **ninguna mejora de resolución a costa de tasa de verificación**. |
| Auto-mejora | Evaluación comparativa; **prohibido modificar controles sin aprobación humana**. |

## 7. Qué NO debe hacer `lai` (lista revisada)

Se mantienen los doce puntos de la v3.0. Se añaden tres:

13. **No tratar SWE-bench Pro (ni ningún benchmark público) como criterio de aceptación de una tarea individual.** Un benchmark es una distribución de tareas, no una especificación de corrección.
14. **No ejecutar una herramienta cuyo `definition_hash` no pueda verificarse.** La alineación del modelo no es un control de integridad; los datos de MCPTox muestran tasas de éxito de ataque superiores al 70 % en modelos expuestos.
15. **No declarar `SUCCESS` cuando la verificación ejecutada es de un nivel inferior al requerido por la especificación, aunque todas las verificaciones disponibles hayan pasado.** "Todo lo que ejecuté pasó" no es lo mismo que "se verificó lo que debía verificarse".

## 8. Arquitectura mínima viable (ajustada)

La estructura de directorios de la v3.0 es correcta. Se añaden dos módulos que la evidencia de 2026 hace necesarios desde la primera versión:

```text
lai/
+-- tools/
|   +-- registry.py
|   +-- integrity.py      # NUEVO: verificación de definition_hash
|   +-- executor.py
|
+-- security/
|   +-- policy_engine.py
|   +-- sandbox_manager.py
|   +-- sandbox_preflight.py  # NUEVO: detección AppArmor/namespaces
```

`integrity.py` no es opcional si se va a usar cualquier herramienta externa o servidor MCP. `sandbox_preflight.py` no es opcional si `lai` va a ejecutarse en Ubuntu 24.04+ o entornos con restricciones de user namespaces.

## 9. Evaluación final revisada

La puntuación de 9,5/10 de la v3.0 evaluaba la **coherencia interna de la propuesta**, no su respaldo empírico. Con las correcciones anteriores, la valoración cambia en dos dimensiones:

| Área | v3.0 | v4.0 | Razón del cambio |
|---|---:|---:|---|
| Arquitectura general | 9,5 | 9,5 | La topología de capas sigue siendo correcta. |
| Determinismo y control | 10 | 10 | La FSM y el fail-closed se refuerzan. |
| Seguridad | 9,5 | 9,5 | Se concretan controles MCP y sandbox; no sube porque la superficie de ataque sigue siendo amplia. |
| Comprensión de repositorios | 9,5 | 9,5 | La elección tree-sitter + FTS5 está respaldada; embeddings quedan como opción medida. |
| Verificación | 10 | 9,5 | Baja: la v3.0 sobreprometía con "verificación independiente"; la v4.0 reconoce gaps declarados. |
| Diagnóstico y reparación | 9,5 | 9,0 | Baja: SemLoc no es una garantía general; es una señal auxiliar dependiente del benchmark. |
| Memoria persistente | 9 | 9 | Se añade invalidación por herramienta. |
| Especialización | 9 | 9 | Sin cambios. |
| Eficiencia | 9,5 | 9,5 | Sin cambios. |
| Auto-mejora | 9 | 9 | Se añade prohibición de modificar controles. |
| **Global** | **9,5** | **9,3** | La propuesta es más honesta sobre lo que no puede garantizar. |

La bajada de 0,2 puntos no es un empeoramiento: es el resultado de eliminar dos sobrepromesas —la verificación como garantía de corrección y SemLoc como mecanismo central— que la evidencia de 2026 no respalda.

## 10. Recomendación final

La secuencia de cuatro componentes de la v3.0 —`ExecutionEngine`, `PolicyEngine`, `RepoMap`, `VerificationEngine`— sigue siendo correcta, pero debe ejecutarse con una precondición que la v3.0 no explicitaba: **el `PolicyEngine` debe incluir la verificación de integridad de herramientas desde el primer día**. No es una mejora de fase 5. Es una condición de la fase 1.

El objetivo de `lai` no es resolver más tareas en un benchmark público que ya ha demostrado ser una medida inestable. Es construir un sistema que, para cada tarea que declara resuelta, pueda responder con evidencia:

1. **Qué se autorizó** y con qué política.
2. **Qué se verificó** y en qué nivel.
3. **Qué no se verificó** y por qué.
4. **Qué herramientas se usaron** y con qué integridad.
5. **Qué límites se respetaron** y qué presupuesto se consumió.

Un sistema que puede responder esas cinco preguntas para cada tarea es más valioso —y más autónomo en el sentido útil del término— que uno que obtiene una puntuación alta en un benchmark que ya no mide lo que dice medir.

## 11. Estado de implementación en esta rama (feat/fase-0-1-harness)

Correspondencia v4.0 ↔ código (rama `feat/fase-0-1-harness`):

| Componente v4.0 | Estado |
|---|---|
| ExecutionEngine (FSM) | `harness.rs` — INTAKE/INSPECT/PLAN/LOCATE/EDIT/VERIFY/REPAIR/REPORT, límites de iteraciones, checkpoints, recuperación |
| PolicyEngine fail-closed | `policy.rs` — allowlist, frontera de workspace, rutas protegidas, secretos en `write_file`/`apply_patch`, `require_sandbox` con denegación incondicional |
| Tool integrity (definition_hash) | `integrity.rs` + `tools.rs` — hash SHA-256 de la definición completa fijado al registrar; verificación **antes de cada invocación** (fail-closed); `verify_all()` |
| SandboxManager + preflight | `sandbox.rs` — sonda real de bubblewrap (namespaces PID+net); Ubuntu 24.04+ con AppArmor restringido se detecta y el PolicyEngine **falla cerrado** (`lai harness --sandbox`) |
| RepoMap | `repomap.rs` — índice AST (syn) + heurístico Python, `summary()`, `find_files`, `find_symbols`, **actualización incremental** (`update()` por hash de contenido; símbolos eliminados no reaparecen) |
| ContextBuilder | `context.rs` — `ContextEngine` con presupuesto de tokens y truncamiento registrado |
| Planner (Spec-Driven) | `spec.rs` — `generate_spec` con validación estructural y 2 reintentos, `verification_scope` explícito, `SPEC.md` persistido en `.lai/specs/` |
| VerificationEngine | `verifier.rs` — niveles 1-10, veredicto acotado (`SUCCESS` / `SUCCESS_WITH_DECLARED_GAPS` / `PARTIALLY_VERIFIED` / `BLOCKED`), baseline y detección de regresiones, clasificación de fallos (código/entorno/dependencia/config) |
| MemoryStore | `memory.rs` — SQLite + FTS5 local, memoria semántica/episódica, procedencia y confianza, **invalidación por cambio de definition_hash** (`invalidate_by_tool_hash`), destilación de trayectorias |
| MCP (interoperabilidad) | `mcp.rs` — servidor stdio JSON-RPC 2.0 (`initialize`, `tools/list`, `tools/call`) sobre el ToolRegistry; toda invocación pasa integridad + PolicyEngine |
| BenchmarkEngine | `bench.rs` + `benchmarks/tasks/*.json` + `lai bench` — métricas: resolución, verificación efectiva, integridad (100 % por construcción), latencia, iteraciones; resultados versionados en `.benchmarks/results/` |
| Observabilidad | `telemetry.rs` — eventos `TASK/LLM/TOOL/ERROR/CKPT/POLICY/CTX/REPAIR` en consola y JSONL |
| Deny incondicional | `harness.rs::edit_phase` — integridad + PolicyEngine; `auto_approve` nunca convierte un Deny en ejecución |

**Estado del build:** `cargo check` 0 errores · `cargo test` 45/45 en verde · binario valida `harness`/`bench`/`mcp`.

Pendiente para fases posteriores (según hoja de ruta v4.0): revisión adversarial (Fase 5), aislamiento por tarea con worktrees + concurrencia (transversal 4.1/4.4), Change Engine transaccional completo (transversal 4.2), suite de benchmark ampliada y comparativas (Fase 6), auto-mejora controlada.