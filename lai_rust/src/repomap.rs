use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Directorios que nunca aportan símbolos útiles y pueden ser enormes.
pub fn is_skipped_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".hg"
            | ".svn"
            | "target"
            | "node_modules"
            | "__pycache__"
            | ".venv"
            | "venv"
            | "env"
            | ".tox"
            | ".mypy_cache"
            | ".pytest_cache"
            | ".ruff_cache"
            | "dist"
            | "build"
            | ".next"
            | ".cache"
            | "coverage"
            | "vendor"
    )
}

/// Extrae declaraciones de Python y Rust de un archivo.
///
/// Antes el `repo_map` de `tools.rs` cortaba en el byte 4 (`async fn foo` se
/// convertía en `fn c fn foo`) y no veía `pub fn`/`pub struct`/`enum`/`trait`.
/// Una sola implementación evita que las dos rutas diverjan.
pub fn extract_symbols(content: &str) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();

    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        let (kind, rest) = if let Some(r) = trimmed.strip_prefix("pub async fn ") {
            ("fn", r)
        } else if let Some(r) = trimmed.strip_prefix("async fn ") {
            ("fn", r)
        } else if let Some(r) = trimmed.strip_prefix("pub fn ") {
            ("fn", r)
        } else if let Some(r) = trimmed.strip_prefix("fn ") {
            ("fn", r)
        } else if let Some(r) = trimmed.strip_prefix("pub struct ") {
            ("struct", r)
        } else if let Some(r) = trimmed.strip_prefix("struct ") {
            ("struct", r)
        } else if let Some(r) = trimmed.strip_prefix("pub enum ") {
            ("enum", r)
        } else if let Some(r) = trimmed.strip_prefix("enum ") {
            ("enum", r)
        } else if let Some(r) = trimmed.strip_prefix("pub trait ") {
            ("trait", r)
        } else if let Some(r) = trimmed.strip_prefix("trait ") {
            ("trait", r)
        } else if let Some(r) = trimmed.strip_prefix("async def ") {
            ("def", r)
        } else if let Some(r) = trimmed.strip_prefix("def ") {
            ("def", r)
        } else if let Some(r) = trimmed.strip_prefix("class ") {
            ("class", r)
        } else {
            continue;
        };

        let name = rest
            .split(|c: char| c == '(' || c == '{' || c == '<' || c == ':' || c.is_whitespace())
            .next()
            .unwrap_or("")
            .trim();
        if !name.is_empty() {
            symbols.push(SymbolInfo {
                kind: kind.to_string(),
                name: name.to_string(),
                line: index + 1,
            });
        }
    }

    symbols
}
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub language: Option<String>,
    pub symbols: Vec<SymbolInfo>,
}
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub kind: String,
    pub name: String,
    pub line: usize,
}
pub struct RepoMap {
    pub files: Vec<FileInfo>,
    pub root: PathBuf,
}
impl RepoMap {
    pub fn new(root: &Path) -> Self {
        Self {
            files: Vec::new(),
            root: root.to_path_buf(),
        }
    }
    pub fn build(root: &Path) -> Self {
        let mut map = Self::new(root);
        for entry in WalkDir::new(root)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| {
                !e.file_type().is_dir() || !is_skipped_dir(&e.file_name().to_string_lossy())
            })
            .filter_map(|e| e.ok())
        {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "py" && ext != "rs" {
                continue;
            }
            let language = match ext {
                "py" => Some("python".to_string()),
                "rs" => Some("rust".to_string()),
                _ => None,
            };
            let symbols = fs::read_to_string(p)
                .map(|content| extract_symbols(&content))
                .unwrap_or_default();
            map.files.push(FileInfo {
                path: p.to_path_buf(),
                language,
                symbols,
            });
        }
        map
    }
    pub fn to_text(&self, max_chars: usize) -> String {
        let mut blocks = Vec::new();
        let mut used = 0;
        for file in &self.files {
            if file.symbols.is_empty() {
                continue;
            }
            let rel_path = file.path.strip_prefix(&self.root).unwrap_or(&file.path);
            let block = format!(
                "{}:\n{}",
                rel_path.display(),
                file.symbols
                    .iter()
                    .map(|s| format!("  {} {}", s.kind, s.name))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let bl = block.len();
            if used + bl > max_chars {
                blocks.push("... (truncated)".to_string());
                break;
            }
            blocks.push(block);
            used += bl;
        }
        if blocks.is_empty() {
            "(no symbols found)".to_string()
        } else {
            blocks.join("\n\n")
        }
    }
    pub fn find_symbol(&self, name: &str) -> Vec<&FileInfo> {
        self.files
            .iter()
            .filter(|f| f.symbols.iter().any(|s| s.name == name))
            .collect()
    }
    pub fn files_by_language(&self, language: &str) -> Vec<&FileInfo> {
        self.files
            .iter()
            .filter(|f| f.language.as_deref() == Some(language))
            .collect()
    }
}
impl Default for RepoMap {
    fn default() -> Self {
        Self::new(Path::new("."))
    }
}
