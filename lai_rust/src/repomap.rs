//! repomap.rs - Índice estructural del repositorio.
//! Extrae símbolos (funciones, structs, clases, imports) mediante AST
//! (syn para Rust) y heurísticas para Python. Provee resumen compacto
//! para prompts y búsqueda de archivos/símbolos.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Información de un símbolo del código.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub name: String,
    /// "function" | "struct" | "enum" | "trait" | "mod" | "impl" | "class" | "method"
    pub kind: String,
    pub line: usize,
    pub signature: Option<String>,
}

/// Información de un archivo indexado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// Ruta relativa al root del repositorio.
    pub path: String,
    /// "rust" | "python" | "other"
    pub language: String,
    pub size_bytes: u64,
    pub symbols: Vec<SymbolInfo>,
    pub imports: Vec<String>,
}

/// Mapa estructural del repositorio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoMap {
    pub root: PathBuf,
    pub files: Vec<FileInfo>,
    /// Aristas de dependencias (importador, importado).
    pub edges: Vec<(String, String)>,
}

impl RepoMap {
    /// Resumen compacto del repositorio para prompts de planificación.
    pub fn summary(&self) -> String {
        let rust = self.files.iter().filter(|f| f.language == "rust").count();
        let python = self.files.iter().filter(|f| f.language == "python").count();
        let other = self.files.len() - rust - python;
        let total_symbols: usize = self.files.iter().map(|f| f.symbols.len()).sum();
        let total_imports: usize = self.files.iter().map(|f| f.imports.len()).sum();

        let mut top_dirs: Vec<(String, usize)> = Vec::new();
        for f in &self.files {
            let dir = f.path.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_else(|| ".".to_string());
            if let Some(entry) = top_dirs.iter_mut().find(|(d, _)| *d == dir) {
                entry.1 += 1;
            } else {
                top_dirs.push((dir, 1));
            }
        }
        top_dirs.sort_by(|a, b| b.1.cmp(&a.1));
        let top_str: Vec<String> = top_dirs.iter().take(10).map(|(d, n)| format!("{} ({} arch.)", d, n)).collect();

        format!(
            "RepoMap: {} archivos [rust={}, python={}, otros={}], {} símbolos, {} imports.\nDirectorios principales: {}",
            self.files.len(), rust, python, other, total_symbols, total_imports,
            if top_str.is_empty() { "(ninguno)".to_string() } else { top_str.join(", ") }
        )
    }

