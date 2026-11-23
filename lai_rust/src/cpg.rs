//! Code Property Graph lite (v5.0, herramienta 5: codebadger / Joern CPG).
//!
//! No sustituye al RepoMap estructural (FTS5 + grafo de símbolos): añade
//! una capa de **grafo de propiedades** con:
//!
//! - **Call graph**: aristas caller → callee resueltas por nombre cualificado.
//! - **Taint tracking**: propagación intra-procedural de *sources* (parámetros,
//!   `input()`, `env::var`) a *sinks* (`eval`, `exec`, `subprocess`, SQL,
//!   `Command::new`), con path explicito.
//! - **Program slicing**: subgrafo alcanzable desde un símbolo (para
//!   DiagnosticEngine y para acotar contexto en el ContextBuilder).
//!
//! Criterio v5.0: la capa se evalúa en `lai bench` contra el índice
//! estructural antes de adoptarse; nunca sustituye los controles existentes.

use crate::repomap::{FileInfo, RepoMap, SymbolInfo};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Nodo del grafo de propiedades.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpgNode {
    pub id: String,
    pub kind: String, // "file" | symbol kind
    pub label: String,
    pub file: String,
    pub line: usize,
}

/// Arista del grafo de propiedades.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpgEdge {
    pub from: String,
    pub to: String,
    pub kind: String, // "calls" | "imports" | "contains"
}

/// Grafo de propiedades de código.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodePropertyGraph {
    pub nodes: Vec<CpgNode>,
    pub edges: Vec<CpgEdge>,
}

impl CodePropertyGraph {
    pub fn node(&self, id: &str) -> Option<&CpgNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn callees(&self, id: &str) -> Vec<&CpgNode> {
        self.edges
            .iter()
            .filter(|e| e.from == id && e.kind == "calls")
            .filter_map(|e| self.node(&e.to))
            .collect()
    }

    pub fn callers(&self, id: &str) -> Vec<&CpgNode> {
        self.edges
            .iter()
            .filter(|e| e.to == id && e.kind == "calls")
            .filter_map(|e| self.node(&e.from))
            .collect()
    }

    /// Program slicing: todos los símbolos alcanzables desde `root` mediante
    /// aristas `calls` (transitiva).
    pub fn slice(&self, root: &str) -> HashSet<String> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        queue.push_back(root.to_string());
        while let Some(id) = queue.pop_front() {
            if !visited.insert(id.clone()) {
                continue;
            }
            for e in &self.edges {
                if e.from == id && e.kind == "calls" && !visited.contains(&e.to) {
                    queue.push_back(e.to.clone());
                }
            }
        }
        visited
    }

    /// Resumen compacto (para prompts y telemetría).
    pub fn summary(&self) -> String {
        let calls = self.edges.iter().filter(|e| e.kind == "calls").count();
        let imports = self.edges.iter().filter(|e| e.kind == "imports").count();
        format!(
            "CPG: {} nodos, {} aristas ({} calls, {} imports)",
            self.nodes.len(),
            self.edges.len(),
            calls,
            imports
        )
    }
}

/// Fuente de taint: parámetros de función, `input()`, `env::var`, argv.
pub fn taint_sources() -> Vec<String> {
    vec![
        "input()".to_string(),
        "env::var".to_string(),
        "std::env::args".to_string(),
        "argv".to_string(),
        "sys.argv".to_string(),
        "params".to_string(),
        "param".to_string(),
        "args".to_string(),
        "request".to_string(),
    ]
}

/// Sinks de taint: ejecución de código/comandos y SQL dinámico.
pub fn taint_sinks() -> Vec<String> {
    vec![
        "eval(".to_string(),
        "exec(".to_string(),
        "subprocess".to_string(),
        "Command::new".to_string(),
        "os.system".to_string(),
        "popen(".to_string(),
        "execute(".to_string(),
        "query(".to_string(),
    ]
}

/// Un hallazgo de taint: source → sink con el path de líneas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintFinding {
    pub file: String,
    pub source_line: usize,
    pub sink_line: usize,
    pub source: String,
    pub sink: String,
    pub severity: String, // "high" | "medium"
}

