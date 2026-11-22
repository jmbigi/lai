# Documentación de Errores y Lecciones Aprendidas - lai Rust Port

**Fecha:** 2026-11-22  
**Commit:** 233cf76  
**Branch:** master

---

## Resumen Ejecutivo

Durante el port completo de lai de Python a Rust, se encontraron y resolvieron múltiples errores. Este documento registra todos los errores, sus causas, soluciones y lecciones aprendidas para futuras referencias.

---

## Errores por Categoría

### 1. Borrow Checker Errors (E0499, E0502, E0596, E0597)

#### Error: Cannot borrow `*self` as mutable more than once
**Ubicación:** `automode.rs` líneas 181-221 (loop de ejecución de plan)
**Causa:** Iterar sobre `self.plan` con `&mut` y luego llamar métodos que toman `&mut self` dentro del loop
```rust
// ❌ INCORRECTO
for i in 0..self.plan.len() {
    let step = &mut self.plan[i];  // mutable borrow
    step.status = "in_progress".into();
    self.log(...);  // another mutable borrow!
    let result = self.execute_step_react(step).await?;  // another!
}
```

**Solución:** Extraer datos necesarios antes del mutable borrow, usar índices
```rust
// ✅ CORRECTO
let (step_id, step_desc, step_criteria) = {
    let step = &self.plan[i];
    (step.id.clone(), step.description.clone(), step.criteria.clone())
};
self.plan[i].status = "in_progress".into();  // solo un mutable borrow
let temp_step = PlanStep { id: step_id, ... };
let result = self.execute_step_react(&temp_step).await?;
```

#### Error: Use of moved value
**Ubicación:** `tools.rs` línea 106 (register_tool)
**Causa:** `name` movido al closure y luego usado para insertar en HashMap
```rust
// ❌ INCORRECTO
let name = name.to_string();
let wrapped = if is_sensitive {
    let name = name.clone();  // moved into closure
    Arc::new(move |args| { ... name ... })
} else { ... };
self.tools.insert(name, wrapped);  // use after move!
```

**Solución:** Clonar para la inserción final
```rust
let name = name.to_string();
let name_for_insert = name.clone();  // keep for insert
let wrapped = if is_sensitive {
    let name = name.clone();
    Arc::new(move |args| { ... name ... })
} else { ... };
self.tools.insert(name_for_insert, wrapped);  // use clone
```

#### Error: Cannot borrow `messages` as mutable
**Ubicación:** `automode.rs` línea 546 (revise_step)
**Causa:** `messages` declarado como `let` inmutable, luego se intenta `push`
```rust
// ❌ INCORRECTO
let messages = vec![...];
messages.push(...);  // error!
```

**Solución:** Declarar como `mut`
```rust
let mut messages = vec![...];
messages.push(...);  // OK
```

---

### 2. Type Mismatch Errors (E0308)

#### Error: `?` operator on `ChatResult`
**Ubicación:** `agents.rs` múltiples líneas
**Causa:** `ChatResult` no implementa `Try`, tiene campo `error: Option<String>`
```rust
// ❌ INCORRECTO
let response = self.llm.chat_messages(...).await?;
```

**Solución:** Verificar campo `error` manualmente
```rust
let response = self.llm.chat_messages(...).await;
if let Some(err) = response.error {
    return Err(anyhow::anyhow!("Error: {}", err));
}
// use response.content, response.tool_calls directly
```

#### Error: `unwrap_or_default` on `String` and `Vec`
**Ubicación:** `agents.rs` líneas 183, 184, 233, 234, 241, 242, 266, 267, 295, 332, 357
**Causa:** `ChatResult.content` es `String` (no `Option`), `tool_calls` es `Vec` (no `Option`)
```rust
// ❌ INCORRECTO
output: response.content.unwrap_or_default(),
tool_calls: response.tool_calls.unwrap_or_default(),
```

**Solución:** Usar directamente (ya son tipos con Default implícito)
```rust
// ✅ CORRECTO
output: response.content,
tool_calls: response.tool_calls,
// O si se necesita clone:
output: response.content.clone(),
tool_calls: response.tool_calls.clone(),
```

#### Error: Mismatched types `Vec<ToolCall>` vs `Option<Vec<ToolCall>>`
**Ubicación:** `agents.rs` líneas 184, 234, 242, 267, 295, 332, 357
**Causa:** `AgentResult.tool_calls` espera `Vec<ToolCall>` pero `ChatResult.tool_calls` ya es `Vec<ToolCall>`
```rust
// ✅ CORRECTO - ya coinciden
tool_calls: response.tool_calls,
```

---

### 3. Async/Sync Issues

#### Error: Cannot drop runtime in blocking context
**Ubicación:** `services.rs` `check_service` usando `reqwest::blocking` dentro de async
**Causa:** `reqwest::blocking::get` crea runtime interno que no puede dropearse en contexto async
```rust
// ❌ INCORRECTO
fn check_service(port: u16, health_path: &str) -> bool {
    reqwest::blocking::get(...)
}
```

