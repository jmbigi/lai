//! context.rs - Enhanced context understanding for lai
//! Provides deep project understanding: code structure, documentation, patterns, objectives

use crate::tools::ToolRegistry;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Comprehensive project context with deep understanding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContext {
    /// Basic project info
    pub name: String,
    pub root_path: PathBuf,
    pub project_type: ProjectType,

    /// Documentation
    pub readme: Option<String>,
    pub agents_md: Option<String>,
    pub claude_md: Option<String>,
    pub contributing_md: Option<String>,
    pub changelog_md: Option<String>,

    /// Configuration files
    pub config_files: HashMap<String, String>,

    /// Code structure analysis
    pub code_structure: CodeStructure,

    /// Documentation files found
    pub docs: Vec<DocFile>,

    /// Project goals/objectives (extracted from docs)
    pub objectives: Vec<Objective>,

    /// Key patterns detected
    pub patterns: Vec<CodePattern>,

    /// Roles and responsibilities
    pub roles: Vec<Role>,

    /// Tech stack detected
    pub tech_stack: TechStack,
}

/// Project type classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProjectType {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    Cpp,
    CSharp,
    Mixed,
    Unknown,
}

/// Code structure analysis
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodeStructure {
    pub total_files: usize,
    pub total_lines: usize,
    pub languages: HashMap<String, LanguageStats>,
    pub modules: Vec<ModuleInfo>,
    pub entry_points: Vec<String>,
    pub test_files: Vec<String>,
    pub config_files: Vec<String>,
}

/// Language-specific statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageStats {
    pub files: usize,
    pub lines: usize,
    pub functions: usize,
    pub classes: usize,
    pub imports: Vec<String>,
}

/// Module information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub name: String,
    pub path: PathBuf,
    pub description: Option<String>,
    pub functions: Vec<String>,
    pub classes: Vec<String>,
    pub is_test: bool,
    pub is_entry: bool,
}

/// Documentation file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocFile {
    pub path: PathBuf,
    pub title: String,
    pub summary: String,
    pub content_type: DocType,
}

/// Documentation type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DocType {
    Readme,
    Architecture,
    Api,
    Guide,
    Tutorial,
    Changelog,
    Contributing,
    License,
    Other,
}

/// Project objective/goal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Objective {
    pub title: String,
    pub description: String,
    pub priority: Priority,
    pub source: String, // e.g., "README.md", "AGENTS.md"
}

/// Priority level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Critical,
    High,
    Medium,
    Low,
}

/// Code pattern detected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePattern {
    pub name: String,
    pub description: String,
    pub examples: Vec<String>,
    pub files: Vec<PathBuf>,
}

/// Role in the project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub name: String,
    pub description: String,
    pub responsibilities: Vec<String>,
    pub files: Vec<PathBuf>,
}

/// Technology stack
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TechStack {
    pub languages: Vec<String>,
    pub frameworks: Vec<String>,
    pub databases: Vec<String>,
    pub tools: Vec<String>,
    pub ci_cd: Vec<String>,
}

/// Enhanced context analyzer
pub struct ContextAnalyzer {
    project_root: PathBuf,
    tool_registry: ToolRegistry,
}