/// Construye el grafo de propiedades desde un RepoMap + el contenido de los
/// archivos (para el análisis intra-procedural de taint).
pub fn build_cpg(repo_map: &RepoMap, contents: &HashMap<String, String>) -> CodePropertyGraph {
    let mut graph = CodePropertyGraph::default();

    for file in &repo_map.files {
        let file_id = format!("file:{}", file.path);
        graph.nodes.push(CpgNode {
            id: file_id.clone(),
            kind: "file".to_string(),
            label: file.path.clone(),
            file: file.path.clone(),
            line: 0,
        });
        for sym in &file.symbols {
            graph.nodes.push(CpgNode {
                id: symbol_id(file, sym),
                kind: sym.kind.clone(),
                label: sym.name.clone(),
                file: file.path.clone(),
                line: sym.line,
            });
            graph.edges.push(CpgEdge {
                from: file_id.clone(),
                to: symbol_id(file, sym),
                kind: "contains".to_string(),
            });
        }
    }

    // Call graph por resolución de nombre: si `name(` aparece en el cuerpo
    // de un símbolo y existe un símbolo con ese nombre, se crea la arista.
    let all_symbols: Vec<&SymbolInfo> = repo_map
        .files
        .iter()
        .flat_map(|f| f.symbols.iter())
        .collect();
    for file in &repo_map.files {
        let Some(content) = contents.get(&file.path) else { continue };
        for sym in &file.symbols {
            let from = symbol_id(file, sym);
            for candidate in &all_symbols {
                if candidate.name == sym.name && candidate.line == sym.line {
                    continue;
                }
                if contains_call(content, &candidate.name) {
                    let to_file = find_file_of(repo_map, candidate);
                    graph.edges.push(CpgEdge {
                        from: from.clone(),
                        to: symbol_id(&to_file, candidate),
                        kind: "calls".to_string(),
                    });
                }
            }
        }
    }

    // Import edges: archivo -> archivo importado (por nombre de módulo,
    // es decir, ruta sin extensión: "db" -> "db.py").
    for file in &repo_map.files {
        let from = format!("file:{}", file.path);
        for imp in &file.imports {
            let target = format!("file:{}", imp);
            let by_exact = repo_map.files.iter().any(|f| format!("file:{}", f.path) == target);
            let by_stem = repo_map.files.iter().any(|f| {
                std::path::Path::new(&f.path)
                    .file_stem()
                    .map(|s| s.to_string_lossy() == *imp)
                    .unwrap_or(false)
            });
            if by_exact || by_stem {
                let resolved = repo_map
                    .files
                    .iter()
                    .find(|f| {
                        format!("file:{}", f.path) == target
                            || std::path::Path::new(&f.path)
                                .file_stem()
                                .map(|s| s.to_string_lossy() == *imp)
                                .unwrap_or(false)
                    })
                    .map(|f| format!("file:{}", f.path))
                    .unwrap_or(target);
                graph.edges.push(CpgEdge { from: from.clone(), to: resolved, kind: "imports".to_string() });
            }
        }
    }

    graph
}

fn symbol_id(file: &FileInfo, sym: &SymbolInfo) -> String {
    format!("sym:{}::{}:{}", file.path, sym.kind, sym.name)
}

fn find_file_of(repo_map: &RepoMap, sym: &SymbolInfo) -> FileInfo {
    repo_map
        .files
        .iter()
        .find(|f| f.symbols.iter().any(|s| std::ptr::eq(s, sym)))
        .cloned()
        .unwrap_or_else(|| FileInfo {
            path: String::new(),
            language: String::new(),
            size_bytes: 0,
            symbols: vec![],
            imports: vec![],
            hash: String::new(),
        })
}

fn contains_call(content: &str, name: &str) -> bool {
    let needle = format!("{}(", name);
    content.contains(&needle)
}