**Solución:** Usar cliente async nativo
```rust
async fn check_service_async(client: &Client, port: u16, health_path: &str) -> bool {
    client.get(format!("http://127.0.0.1:{}{}", port, health_path)).send().await
}
```

---

### 4. Syn/Quote Issues (Unresolved)

#### Error: `quote::to_string` doesn't exist
**Ubicación:** `tools.rs` líneas 258, 273 (repo_map con syn)
**Causa:** `quote` crate no tiene función `to_string`, usa macro `quote!`
```rust
// ❌ INCORRECTO
quote::to_string(&path)
```

**Solución:** Usar macro `quote!`
```rust
quote!(#path).to_string()
```

#### Error: Borrow checker con syn AST
**Ubicación:** `tools.rs` repo_map (comentado)
**Causa:** `syn::parse_file` devuelve `syn::File` con lifetime, mover `item` en match consume valor
```rust
// Parcialmente resuelto - usar regex en lugar de syn por ahora
```

---

### 5. CLI Argument Parsing

#### Error: Unexpected argument for positional parameter
**Ubicación:** `main.rs` Repomap command
**Causa:** `clap` requiere `#[arg(index = 1)]` para argumentos posicionales
```rust
// ❌ INCORRECTO
path: String,

// ✅ CORRECTO
#[arg(index = 1, default_value = ".")]
path: String,
```

---

## Lecciones Aprendidas

### 1. Rust Ownership en Loops con Self
```rust
// PATRÓN: Extraer datos antes de mutar self
for i in 0..self.items.len() {
    let data = {
        let item = &self.items[i];
        (item.id.clone(), item.name.clone())
    };
    self.items[i].status = "processing".into();
    let result = self.process(data).await?;
    self.items[i].result = Some(result);
}
```

### 2. Manejo de Errores en ChatResult
```rust
// PATRÓN: ChatResult no usa ?, verificar error field
let response = client.chat_messages(...).await;
if let Some(err) = response.error {
    return Err(anyhow::anyhow!("LLM error: {}", err));
}
let content = response.content;
let tools = response.tool_calls;
```

### 3. Register Tool con Closures y Borrow Checker
```rust
// PATRÓN: Clonar nombre para inserción final
fn register_tool(&mut self, name: &str, func: F) {
    let name = name.to_string();
    let name_for_insert = name.clone();  // <-- clave
    let wrapped = if sensitive {
        let name = name.clone();
        Arc::new(move |args| { ... name ... })
    } else { ... };
    self.tools.insert(name_for_insert, wrapped);  // use clone
}
```

### 4. Async Health Checks
```rust
// PATRÓN: No usar reqwest::blocking en código async
async fn check_all(&self, client: &Client) {
    for service in self.services() {
        service.set_running(check_async(client, service.port, &service.health_path).await);
    }
}
```

### 5. Syn/Quote para Análisis de Código
```rust
// LECCIÓN: syn + quote en runtime tiene problemas de lifetime y borrow
// OPCIÓN A: Usar regex para parsing simple (actual)
// OPCIÓN B: Mover a proc-macro crate separado
// OPCIÓN C: Usar prettyplease para pretty-print
```

---

## Errores por Archivo

| Archivo | Errores Encontrados | Estado |
|---------|---------------------|--------|
| `automode.rs` | 8 (E0499, E0502, E0596) | ✅ Resueltos |
| `tools.rs` | 3 (E0382, E0499, syn/quote) | ✅ Parcial (repo_map con regex) |
| `agents.rs` | 12 (E0308, E0277) | ✅ Resueltos |
| `services.rs` | 2 (blocking en async) | ✅ Resueltos (async version) |
| `commands.rs` | 2 (arg index, HashMap) | ✅ Resueltos |
| `main.rs` | 1 (async call) | ✅ Resuelto |
| `config.rs` | 0 (solo warnings) | ✅ OK |
| `llm.rs` | 0 (solo warnings) | ✅ OK |
| `session.rs` | 0 | ✅ OK |
| `thinking.rs` | 0 | ✅ OK |

---

## Métricas Finales

- **Errores de compilación resueltos:** ~35
- **Warnings restantes:** 69 (unused imports, dead code, unused variables)
- **Tests pasando:** 3/3 (safety module)
- **Comandos funcionales:** 25/33 (75%)
- **Comandos placeholder:** 8/33 (25%)

---

## Próximos Pasos Recomendados

1. **Completar `automode.rs`** - Implementar Plan+ReAct+Reflection real
2. **Completar `tools.rs`** - Arreglar repo_map con syn/quote o prettyplease
3. **Completar `agents.rs`** - Probar multi-agente orchestration
4. **Eliminar warnings** - `cargo fix` para unused imports/dead code
5. **Tests de integración** - Con llama.cpp/gateway reales
6. **Benchmarks** - Comparar performance Python vs Rust

---

## Referencias

- [Rust Borrow Checker Patterns](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html)
- [Async Rust Best Practices](https://rust-lang.github.io/async-book/)
- [Syn/Quote Documentation](https://docs.rs/syn/latest/syn/)
- [Clap Derive Tutorial](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html)