    /// Busca archivos cuya ruta o nombre contenga el patrón (case-insensitive).
    pub fn find_files(&self, pattern: &str) -> Vec<String> {
        let p = pattern.to_lowercase();
        let mut out: Vec<String> = self
            .files
            .iter()
            .filter(|f| {
                let path_l = f.path.to_lowercase();
                path_l.contains(&p)
                    || f.path
                        .rsplit('/')
                        .next()
                        .map(|n| n.to_lowercase().contains(&p))
                        .unwrap_or(false)
            })
            .map(|f| f.path.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Busca símbolos por nombre exacto.
    pub fn find_symbols(&self, name: &str) -> Vec<&SymbolInfo> {
        let mut out = Vec::new();
        for f in &self.files {
            for s in &f.symbols {
                if s.name == name {
                    out.push(s);
                }
            }
        }
        out
    }
}

const SKIP_DIRS: &[&str] = &[
    ".git", "target", "node_modules", "__pycache__", ".venv", "venv", "dist", "build", ".mypy_cache",
    ".pytest_cache", ".benchmarks", "sessions", "logs",
];

const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

/// Construye el mapa estructural del repositorio en `root`.
pub fn build_repo_map(root: &Path) -> Result<RepoMap> {
    let mut files = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !SKIP_DIRS.contains(&name.as_ref()) && !name.starts_with(".git")
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.len() > MAX_FILE_SIZE {
            continue;
        }
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let language = match ext {
            "rs" => "rust",
            "py" => "python",
            _ => continue,
        };
        let code = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let (symbols, imports) = match language {
            "rust" => (parse_rust(&code), parse_imports(&code, &["use ", "extern crate "])),
            _ => (parse_python(&code), parse_imports(&code, &["import ", "from "])),
        };
        files.push(FileInfo {
            path: rel,
            language: language.to_string(),
            size_bytes: meta.len(),
            symbols,
            imports,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    let edges = build_edges(&files);
    Ok(RepoMap { root: root.to_path_buf(), files, edges })
}

fn build_edges(files: &[FileInfo]) -> Vec<(String, String)> {
    let mut edges = Vec::new();
    for f in files {
        for imp in &f.imports {
            let base = imp.rsplit("::").last().unwrap_or(imp).rsplit('.').last().unwrap_or(imp);
            for target in files {
                if target.path == f.path {
                    continue;
                }
                let stem = target
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&target.path)
                    .rsplit('.')
                    .next()
                    .unwrap_or("");
                if !stem.is_empty() && base.contains(stem) {
                    edges.push((f.path.clone(), target.path.clone()));
                }
            }
        }
    }
    edges.sort();
    edges.dedup();
    edges
}

fn parse_imports(code: &str, prefixes: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for line in code.lines() {
        let t = line.trim();
        for p in prefixes {
            if let Some(rest) = t.strip_prefix(p) {
                let name: String = rest
                    .split(|c: char| c == ';' || c == ' ' || c == '{' || c == ',')
                    .filter(|s| !s.is_empty())
                    .take(1)
                    .collect::<Vec<_>>()
                    .join("");
                if !name.is_empty() {
                    out.push(name);
                }
                break;
            }
        }
    }
    out
}

fn parse_rust(code: &str) -> Vec<SymbolInfo> {
    let mut out = Vec::new();
    let parsed = syn::parse_file(code);
    if let Ok(file) = parsed {
        for item in &file.items {
            match item {
                syn::Item::Fn(f) => {
                    let name = f.sig.ident.to_string();
                    let line = find_line(code, &format!("fn {}", name));
                    out.push(SymbolInfo {
                        signature: Some(sig_to_string(&f.sig)),
                        name,
                        kind: "function".to_string(),
                        line,
                    });
                }
                syn::Item::Struct(s) => {
                    let name = s.ident.to_string();
                    let line = find_line(code, &format!("struct {}", name));
                    out.push(SymbolInfo { signature: None, name, kind: "struct".to_string(), line });
                }
                syn::Item::Enum(e) => {
                    let name = e.ident.to_string();
                    let line = find_line(code, &format!("enum {}", name));
                    out.push(SymbolInfo { signature: None, name, kind: "enum".to_string(), line });
                }
                syn::Item::Trait(t) => {
                    let name = t.ident.to_string();
                    let line = find_line(code, &format!("trait {}", name));
                    out.push(SymbolInfo { signature: None, name, kind: "trait".to_string(), line });
                }
                syn::Item::Mod(m) => {
                    let name = m.ident.to_string();
                    let line = find_line(code, &format!("mod {}", name));
                    out.push(SymbolInfo { signature: None, name, kind: "mod".to_string(), line });
                }
                syn::Item::Impl(i) => {
                    for item in &i.items {
                        if let syn::ImplItem::Fn(m) = item {
                            let name = m.sig.ident.to_string();
                            let line = find_line(code, &format!("fn {}", name));
                            out.push(SymbolInfo {
                                signature: Some(sig_to_string(&m.sig)),
                                name,
                                kind: "method".to_string(),
                                line,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn parse_python(code: &str) -> Vec<SymbolInfo> {
    let mut out = Vec::new();
    for (idx, line) in code.lines().enumerate() {
        let t = line.trim();
        if t.starts_with("def ") || t.starts_with("async def ") {
            if let Some(name) = t
                .split(|c: char| c == '(' || c == ' ' || c == ':')
                .find(|s| !s.is_empty() && *s != "def" && *s != "async")
            {
                out.push(SymbolInfo {
                    name: name.to_string(),
                    kind: "function".to_string(),
                    line: idx + 1,
                    signature: Some(t.to_string()),
                });
            }
        } else if t.starts_with("class ") {
            let name = t
                .split(|c: char| c == '(' || c == ':' || c == ' ')
                .find(|s| !s.is_empty() && *s != "class")
                .unwrap_or("")
                .to_string();
            if !name.is_empty() {
                out.push(SymbolInfo {
                    name,
                    kind: "class".to_string(),
                    line: idx + 1,
                    signature: Some(t.to_string()),
                });
            }
        }
    }
    out
}

fn sig_to_string(sig: &syn::Signature) -> String {
    quote::quote!(#sig).to_string()
}

fn find_line(code: &str, needle: &str) -> usize {
    for (idx, line) in code.lines().enumerate() {
        if line.contains(needle) {
            return idx + 1;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust() {
        let code = "pub fn hello() -> i32 { 42 }\nstruct Foo { x: i32 }\nfn main() {}";
        let symbols = parse_rust(code);
        assert!(symbols.iter().any(|s| s.name == "hello" && s.kind == "function"));
        assert!(symbols.iter().any(|s| s.name == "Foo" && s.kind == "struct"));
        assert!(symbols.iter().any(|s| s.name == "main" && s.kind == "function"));
    }

    #[test]
    fn test_parse_python() {
        let code = "import os\n\ndef foo(x):\n    return x\n\nclass Bar:\n    def method(self):\n        pass";
        let symbols = parse_python(code);
        assert!(symbols.iter().any(|s| s.name == "foo"));
        assert!(symbols.iter().any(|s| s.name == "Bar"));
    }

    #[test]
    fn test_find_files() {
        let map = RepoMap {
            root: PathBuf::from("."),
            files: vec![
                FileInfo { path: "src/main.rs".into(), language: "rust".into(), size_bytes: 1, symbols: vec![], imports: vec![] },
                FileInfo { path: "src/lib.py".into(), language: "python".into(), size_bytes: 1, symbols: vec![], imports: vec![] },
            ],
            edges: vec![],
        };
        assert_eq!(map.find_files("main"), vec!["src/main.rs".to_string()]);
        assert!(map.find_files("nomatch").is_empty());
    }

    #[test]
    fn test_summary() {
        let map = RepoMap {
            root: PathBuf::from("."),
            files: vec![FileInfo { path: "a.rs".into(), language: "rust".into(), size_bytes: 1, symbols: vec![], imports: vec![] }],
            edges: vec![],
        };
        assert!(map.summary().contains("1 archivos"));
    }
}