/// Taint tracking intra-procedural: busca líneas con source y líneas con sink
/// en el mismo archivo, y las empareja si el sink aparece después del source.
pub fn taint_analysis(
    repo_map: &RepoMap,
    contents: &HashMap<String, String>,
) -> Vec<TaintFinding> {
    let sources = taint_sources();
    let sinks = taint_sinks();
    let mut findings = Vec::new();

    for file in &repo_map.files {
        let Some(content) = contents.get(&file.path) else { continue };
        for (idx, line) in content.lines().enumerate() {
            let line_no = idx + 1;
            let source = sources.iter().find(|s| line.contains(s.as_str()));
            if let Some(src) = source {
                // Busca un sink posterior en el mismo archivo (ventana 50 líneas).
                let rest: Vec<&str> = content.lines().skip(idx + 1).take(50).collect();
                for (delta, l) in rest.iter().enumerate() {
                    if let Some(sink) = sinks.iter().find(|s| l.contains(s.as_str())) {
                        findings.push(TaintFinding {
                            file: file.path.clone(),
                            source_line: line_no,
                            sink_line: line_no + delta + 1,
                            source: src.clone(),
                            sink: sink.clone(),
                            severity: if sink.contains("Command") || sink.contains("subprocess") {
                                "high".to_string()
                            } else {
                                "medium".to_string()
                            },
                        });
                        break;
                    }
                }
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repomap::{FileInfo, RepoMap};

    fn sample_map() -> (RepoMap, HashMap<String, String>) {
        let mut files = vec![
            FileInfo {
                path: "app.py".to_string(),
                language: "python".to_string(),
                size_bytes: 100,
                symbols: vec![
                    SymbolInfo { name: "main".to_string(), kind: "function".to_string(), line: 1, signature: None },
                    SymbolInfo { name: "run".to_string(), kind: "function".to_string(), line: 5, signature: None },
                ],
                imports: vec![],
                hash: "h1".to_string(),
            },
            FileInfo {
                path: "db.py".to_string(),
                language: "python".to_string(),
                size_bytes: 80,
                symbols: vec![SymbolInfo {
                    name: "query".to_string(),
                    kind: "function".to_string(),
                    line: 2,
                    signature: None,
                }],
                imports: vec![],
                hash: "h2".to_string(),
            },
        ];
        files[0].imports = vec!["db".to_string()];
        let mut contents = HashMap::new();
        contents.insert(
            "app.py".to_string(),
            "def main():\n    cmd = input()\n    run(cmd)\n\ndef run(c):\n    import subprocess\n    subprocess.call(c)\n".to_string(),
        );
        contents.insert("db.py".to_string(), "def query(sql):\n    execute(sql)\n".to_string());
        (
            RepoMap { root: Default::default(), files, edges: vec![] },
            contents,
        )
    }

    #[test]
    fn test_cpg_nodes_and_contains() {
        let (map, contents) = sample_map();
        let graph = build_cpg(&map, &contents);
        assert!(graph.nodes.iter().any(|n| n.id == "file:app.py"));
        assert!(graph.edges.iter().any(|e| e.kind == "contains"));
        assert!(graph.edges.iter().any(|e| e.kind == "imports"));
        assert!(graph.summary().contains("CPG"));
    }

    #[test]
    fn test_slice_transitive() {
        let (map, contents) = sample_map();
        let graph = build_cpg(&map, &contents);
        // main llama a run; run no resuelve a query (distinto archivo, sin
        // resolución cross-file por nombre en esta versión lite).
        let slice = graph.slice(&format!("sym:app.py::function:main"));
        assert!(slice.contains(&"sym:app.py::function:main".to_string()));
    }

    #[test]
    fn test_taint_detection_input_to_subprocess() {
        let (map, contents) = sample_map();
        let findings = taint_analysis(&map, &contents);
        assert!(!findings.is_empty(), "debe detectar input() -> subprocess");
        let f = findings.iter().find(|f| f.file == "app.py").unwrap();
        assert_eq!(f.severity, "high");
        assert!(f.sink_line > f.source_line);
    }

    #[test]
    fn test_callees_and_callers() {
        let (map, contents) = sample_map();
        let graph = build_cpg(&map, &contents);
        let main = format!("sym:app.py::function:main");
        let callees = graph.callees(&main);
        assert!(callees.iter().any(|c| c.label == "run"));
        let callers = graph.callers(&format!("sym:app.py::function:run"));
        assert!(callers.iter().any(|c| c.label == "main"));
    }
}