impl ContextAnalyzer {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            tool_registry: ToolRegistry::new(),
        }
    }

    /// Perform deep project analysis
    pub fn analyze(&self) -> Result<ProjectContext> {
        let mut ctx = ProjectContext {
            name: self.project_root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            root_path: self.project_root.clone(),
            project_type: ProjectType::Unknown,
            readme: None,
            agents_md: None,
            claude_md: None,
            contributing_md: None,
            changelog_md: None,
            config_files: HashMap::new(),
            code_structure: CodeStructure::default(),
            docs: Vec::new(),
            objectives: Vec::new(),
            patterns: Vec::new(),
            roles: Vec::new(),
            tech_stack: TechStack::default(),
        };

        // 1. Read documentation files
        self.read_documentation(&mut ctx)?;

        // 2. Read configuration files
        self.read_config_files(&mut ctx)?;

        // 3. Analyze code structure
        self.analyze_code_structure(&mut ctx)?;

        // 4. Detect tech stack
        self.detect_tech_stack(&mut ctx)?;

        // 5. Determine project type
        ctx.project_type = self.determine_project_type(&ctx);

        // 6. Extract objectives from documentation
        self.extract_objectives(&mut ctx)?;

        // 7. Detect code patterns
        self.detect_patterns(&mut ctx)?;

        // 8. Identify roles
        self.identify_roles(&mut ctx)?;

        Ok(ctx)
    }

    fn read_documentation(&self, ctx: &mut ProjectContext) -> Result<()> {
        // README files
        for name in ["README.md", "README.txt", "README.rst", "readme.md"] {
            let path = ctx.root_path.join(name);
            if path.exists() {
                ctx.readme = Some(fs::read_to_string(&path)?);
                ctx.docs.push(DocFile {
                    path: path.clone(),
                    title: "README".to_string(),
                    summary: self.extract_summary(&ctx.readme.as_ref().unwrap()),
                    content_type: DocType::Readme,
                });
                break;
            }
        }

        // AGENTS.md
        let agents_path = ctx.root_path.join("AGENTS.md");
        if agents_path.exists() {
            ctx.agents_md = Some(fs::read_to_string(&agents_path)?);
            ctx.docs.push(DocFile {
                path: agents_path,
                title: "Agent Instructions".to_string(),
                summary: "Agent instructions and rules".to_string(),
                content_type: DocType::Guide,
            });
        }

        // CLAUDE.md
        let claude_path = ctx.root_path.join("CLAUDE.md");
        if claude_path.exists() {
            ctx.claude_md = Some(fs::read_to_string(&claude_path)?);
            ctx.docs.push(DocFile {
                path: claude_path,
                title: "Claude Instructions".to_string(),
                summary: "Claude Code instructions".to_string(),
                content_type: DocType::Guide,
            });
        }

        // CONTRIBUTING.md
        let contrib_path = ctx.root_path.join("CONTRIBUTING.md");
        if contrib_path.exists() {
            ctx.contributing_md = Some(fs::read_to_string(&contrib_path)?);
            ctx.docs.push(DocFile {
                path: contrib_path,
                title: "Contributing Guide".to_string(),
                summary: self.extract_summary(&ctx.contributing_md.as_ref().unwrap()),
                content_type: DocType::Guide,
            });
        }

        // CHANGELOG.md
        let changelog_path = ctx.root_path.join("CHANGELOG.md");
        if changelog_path.exists() {
            ctx.changelog_md = Some(fs::read_to_string(&changelog_path)?);
            ctx.docs.push(DocFile {
                path: changelog_path,
                title: "Changelog".to_string(),
                summary: "Project changelog".to_string(),
                content_type: DocType::Changelog,
            });
        }

        Ok(())
    }

    fn read_config_files(&self, ctx: &mut ProjectContext) -> Result<()> {
        let config_files = [
            ("pyproject.toml", "Python"),
            ("setup.py", "Python"),
            ("setup.cfg", "Python"),
            ("requirements.txt", "Python"),
            ("Cargo.toml", "Rust"),
            ("go.mod", "Go"),
            ("package.json", "JavaScript/TypeScript"),
            ("tsconfig.json", "TypeScript"),
            ("pom.xml", "Java"),
            ("build.gradle", "Java/Gradle"),
            ("Dockerfile", "Docker"),
            ("docker-compose.yml", "Docker"),
            ("Makefile", "Make"),
            (".gitignore", "Git"),
        ];

        for (name, _lang) in config_files {
            let path = ctx.root_path.join(name);
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    ctx.config_files.insert(name.to_string(), content);
                }
            }
        }
        Ok(())
    }

    fn analyze_code_structure(&self, ctx: &mut ProjectContext) -> Result<()> {
        let mut total_files = 0;
        let mut total_lines = 0;
        let mut languages: HashMap<String, LanguageStats> = HashMap::new();
        let mut modules = Vec::new();
        let mut entry_points = Vec::new();
        let mut test_files = Vec::new();
        let mut config_files = Vec::new();

        for entry in WalkDir::new(&ctx.root_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let path = entry.path();
            let rel_path = path.strip_prefix(&ctx.root_path).unwrap_or(path);

            // Skip hidden, target, node_modules, .git, etc.
            if self.should_skip(path) {
                continue;
            }

            total_files += 1;

            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let lang = self.extension_to_language(ext);
                let stats = languages.entry(lang.to_string()).or_insert(LanguageStats {
                    files: 0,
                    lines: 0,
                    functions: 0,
                    classes: 0,
                    imports: Vec::new(),
                });

                stats.files += 1;

                if let Ok(content) = fs::read_to_string(path) {
                    let line_count = content.lines().count();
                    stats.lines += line_count;
                    total_lines += line_count;

                    // Analyze code based on language
                    self.analyze_file_content(&content, lang, &mut modules, &mut entry_points, &rel_path);
                }

                // Detect test files
                if self.is_test_file(&rel_path) {
                    test_files.push(rel_path.to_string_lossy().to_string());
                }
            }
        }

        ctx.code_structure = CodeStructure {
            total_files,
            total_lines,
            languages,
            modules,
            entry_points,
            test_files,
            config_files,
        };

        Ok(())
    }

    fn should_skip(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        path_str.contains("/.git/")
            || path_str.contains("/target/")
            || path_str.contains("/node_modules/")
            || path_str.contains("/.venv/")
            || path_str.contains("/__pycache__/")
            || path_str.contains("/build/")
            || path_str.contains("/dist/")
            || path_str.contains("/.next/")
            || path_str.contains("/.git/")
            || path_str.contains(".min.js")
            || path_str.contains(".min.css")
    }

    fn extension_to_language(&self, ext: &str) -> &str {
        match ext {
            "rs" => "Rust",
            "py" => "Python",
            "js" | "jsx" => "JavaScript",
            "ts" | "tsx" => "TypeScript",
            "go" => "Go",
            "java" => "Java",
            "cpp" | "cc" | "cxx" => "C++",
            "c" => "C",
            "cs" => "C#",
            "rb" => "Ruby",
            "php" => "PHP",
            "swift" => "Swift",
            "kt" => "Kotlin",
            "scala" => "Scala",
            "clj" => "Clojure",
            "hs" => "Haskell",
            "ml" => "OCaml",
            "fs" => "F#",
            "vue" => "Vue",
            "svelte" => "Svelte",
            _ => "Other",
        }
    }

    fn analyze_file_content(
        &self,
        content: &str,
        lang: &str,
        modules: &mut Vec<ModuleInfo>,
        entry_points: &mut Vec<String>,
        rel_path: &Path,
    ) {
        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            // Language-specific analysis
            match lang {
                "Rust" => {
                    if trimmed.starts_with("fn ") {
                        if let Some(ne) = trimmed.find('(') {
                            if ne > 4 {
                                let name = trimmed[4..ne].trim();
                                if name == "main" {
                                    entry_points.push("main".to_string());
                                }
                            }
                        }
                    }
                    if trimmed.starts_with("struct ") || trimmed.starts_with("enum ") {
                        // classes count
                    }
                    if trimmed.starts_with("use ") || trimmed.starts_with("mod ") {
                        // imports
                    }
                    if trimmed.starts_with("mod ") {
                        if let Some(ne) = trimmed.find(' ') {
                            if ne > 4 {
                                let name = trimmed[4..ne].trim().trim_end_matches(';').to_string();
                            }
                        }
                    }
                }
                "Python" => {
                    if trimmed.starts_with("def ") {
                        if let Some(ne) = trimmed.find('(') {
                            if ne > 4 {
                                let name = trimmed[4..ne].trim();
                                if name == "__main__" || name == "main" {
                                    entry_points.push("__main__".to_string());
                                }
                            }
                        }
                    }
                    if trimmed.starts_with("class ") {
                        if let Some(ne) = trimmed.find(['(', ':']) {
                            if ne > 6 {
                                let name = trimmed[6..ne].trim().to_string();
                            }
                        }
                    }
                    if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                        // imports
                    }
                    if trimmed.contains("if __name__ == \"__main__\"") {
                        entry_points.push("__main__".to_string());
                    }
                }
                "JavaScript" | "TypeScript" => {
                    if trimmed.starts_with("function ") || trimmed.contains("=>") {
                        // functions
                    }
                    if trimmed.starts_with("class ") {
                        // classes
                    }
                    if trimmed.starts_with("import ") || trimmed.starts_with("require(") {
                        // imports
                    }
                }
                _ => {}
            }
        }
    }

    fn is_test_file(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().to_lowercase();
        path_str.contains("test")
            || path_str.contains("spec")
            || path_str.ends_with("_test.py")
            || path_str.ends_with("_test.rs")
            || path_str.ends_with(".test.js")
            || path_str.ends_with(".test.ts")
            || path_str.ends_with(".spec.js")
            || path_str.ends_with(".spec.ts")
    }

    fn detect_tech_stack(&self, ctx: &mut ProjectContext) -> Result<()> {
        let mut stack = TechStack::default();

        // From config files
        if ctx.config_files.contains_key("Cargo.toml") {
            stack.languages.push("Rust".to_string());
            if let Some(content) = ctx.config_files.get("Cargo.toml") {
                if content.contains("actix") { stack.frameworks.push("Actix".to_string()); }
                if content.contains("tokio") { stack.frameworks.push("Tokio".to_string()); }
                if content.contains("axum") { stack.frameworks.push("Axum".to_string()); }
                if content.contains("serde") { stack.tools.push("Serde".to_string()); }
            }
        }
        if ctx.config_files.contains_key("pyproject.toml") || ctx.config_files.contains_key("requirements.txt") {
            stack.languages.push("Python".to_string());
        }
        if ctx.config_files.contains_key("package.json") {
            stack.languages.push("JavaScript".to_string());
            if let Some(content) = ctx.config_files.get("package.json") {
                if content.contains("react") { stack.frameworks.push("React".to_string()); }
                if content.contains("next") { stack.frameworks.push("Next.js".to_string()); }
                if content.contains("vue") { stack.frameworks.push("Vue".to_string()); }
                if content.contains("typescript") { stack.languages.push("TypeScript".to_string()); }
            }
        }
        if ctx.config_files.contains_key("go.mod") {
            stack.languages.push("Go".to_string());
        }
        if ctx.config_files.contains_key("pom.xml") || ctx.config_files.contains_key("build.gradle") {
            stack.languages.push("Java".to_string());
        }
        if ctx.config_files.contains_key("Dockerfile") {
            stack.tools.push("Docker".to_string());
        }
        if ctx.config_files.contains_key("docker-compose.yml") {
            stack.tools.push("Docker Compose".to_string());
        }
        if ctx.config_files.contains_key(".github/workflows") || ctx.root_path.join(".github/workflows").exists() {
            stack.ci_cd.push("GitHub Actions".to_string());
        }
        if ctx.root_path.join(".gitlab-ci.yml").exists() {
            stack.ci_cd.push("GitLab CI".to_string());
        }

        ctx.tech_stack = stack;
        Ok(())
    }

    fn determine_project_type(&self, ctx: &ProjectContext) -> ProjectType {
        if ctx.tech_stack.languages.contains(&"Rust".to_string()) {
            ProjectType::Rust
        } else if ctx.tech_stack.languages.contains(&"Python".to_string()) {
            ProjectType::Python
        } else if ctx.tech_stack.languages.contains(&"JavaScript".to_string())
            || ctx.tech_stack.languages.contains(&"TypeScript".to_string()) {
            ProjectType::TypeScript
        } else if ctx.tech_stack.languages.contains(&"Go".to_string()) {
            ProjectType::Go
        } else if ctx.tech_stack.languages.contains(&"Java".to_string()) {
            ProjectType::Java
        } else {
            ProjectType::Mixed
        }
    }

    fn extract_summary(&self, content: &str) -> String {
        let lines: Vec<&str> = content.lines().take(10).collect();
        lines.join(" ").chars().take(200).collect::<String>()
    }

    fn extract_objectives(&self, ctx: &mut ProjectContext) -> Result<()> {
        // Extract from README
        if let Some(readme) = &ctx.readme {
            for line in readme.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("# ")
                    || trimmed.starts_with("## ")
                    || trimmed.to_lowercase().contains("objective")
                    || trimmed.to_lowercase().contains("goal")
                    || trimmed.to_lowercase().contains("purpose")
                    || trimmed.to_lowercase().contains("mission") {
                    ctx.objectives.push(Objective {
                        title: trimmed.trim_start_matches('#').trim().to_string(),
                        description: trimmed.to_string(),
                        priority: Priority::High,
                        source: "README.md".to_string(),
                    });
                }
            }
        }

        // Extract from AGENTS.md
        if let Some(agents) = &ctx.agents_md {
            for line in agents.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("- [ ]") || trimmed.starts_with("- [x]")
                    || trimmed.contains("TODO")
                    || trimmed.contains("objective")
                    || trimmed.contains("goal") {
                    ctx.objectives.push(Objective {
                        title: trimmed.to_string(),
                        description: trimmed.to_string(),
                        priority: Priority::Medium,
                        source: "AGENTS.md".to_string(),
                    });
                }
            }
        }

        Ok(())
    }

    fn detect_patterns(&self, ctx: &mut ProjectContext) -> Result<()> {
        let mut patterns = Vec::new();

        // Check for common patterns in code
        for module in &ctx.code_structure.modules {
            for func in &module.functions {
                if func.contains("async") || func.contains("await") {
                    patterns.push(CodePattern {
                        name: "Async/Await".to_string(),
                        description: "Asynchronous programming pattern".to_string(),
                        examples: vec![func.clone()],
                        files: vec![module.path.clone()],
                    });
                }
                if func.contains("Result<") || func.contains("Option<") {
                    patterns.push(CodePattern {
                        name: "Error Handling with Result/Option".to_string(),
                        description: "Explicit error handling pattern".to_string(),
                        examples: vec![func.clone()],
                        files: vec![module.path.clone()],
                    });
                }
            }
        }

        // Deduplicate
        patterns.sort_by(|a, b| a.name.cmp(&b.name));
        patterns.dedup_by(|a, b| a.name == b.name);

        ctx.patterns = patterns;
        Ok(())
    }

    fn identify_roles(&self, ctx: &mut ProjectContext) -> Result<()> {
        let mut roles = Vec::new();

        // Backend developer
        if ctx.tech_stack.languages.iter().any(|l|
            matches!(l.as_str(), "Rust" | "Python" | "Go" | "Java" | "Node.js")
        ) {
            roles.push(Role {
                name: "Backend Developer".to_string(),
                description: "Implements server-side logic, APIs, databases".to_string(),
                responsibilities: vec![
                    "API design and implementation".to_string(),
                    "Database design and optimization".to_string(),
                    "Authentication and authorization".to_string(),
                    "Performance optimization".to_string(),
                ],
                files: Vec::new(),
            });
        }

        // Frontend developer
        if ctx.tech_stack.languages.iter().any(|l|
            matches!(l.as_str(), "JavaScript" | "TypeScript" | "Vue" | "React")
        ) {
            roles.push(Role {
                name: "Frontend Developer".to_string(),
                description: "Implements user interfaces and client-side logic".to_string(),
                responsibilities: vec![
                    "UI component development".to_string(),
                    "State management".to_string(),
                    "API integration".to_string(),
                    "Responsive design".to_string(),
                ],
                files: Vec::new(),
            });
        }

        // DevOps/Platform
        if !ctx.tech_stack.ci_cd.is_empty() || !ctx.tech_stack.tools.is_empty() {
            roles.push(Role {
                name: "DevOps Engineer".to_string(),
                description: "Manages infrastructure, CI/CD, deployment".to_string(),
                responsibilities: vec![
                    "CI/CD pipeline maintenance".to_string(),
                    "Infrastructure as Code".to_string(),
                    "Monitoring and observability".to_string(),
                    "Security and compliance".to_string(),
                ],
                files: Vec::new(),
            });
        }

        // QA/Testing
        if !ctx.code_structure.test_files.is_empty() {
            roles.push(Role {
                name: "QA Engineer".to_string(),
                description: "Ensures code quality through testing".to_string(),
                responsibilities: vec![
                    "Unit test development".to_string(),
                    "Integration testing".to_string(),
                    "Test automation".to_string(),
                    "Quality metrics".to_string(),
                ],
                files: ctx.code_structure.test_files.iter().map(PathBuf::from).collect(),
            });
        }

        // Architect
        roles.push(Role {
            name: "Software Architect".to_string(),
            description: "Defines system architecture and technical decisions".to_string(),
            responsibilities: vec![
                "System design".to_string(),
                "Technology selection".to_string(),
                "Code review and standards".to_string(),
                "Technical debt management".to_string(),
            ],
            files: Vec::new(),
        });

        ctx.roles = roles;
        Ok(())
    }
}

