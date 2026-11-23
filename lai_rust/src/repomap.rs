//! RepoMap - Índice estructural del repositorio (Regla 2).
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub language: Option<String>,
    pub symbols: Vec<SymbolInfo>,
    pub imports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        Self { files: Vec::new(), root: root.to_path_buf() }
    }

    pub fn build(root: &Path) -> Self {
        let mut map = Self::new(root);
        let skip_dirs = ["__pycache__", "node_modules", ".git", "venv", "env", ".venv", ".ruff_cache", "target"];
        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if !p.is_file() { continue; }
            if skip_dirs.iter().any(|s| p.components().any(|c| c.as_os_str().to_string_lossy() == *s)) { continue; }
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "py" && ext != "rs" { continue; }
            let language = match ext { "py" => Some("python".to_string()), "rs" => Some("rust".to_string()), _ => None };
            let mut symbols = Vec::new();
            if let Ok(content) = fs::read_to_string(p) {
                for (i, line) in content.lines().enumerate() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("def ") || trimmed.starts_with("fn ") || trimmed.starts_with("async fn ") {
                        if let Some(ne) = trimmed.find('(') {
                            symbols.push(SymbolInfo { kind: "fn".into(), name: trimmed[4..ne].trim().to_string(), line: i + 1 });
                        }
                    } else if trimmed.starts_with("class ") || trimmed.starts_with("struct ") {
                        if let Some(ne) = trimmed.find(|c| c == ':' || c == '(') {
                            let kind = if trimmed.starts_with("class ") { "class" } else { "struct" };
                            symbols.push(SymbolInfo { kind: kind.into(), name: trimmed[6..ne].trim().to_string(), line: i + 1 });
                        }
                    }
                }
            }
            map.files.push(FileInfo { path: p.to_path_buf(), language, symbols, imports: vec![] });
        }
        map
    }

    pub fn summary(&self) -> String {
        let rust = self.files.iter().filter(|f| f.language.as_deref() == Some("rust")).count();
        let python = self.files.iter().filter(|f| f.language.as_deref() == Some("python")).count();
        let other = self.files.len() - rust - python;
        let total_symbols: usize = self.files.iter().map(|f| f.symbols.len()).sum();
        let total_imports: usize = self.files.iter().map(|f| f.imports.len()).sum();

        let mut top_dirs: Vec<(String, usize)> = Vec::new();
        for f in &self.files {
            let dir = f.path.strip_prefix(&self.root).unwrap_or(&f.path)
                .to_string_lossy().rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_else(|| ".".to_string());
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

    pub fn to_text(&self, max_chars: usize) -> String {
        let mut blocks = Vec::new();
        let mut used = 0;
        for file in &self.files {
            if file.symbols.is_empty() { continue; }
            let rel_path = file.path.strip_prefix(&self.root).unwrap_or(&file.path);
            let block = format!("{}:\n{}", rel_path.display(), file.symbols.iter().map(|s| format!("  {} {}", s.kind, s.name)).collect::<Vec<_>>().join("\n"));
            let bl = block.len();
            if used + bl > max_chars { blocks.push("... (truncated)".to_string()); break; }
            blocks.push(block); used += bl;
        }
        if blocks.is_empty() { "(no symbols found)".to_string() } else { blocks.join("\n\n") }
    }

    pub fn find_symbol(&self, name: &str) -> Vec<&FileInfo> { self.files.iter().filter(|f| f.symbols.iter().any(|s| s.name == name)).collect() }
    pub fn files_by_language(&self, language: &str) -> Vec<&FileInfo> { self.files.iter().filter(|f| f.language.as_deref() == Some(language)).collect() }
}

impl Default for RepoMap { fn default() -> Self { Self::new(Path::new(".")) } }

/// Free function wrapper for building a RepoMap (compatibility with context.rs)
pub fn build_repo_map(root: &Path) -> anyhow::Result<RepoMap> {
    Ok(RepoMap::build(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_repomap_build() {
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file = temp_dir.path().join("test.py");
        fs::write(&test_file, "def hello():\n    pass\n\nclass MyClass:\n    def method(self):\n        pass\n").unwrap();

        let repo_map = RepoMap::build(temp_dir.path());
        let summary = repo_map.summary();
        assert!(summary.contains("test.py"));
        assert!(summary.contains("hello"));
        assert!(summary.contains("MyClass"));
    }

    #[test]
    fn test_summary() {
        let repo_map = RepoMap {
            root: PathBuf::from("."),
            files: vec![FileInfo { path: PathBuf::from("test.py"), language: Some("python".to_string()), symbols: vec![], imports: vec![] }],
        };
        let summary = repo_map.summary();
        assert!(summary.contains("Total files: 1"));
    }
}