/// Format project context for LLM consumption
pub fn format_context_for_llm(ctx: &ProjectContext) -> String {
    let mut parts = Vec::new();

    parts.push(format!("=== PROJECT: {} ===", ctx.name));
    parts.push(format!("Type: {:?}", ctx.project_type));
    parts.push(format!("Path: {}", ctx.root_path.display()));
    parts.push("".to_string());

    // Tech Stack
    if !ctx.tech_stack.languages.is_empty() {
        parts.push("=== TECH STACK ===".to_string());
        parts.push(format!("Languages: {}", ctx.tech_stack.languages.join(", ")));
        if !ctx.tech_stack.frameworks.is_empty() {
            parts.push(format!("Frameworks: {}", ctx.tech_stack.frameworks.join(", ")));
        }
        if !ctx.tech_stack.tools.is_empty() {
            parts.push(format!("Tools: {}", ctx.tech_stack.tools.join(", ")));
        }
        if !ctx.tech_stack.ci_cd.is_empty() {
            parts.push(format!("CI/CD: {}", ctx.tech_stack.ci_cd.join(", ")));
        }
        parts.push("".to_string());
    }

    // Code Structure Summary
    if ctx.code_structure.total_files > 0 {
        parts.push("=== CODE STRUCTURE ===".to_string());
        parts.push(format!("Total files: {}", ctx.code_structure.total_files));
        parts.push(format!("Total lines: {}", ctx.code_structure.total_lines));
        parts.push(format!("Languages: {}", ctx.code_structure.languages.keys().cloned().collect::<Vec<_>>().join(", ")));
        parts.push(format!("Modules: {}", ctx.code_structure.modules.len()));
        parts.push(format!("Entry points: {}", ctx.code_structure.entry_points.join(", ")));
        parts.push(format!("Test files: {}", ctx.code_structure.test_files.len()));
        parts.push("".to_string());
    }

    // Objectives
    if !ctx.objectives.is_empty() {
        parts.push("=== OBJECTIVES ===".to_string());
        for obj in &ctx.objectives {
            parts.push(format!("- [{:?}] {}: {}", obj.priority, obj.title, obj.description));
        }
        parts.push("".to_string());
    }

    // Patterns
    if !ctx.patterns.is_empty() {
        parts.push("=== CODE PATTERNS ===".to_string());
        for pattern in &ctx.patterns {
            parts.push(format!("- {}: {}", pattern.name, pattern.description));
        }
        parts.push("".to_string());
    }

    // Roles
    if !ctx.roles.is_empty() {
        parts.push("=== PROJECT ROLES ===".to_string());
        for role in &ctx.roles {
            parts.push(format!("- {}: {}", role.name, role.description));
            for resp in &role.responsibilities {
                parts.push(format!("  - {}", resp));
            }
        }
        parts.push("".to_string());
    }

    // Key documentation
    if !ctx.docs.is_empty() {
        parts.push("=== DOCUMENTATION ===".to_string());
        for doc in &ctx.docs {
            parts.push(format!("- {} ({:?}): {}", doc.title, doc.content_type, doc.summary));
        }
        parts.push("".to_string());
    }

    // Configuration summary
    if !ctx.config_files.is_empty() {
        parts.push("=== KEY CONFIG FILES ===".to_string());
        for (name, _) in &ctx.config_files {
            parts.push(format!("- {}", name));
        }
        parts.push("".to_string());
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_context_analysis() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create a simple Rust project
        fs::write(root.join("Cargo.toml"), r#"
[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
"#).unwrap();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), r#"
use tokio;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
}

async fn fetch_user(id: u64) -> Result<User, Box<dyn std::error::Error>> {
    Ok(User { id, name: "Test".to_string() })
}

#[tokio::main]
async fn main() {
    let user = fetch_user(1).await.unwrap();
    println!("User: {}", user.name);
}
"#).unwrap();

        fs::write(root.join("README.md"), "# Test Project\n\nA simple Rust project using Tokio and Serde.\n\n## Objectives\n- Build a simple async user fetcher\n- Demonstrate serde serialization\n").unwrap();

        let analyzer = ContextAnalyzer::new(root.to_path_buf());
        let ctx = analyzer.analyze().unwrap();

        assert_eq!(ctx.project_type, ProjectType::Rust);
        assert!(ctx.tech_stack.languages.contains(&"Rust".to_string()));
        assert!(ctx.tech_stack.frameworks.contains(&"Tokio".to_string()));
        assert!(ctx.readme.is_some());
        assert!(ctx.objectives.len() > 0);
        assert!(ctx.code_structure.total_files > 0);

        println!("Context analysis test passed!");
        println!("{}", format_context_for_llm(&ctx));
    }
}