//! context.rs - Enhanced context understanding for lai
//! Provides deep project understanding: code structure, documentation, patterns, objectives

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

/// Maximum size of a source file to analyze (bytes).
const MAX_SOURCE_FILE_BYTES: u64 = 512 * 1024;
/// Maximum size of a documentation/config file to load (bytes).
const MAX_TEXT_FILE_BYTES: u64 = 512 * 1024;
/// Maximum number of objectives kept in the context.
const MAX_OBJECTIVES: usize = 20;
/// Maximum number of modules recorded in the code structure.
const MAX_MODULES: usize = 500;
/// Maximum number of examples/files recorded per pattern.
const MAX_PATTERN_EXAMPLES: usize = 3;
const MAX_PATTERN_FILES: usize = 5;
/// Maximum size of the formatted context (characters).
const MAX_CONTEXT_CHARS: usize = 16_000;
/// Maximum excerpt of README/AGENTS included in the formatted context.
const MAX_DOC_EXCERPT_CHARS: usize = 2_500;
/// Maximum excerpt of each manifest included in the formatted context.
const MAX_MANIFEST_EXCERPT_CHARS: usize = 1_000;
/// Maximum number of manifests included in the formatted context.
const MAX_MANIFEST_EXCERPTS: usize = 3;

/// Directories that never contain project source code.
const SKIPPED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    "vendor",
    "dist",
    "build",
    "out",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".benchmarks",
    ".gradle",
    ".idea",
    ".vscode",
    ".cargo",
    "site-packages",
    "Pods",
];

/// Files that only add noise to the analysis.
const SKIPPED_FILES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "poetry.lock",
    "go.sum",
    "composer.lock",
];

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
}

/// Result of analyzing a single source file.
#[derive(Debug, Default)]
struct FileAnalysis {
    functions: Vec<String>,
    classes: Vec<String>,
    imports: Vec<String>,
    is_entry: bool,
}

impl ContextAnalyzer {
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Perform deep project analysis
    pub fn analyze(&self) -> Result<ProjectContext> {
        let mut ctx = ProjectContext {
            name: self
                .project_root
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
        self.read_documentation(&mut ctx);

        // 2. Read configuration files (root and one level deep, for monorepos)
        self.read_config_files(&mut ctx);

        // 3. Analyze code structure and detect patterns
        self.analyze_code_structure(&mut ctx);

        // 4. Detect tech stack
        self.detect_tech_stack(&mut ctx);

        // 5. Determine project type from the code actually present
        ctx.project_type = self.determine_project_type(&ctx);

        // 6. Extract objectives from documentation
        self.extract_objectives(&mut ctx);

        // 7. Finalize patterns (deterministic order)
        self.detect_patterns(&mut ctx);

        // 8. Identify roles
        self.identify_roles(&mut ctx);

        Ok(ctx)
    }

    fn read_documentation(&self, ctx: &mut ProjectContext) {
        for name in [
            "README.md",
            "README.txt",
            "README.rst",
            "readme.md",
            "readme.txt",
        ] {
            let path = ctx.root_path.join(name);
            if let Some(content) = read_text_file(&path, MAX_TEXT_FILE_BYTES) {
                let summary = extract_summary(&content);
                ctx.readme = Some(content);
                ctx.docs.push(DocFile {
                    path,
                    title: "README".to_string(),
                    summary,
                    content_type: DocType::Readme,
                });
                break;
            }
        }

        let agents_path = ctx.root_path.join("AGENTS.md");
        if let Some(content) = read_text_file(&agents_path, MAX_TEXT_FILE_BYTES) {
            ctx.agents_md = Some(content);
            ctx.docs.push(DocFile {
                path: agents_path,
                title: "Agent Instructions".to_string(),
                summary: "Agent instructions and rules".to_string(),
                content_type: DocType::Guide,
            });
        }

        let claude_path = ctx.root_path.join("CLAUDE.md");
        if let Some(content) = read_text_file(&claude_path, MAX_TEXT_FILE_BYTES) {
            ctx.claude_md = Some(content);
            ctx.docs.push(DocFile {
                path: claude_path,
                title: "Claude Instructions".to_string(),
                summary: "Claude Code instructions".to_string(),
                content_type: DocType::Guide,
            });
        }

        let contrib_path = ctx.root_path.join("CONTRIBUTING.md");
        if let Some(content) = read_text_file(&contrib_path, MAX_TEXT_FILE_BYTES) {
            let summary = extract_summary(&content);
            ctx.contributing_md = Some(content);
            ctx.docs.push(DocFile {
                path: contrib_path,
                title: "Contributing Guide".to_string(),
                summary,
                content_type: DocType::Contributing,
            });
        }

        let changelog_path = ctx.root_path.join("CHANGELOG.md");
        if let Some(content) = read_text_file(&changelog_path, MAX_TEXT_FILE_BYTES) {
            ctx.changelog_md = Some(content);
            ctx.docs.push(DocFile {
                path: changelog_path,
                title: "Changelog".to_string(),
                summary: "Project changelog".to_string(),
                content_type: DocType::Changelog,
            });
        }
    }

    fn read_config_files(&self, ctx: &mut ProjectContext) {
        const ROOT_CONFIGS: &[&str] = &[
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
            "Cargo.toml",
            "go.mod",
            "package.json",
            "tsconfig.json",
            "pom.xml",
            "build.gradle",
            "Dockerfile",
            "docker-compose.yml",
            "Makefile",
            ".gitignore",
        ];

        for name in ROOT_CONFIGS {
            let path = ctx.root_path.join(name);
            if let Some(content) = read_text_file(&path, MAX_TEXT_FILE_BYTES) {
                ctx.config_files.insert((*name).to_string(), content);
            }
        }

        // Monorepos often keep manifests in subdirectories (e.g. lai_rust/Cargo.toml).
        const NESTED_MANIFESTS: &[&str] = &[
            "Cargo.toml",
            "pyproject.toml",
            "package.json",
            "go.mod",
            "pom.xml",
            "build.gradle",
        ];
        let mut dirs: Vec<PathBuf> = match fs::read_dir(&ctx.root_path) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && !self.should_skip(p.strip_prefix(&ctx.root_path).unwrap_or(p.as_path()))
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        dirs.sort();

        for dir in dirs.into_iter().take(12) {
            for name in NESTED_MANIFESTS {
                let path = dir.join(name);
                if let Some(content) = read_text_file(&path, MAX_TEXT_FILE_BYTES) {
                    if let Ok(rel) = path.strip_prefix(&ctx.root_path) {
                        ctx.config_files
                            .insert(rel.to_string_lossy().replace('\\', "/"), content);
                    }
                }
            }
        }
    }

    fn analyze_code_structure(&self, ctx: &mut ProjectContext) {
        let mut total_files = 0usize;
        let mut total_lines = 0usize;
        let mut languages: HashMap<String, LanguageStats> = HashMap::new();
        let mut modules: Vec<ModuleInfo> = Vec::new();
        let mut entry_points: Vec<String> = Vec::new();
        let mut test_files: Vec<String> = Vec::new();
        let mut patterns: Vec<CodePattern> = Vec::new();

        let walker = WalkDir::new(&ctx.root_path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                if entry.depth() == 0 {
                    return true;
                }
                let rel = entry
                    .path()
                    .strip_prefix(&ctx.root_path)
                    .unwrap_or_else(|_| entry.path());
                !self.should_skip(rel)
            });

        for entry in walker
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let rel_path = path
                .strip_prefix(&ctx.root_path)
                .unwrap_or(path)
                .to_path_buf();

            let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some(lang) = extension_to_language(ext) else {
                continue;
            };

            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.len() > MAX_SOURCE_FILE_BYTES {
                continue;
            }
            // Skip binary or non-UTF-8 files instead of failing the whole analysis.
            let Ok(content) = fs::read_to_string(path) else {
                continue;
            };

            total_files += 1;
            let line_count = content.lines().count();
            total_lines += line_count;

            let analysis = analyze_file_content(&content, lang);
            let is_test = is_test_file(&rel_path);
            if is_test {
                test_files.push(rel_path.to_string_lossy().to_string());
            }
            if analysis.is_entry {
                entry_points.push(rel_path.to_string_lossy().to_string());
            }

            let stats = languages
                .entry(lang.to_string())
                .or_insert_with(|| LanguageStats {
                    files: 0,
                    lines: 0,
                    functions: 0,
                    classes: 0,
                    imports: Vec::new(),
                });
            stats.files += 1;
            stats.lines += line_count;
            stats.functions += analysis.functions.len();
            stats.classes += analysis.classes.len();
            stats.imports.extend(analysis.imports.iter().cloned());

            if modules.len() < MAX_MODULES
                && (!analysis.functions.is_empty()
                    || !analysis.classes.is_empty()
                    || analysis.is_entry)
            {
                modules.push(ModuleInfo {
                    name: rel_path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    path: rel_path.clone(),
                    description: None,
                    functions: analysis.functions,
                    classes: analysis.classes,
                    is_test,
                    is_entry: analysis.is_entry,
                });
            }

            record_patterns(&content, lang, &rel_path, &mut patterns);
        }

        entry_points.sort();
        entry_points.dedup();
        test_files.sort();
        test_files.dedup();
        modules.sort_by(|a, b| a.path.cmp(&b.path));
        for stats in languages.values_mut() {
            stats.imports.sort();
            stats.imports.dedup();
        }

        let mut config_files: Vec<String> = ctx.config_files.keys().cloned().collect();
        config_files.sort();

        ctx.code_structure = CodeStructure {
            total_files,
            total_lines,
            languages,
            modules,
            entry_points,
            test_files,
            config_files,
        };
        ctx.patterns = patterns;
    }

    /// Check whether a path (relative to the project root) should be ignored.
    fn should_skip(&self, rel_path: &Path) -> bool {
        rel_path.components().any(|component| match component {
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                name.starts_with('.')
                    || SKIPPED_DIRS.contains(&name.as_ref())
                    || SKIPPED_FILES.contains(&name.as_ref())
                    || name.ends_with(".min.js")
                    || name.ends_with(".min.css")
            }
            _ => false,
        })
    }

    fn detect_tech_stack(&self, ctx: &mut ProjectContext) {
        let mut stack = TechStack::default();

        for content in config_contents(ctx, "Cargo.toml") {
            push_unique(&mut stack.languages, "Rust");
            let lower = content.to_lowercase();
            for (needle, name) in [
                ("tokio", "Tokio"),
                ("axum", "Axum"),
                ("actix", "Actix"),
                ("warp", "Warp"),
                ("rocket", "Rocket"),
            ] {
                if lower.contains(needle) {
                    push_unique(&mut stack.frameworks, name);
                }
            }
            for (needle, name) in [
                ("serde", "Serde"),
                ("reqwest", "Reqwest"),
                ("clap", "Clap"),
                ("tracing", "Tracing"),
                ("tonic", "Tonic"),
            ] {
                if lower.contains(needle) {
                    push_unique(&mut stack.tools, name);
                }
            }
            for (needle, name) in [
                ("sqlx", "SQLx"),
                ("rusqlite", "SQLite"),
                ("diesel", "Diesel"),
                ("mongodb", "MongoDB"),
                ("redis", "Redis"),
            ] {
                if lower.contains(needle) {
                    push_unique(&mut stack.databases, name);
                }
            }
        }

        for content in config_contents(ctx, "package.json") {
            push_unique(&mut stack.languages, "JavaScript");
            let lower = content.to_lowercase();
            if lower.contains("typescript") {
                push_unique(&mut stack.languages, "TypeScript");
            }
            for (needle, name) in [
                ("react", "React"),
                ("next", "Next.js"),
                ("vue", "Vue"),
                ("svelte", "Svelte"),
                ("express", "Express"),
                ("nestjs", "NestJS"),
            ] {
                if lower.contains(needle) {
                    push_unique(&mut stack.frameworks, name);
                }
            }
            for (needle, name) in [
                ("jest", "Jest"),
                ("vitest", "Vitest"),
                ("playwright", "Playwright"),
                ("cypress", "Cypress"),
                ("eslint", "ESLint"),
            ] {
                if lower.contains(needle) {
                    push_unique(&mut stack.tools, name);
                }
            }
            for (needle, name) in [
                ("prisma", "Prisma"),
                ("mongoose", "MongoDB"),
                ("pg", "PostgreSQL"),
                ("redis", "Redis"),
            ] {
                if lower.contains(needle) {
                    push_unique(&mut stack.databases, name);
                }
            }
        }

        for name in [
            "pyproject.toml",
            "requirements.txt",
            "setup.py",
            "setup.cfg",
        ] {
            for content in config_contents(ctx, name) {
                push_unique(&mut stack.languages, "Python");
                let lower = content.to_lowercase();
                for (needle, framework) in [
                    ("fastapi", "FastAPI"),
                    ("flask", "Flask"),
                    ("django", "Django"),
                ] {
                    if lower.contains(needle) {
                        push_unique(&mut stack.frameworks, framework);
                    }
                }
                for (needle, tool) in [
                    ("pytest", "pytest"),
                    ("ruff", "Ruff"),
                    ("mypy", "mypy"),
                    ("black", "Black"),
                    ("numpy", "NumPy"),
                    ("pandas", "pandas"),
                    ("torch", "PyTorch"),
                ] {
                    if lower.contains(needle) {
                        push_unique(&mut stack.tools, tool);
                    }
                }
                for (needle, database) in [
                    ("sqlalchemy", "SQLAlchemy"),
                    ("psycopg", "PostgreSQL"),
                    ("pymongo", "MongoDB"),
                    ("redis", "Redis"),
                    ("sqlite", "SQLite"),
                ] {
                    if lower.contains(needle) {
                        push_unique(&mut stack.databases, database);
                    }
                }
            }
        }

        for content in config_contents(ctx, "go.mod") {
            push_unique(&mut stack.languages, "Go");
            let lower = content.to_lowercase();
            for (needle, name) in [
                ("gin-gonic", "Gin"),
                ("labstack/echo", "Echo"),
                ("gofiber", "Fiber"),
                ("gorm", "GORM"),
            ] {
                if lower.contains(needle) {
                    push_unique(&mut stack.frameworks, name);
                }
            }
        }

        for name in ["pom.xml", "build.gradle"] {
            for content in config_contents(ctx, name) {
                push_unique(&mut stack.languages, "Java");
                if content.to_lowercase().contains("spring") {
                    push_unique(&mut stack.frameworks, "Spring");
                }
            }
        }

        if !config_contents(ctx, "Dockerfile").is_empty() {
            push_unique(&mut stack.tools, "Docker");
        }
        if !config_contents(ctx, "docker-compose.yml").is_empty() {
            push_unique(&mut stack.tools, "Docker Compose");
        }
        if !config_contents(ctx, "Makefile").is_empty() {
            push_unique(&mut stack.tools, "Make");
        }

        if ctx.root_path.join(".github/workflows").is_dir() {
            push_unique(&mut stack.ci_cd, "GitHub Actions");
        }
        if ctx.root_path.join(".gitlab-ci.yml").is_file() {
            push_unique(&mut stack.ci_cd, "GitLab CI");
        }
        if ctx.root_path.join(".circleci/config.yml").is_file() {
            push_unique(&mut stack.ci_cd, "CircleCI");
        }
        if ctx.root_path.join("Jenkinsfile").is_file() {
            push_unique(&mut stack.ci_cd, "Jenkins");
        }
        if ctx.root_path.join("azure-pipelines.yml").is_file() {
            push_unique(&mut stack.ci_cd, "Azure Pipelines");
        }

        for list in [
            &mut stack.languages,
            &mut stack.frameworks,
            &mut stack.databases,
            &mut stack.tools,
            &mut stack.ci_cd,
        ] {
            list.sort();
            list.dedup();
        }

        ctx.tech_stack = stack;
    }

    /// Determine the project type from the code actually present.
    ///
    /// A root-level manifest can be misleading in monorepos (e.g. a `pyproject.toml`
    /// at the root of a Rust-first project), so the dominant source language wins.
    fn determine_project_type(&self, ctx: &ProjectContext) -> ProjectType {
        let mut langs: Vec<(&String, &LanguageStats)> = ctx
            .code_structure
            .languages
            .iter()
            .filter(|(name, _)| name.as_str() != "Other")
            .collect();
        langs.sort_by_key(|(_, stats)| std::cmp::Reverse(stats.files));

        let Some((name, stats)) = langs.first() else {
            return ProjectType::Unknown;
        };

        if let Some((_, second)) = langs.get(1) {
            // A project is mixed when the second language is at least half the size
            // of the dominant one (e.g. Rust core + substantial Python tooling).
            if second.files * 100 >= stats.files * 50 {
                return ProjectType::Mixed;
            }
        }

        match name.as_str() {
            "Rust" => ProjectType::Rust,
            "Python" => ProjectType::Python,
            "JavaScript" => ProjectType::JavaScript,
            "TypeScript" => ProjectType::TypeScript,
            "Go" => ProjectType::Go,
            "Java" => ProjectType::Java,
            "C" | "C++" => ProjectType::Cpp,
            "C#" => ProjectType::CSharp,
            _ => ProjectType::Mixed,
        }
    }

    fn extract_objectives(&self, ctx: &mut ProjectContext) {
        let mut objectives = Vec::new();

        if let Some(readme) = &ctx.readme {
            objectives.extend(extract_markdown_objectives(
                readme,
                "README.md",
                Priority::High,
            ));
        }

        for (content, source) in [
            (ctx.agents_md.as_deref(), "AGENTS.md"),
            (ctx.claude_md.as_deref(), "CLAUDE.md"),
        ] {
            let Some(content) = content else {
                continue;
            };
            for line in content.lines() {
                let trimmed = line.trim();
                let lower = trimmed.to_lowercase();
                let text = if let Some(rest) = trimmed
                    .strip_prefix("- [ ]")
                    .or_else(|| trimmed.strip_prefix("* [ ]"))
                {
                    Some(rest.trim())
                } else if lower.contains("todo") || lower.contains("fixme") {
                    Some(trimmed)
                } else {
                    None
                };

                if let Some(text) = text {
                    if !text.is_empty() {
                        objectives.push(Objective {
                            title: truncate_chars(text, 160),
                            description: truncate_chars(text, 240),
                            priority: Priority::Medium,
                            source: source.to_string(),
                        });
                    }
                }
            }
        }

        let mut seen = std::collections::HashSet::new();
        objectives.retain(|objective| seen.insert(objective.title.to_lowercase()));
        objectives.truncate(MAX_OBJECTIVES);
        ctx.objectives = objectives;
    }

    fn detect_patterns(&self, ctx: &mut ProjectContext) {
        ctx.patterns.sort_by(|a, b| a.name.cmp(&b.name));
        ctx.patterns.dedup_by(|a, b| a.name == b.name);
    }

    fn identify_roles(&self, ctx: &mut ProjectContext) {
        let mut roles = Vec::new();

        // Backend developer
        if ctx
            .tech_stack
            .languages
            .iter()
            .any(|l| matches!(l.as_str(), "Rust" | "Python" | "Go" | "Java" | "JavaScript"))
        {
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
        if ctx
            .tech_stack
            .languages
            .iter()
            .any(|l| matches!(l.as_str(), "JavaScript" | "TypeScript" | "Vue" | "React"))
        {
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
                files: ctx
                    .code_structure
                    .test_files
                    .iter()
                    .map(PathBuf::from)
                    .collect(),
            });
        }

        // Architect (only meaningful when there is code to architect)
        if ctx.code_structure.total_files > 0 {
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
        }

        ctx.roles = roles;
    }
}

/// Map a file extension to a source language, or `None` if it is not source code.
fn extension_to_language(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("Rust"),
        "py" => Some("Python"),
        "js" | "jsx" | "mjs" | "cjs" => Some("JavaScript"),
        "ts" | "tsx" | "mts" | "cts" => Some("TypeScript"),
        "go" => Some("Go"),
        "java" => Some("Java"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("C++"),
        "c" | "h" => Some("C"),
        "cs" => Some("C#"),
        "rb" => Some("Ruby"),
        "php" => Some("PHP"),
        "swift" => Some("Swift"),
        "kt" | "kts" => Some("Kotlin"),
        "scala" => Some("Scala"),
        "clj" | "cljs" | "cljc" => Some("Clojure"),
        "hs" => Some("Haskell"),
        "ml" | "mli" => Some("OCaml"),
        "fs" | "fsx" => Some("F#"),
        "vue" => Some("Vue"),
        "svelte" => Some("Svelte"),
        _ => None,
    }
}

/// Return the contents of all loaded config files whose file name matches `name`.
fn config_contents<'a>(ctx: &'a ProjectContext, name: &str) -> Vec<&'a str> {
    ctx.config_files
        .iter()
        .filter(|(key, _)| {
            Path::new(key.as_str())
                .file_name()
                .and_then(|file| file.to_str())
                == Some(name)
        })
        .map(|(_, content)| content.as_str())
        .collect()
}

/// Read a UTF-8 text file, returning `None` for missing, oversized or binary files.
fn read_text_file(path: &Path, max_bytes: u64) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return None;
    }
    fs::read_to_string(path).ok()
}

/// Extract a short, human-readable summary from Markdown/text content.
fn extract_summary(content: &str) -> String {
    let mut summary = String::new();
    let mut in_code_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        if trimmed.is_empty() {
            if !summary.is_empty() {
                break;
            }
            continue;
        }
        if trimmed.starts_with('#')
            || trimmed.starts_with("![")
            || trimmed.starts_with("[![")
            || trimmed.starts_with('<')
        {
            continue;
        }

        if !summary.is_empty() {
            summary.push(' ');
        }
        summary.push_str(trimmed);
        if summary.chars().count() >= 240 {
            break;
        }
    }

    truncate_chars(&summary, 300)
}

/// Extract objectives from bullet/numbered items inside "goals"/"objectives" sections.
fn extract_markdown_objectives(content: &str, source: &str, priority: Priority) -> Vec<Objective> {
    let mut objectives = Vec::new();
    let mut in_section = false;
    let mut section_level = 0usize;

    for line in content.lines() {
        let trimmed = line.trim();
        let heading_level = trimmed.chars().take_while(|c| *c == '#').count();

        if heading_level > 0 {
            let title = trimmed.trim_start_matches('#').trim();
            let lower = title.to_lowercase();
            let is_objective_heading = ["objective", "goal", "mission", "purpose", "vision"]
                .iter()
                .any(|keyword| lower.contains(keyword));

            if is_objective_heading {
                in_section = true;
                section_level = heading_level;
            } else if in_section && heading_level <= section_level {
                in_section = false;
            }
            continue;
        }

        if !in_section || trimmed.is_empty() {
            continue;
        }

        let bullet = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "));

        let text = if let Some(bullet) = bullet {
            Some(bullet.trim())
        } else if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            trimmed
                .split_once(". ")
                .or_else(|| trimmed.split_once(") "))
                .map(|(_, rest)| rest.trim())
        } else {
            None
        };

        if let Some(text) = text {
            let text = clean_markdown(text);
            if text.len() >= 3 {
                objectives.push(Objective {
                    title: truncate_chars(&text, 160),
                    description: truncate_chars(&text, 240),
                    priority: priority.clone(),
                    source: source.to_string(),
                });
            }
        }
    }

    objectives
}

/// Analyze the functions, types, imports and entry points of a source file.
fn analyze_file_content(content: &str, lang: &str) -> FileAnalysis {
    let mut analysis = FileAnalysis::default();
    let mut in_multiline_string = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip code samples embedded in raw/triple-quoted strings (common in
        // tests and documentation strings) so they are not counted as symbols.
        if in_multiline_string {
            if closes_multiline_string(trimmed, lang) {
                in_multiline_string = false;
            }
            continue;
        }
        if opens_multiline_string(trimmed, lang) {
            in_multiline_string = true;
            continue;
        }

        match lang {
            "Rust" => analyze_rust_line(trimmed, &mut analysis),
            "Python" => analyze_python_line(trimmed, &mut analysis),
            "JavaScript" | "TypeScript" => analyze_js_line(trimmed, &mut analysis),
            "Go" => analyze_go_line(trimmed, &mut analysis),
            "Java" | "C#" | "C++" | "Kotlin" | "Scala" => {
                analyze_java_like_line(trimmed, &mut analysis)
            }
            _ => {}
        }
    }

    analysis.functions.sort();
    analysis.functions.dedup();
    analysis.classes.sort();
    analysis.classes.dedup();
    analysis.imports.sort();
    analysis.imports.dedup();
    analysis.imports.truncate(100);

    analysis
}

/// Detect the start of a multi-line raw string that continues on following lines.
fn opens_multiline_string(line: &str, lang: &str) -> bool {
    match lang {
        "Rust" => {
            if let Some(idx) = line.find("r#\"") {
                return !line[idx + 3..].contains("\"#");
            }
            false
        }
        "Python" => ["\"\"\"", "'''"].iter().any(|delim| {
            line.find(delim)
                .is_some_and(|idx| !line[idx + 3..].contains(delim))
        }),
        _ => false,
    }
}

fn closes_multiline_string(line: &str, lang: &str) -> bool {
    match lang {
        "Rust" => line.contains("\"#"),
        "Python" => line.contains("\"\"\"") || line.contains("'''"),
        _ => true,
    }
}

fn analyze_rust_line(line: &str, analysis: &mut FileAnalysis) {
    let code = strip_rust_modifiers(line);

    if let Some(rest) = code.strip_prefix("fn ") {
        if let Some(name) = identifier(rest) {
            if name == "main" {
                analysis.is_entry = true;
            }
            analysis.functions.push(name);
        }
    } else if let Some(rest) = code
        .strip_prefix("struct ")
        .or_else(|| code.strip_prefix("enum "))
        .or_else(|| code.strip_prefix("trait "))
        .or_else(|| code.strip_prefix("union "))
    {
        if let Some(name) = identifier(rest) {
            analysis.classes.push(name);
        }
    } else if code.starts_with("use ") || code.starts_with("mod ") {
        push_import(line, analysis);
    }
}

fn analyze_python_line(line: &str, analysis: &mut FileAnalysis) {
    let code = line.strip_prefix("async ").unwrap_or(line);

    if let Some(rest) = code.strip_prefix("def ") {
        if let Some(name) = identifier(rest) {
            analysis.functions.push(name);
        }
    } else if let Some(rest) = code.strip_prefix("class ") {
        if let Some(name) = identifier(rest) {
            analysis.classes.push(name);
        }
    } else if line.starts_with("import ") || line.starts_with("from ") {
        push_import(line, analysis);
    }

    if line.starts_with("if __name__") {
        analysis.is_entry = true;
    }
}

fn analyze_js_line(line: &str, analysis: &mut FileAnalysis) {
    let code = line
        .strip_prefix("export default ")
        .or_else(|| line.strip_prefix("export "))
        .or_else(|| line.strip_prefix("async "))
        .unwrap_or(line);

    if let Some(rest) = code.strip_prefix("function ") {
        if let Some(name) = identifier(rest) {
            analysis.functions.push(name);
        }
    } else if let Some(rest) = code.strip_prefix("class ") {
        if let Some(name) = identifier(rest) {
            analysis.classes.push(name);
        }
    } else if let Some(rest) = code
        .strip_prefix("const ")
        .or_else(|| code.strip_prefix("let "))
    {
        if rest.contains("=>") || rest.contains("= function") {
            if let Some(name) = identifier(rest) {
                analysis.functions.push(name);
            }
        }
    }

    if line.starts_with("import ") || line.contains("require(") {
        push_import(line, analysis);
    }
}

fn analyze_go_line(line: &str, analysis: &mut FileAnalysis) {
    if let Some(rest) = line.strip_prefix("func ") {
        // Skip an optional receiver: func (s *Server) Handle(...)
        let rest = if rest.starts_with('(') {
            rest.find(')')
                .map(|idx| rest[idx + 1..].trim_start())
                .unwrap_or(rest)
        } else {
            rest
        };
        if let Some(name) = identifier(rest) {
            if name == "main" {
                analysis.is_entry = true;
            }
            analysis.functions.push(name);
        }
    } else if let Some(rest) = line.strip_prefix("type ") {
        if rest.contains(" struct") || rest.contains(" interface") {
            if let Some(name) = identifier(rest) {
                analysis.classes.push(name);
            }
        }
    } else if line.starts_with("import ") {
        push_import(line, analysis);
    }
}

fn analyze_java_like_line(line: &str, analysis: &mut FileAnalysis) {
    if let Some(idx) = line.find("class ") {
        if let Some(name) = identifier(&line[idx + "class ".len()..]) {
            analysis.classes.push(name);
        }
    }
    if line.contains("public static void main") {
        analysis.is_entry = true;
    }
    if line.starts_with("import ") {
        push_import(line, analysis);
    }
}

fn strip_rust_modifiers(mut line: &str) -> &str {
    loop {
        let before = line;
        for prefix in [
            "pub(crate) ",
            "pub(super) ",
            "pub ",
            "async ",
            "const ",
            "unsafe ",
            "default ",
            "extern \"C\" ",
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                line = rest;
            }
        }
        if line == before {
            return line;
        }
    }
}

fn identifier(text: &str) -> Option<String> {
    let end = text
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    if end == 0 {
        None
    } else {
        Some(text[..end].to_string())
    }
}

fn push_import(line: &str, analysis: &mut FileAnalysis) {
    if analysis.imports.len() < 100 {
        analysis.imports.push(truncate_chars(line, 160));
    }
}

/// Record language-specific patterns found in a file.
fn record_patterns(content: &str, lang: &str, rel_path: &Path, patterns: &mut Vec<CodePattern>) {
    let rules: &[(&str, &str, &[&str])] = match lang {
        "Rust" => &[
            (
                "Async/Await",
                "Asynchronous programming pattern",
                &["async ", ".await"],
            ),
            (
                "Result/Option error handling",
                "Explicit error handling with Result/Option",
                &["Result<", "Option<"],
            ),
            (
                "Unit testing",
                "Inline or dedicated unit tests",
                &["#[test]", "#[cfg(test)]"],
            ),
            (
                "Concurrency",
                "Shared state and spawned tasks",
                &["tokio::spawn", "thread::spawn", "Arc<", "Mutex<"],
            ),
            (
                "Serde serialization",
                "Serialization/deserialization with Serde",
                &["Serialize", "Deserialize"],
            ),
        ],
        "Python" => &[
            (
                "Async/Await",
                "Asynchronous programming pattern",
                &["async def", "await "],
            ),
            (
                "Exception handling",
                "try/except based error handling",
                &["try:", "except "],
            ),
            (
                "Unit testing",
                "pytest/unittest based tests",
                &["def test_", "import pytest", "import unittest"],
            ),
            (
                "Data modeling",
                "Dataclasses or Pydantic models",
                &["@dataclass", "BaseModel"],
            ),
        ],
        "JavaScript" | "TypeScript" => &[
            (
                "Async/Await",
                "Asynchronous programming pattern",
                &["async ", "await "],
            ),
            (
                "Exception handling",
                "try/catch based error handling",
                &["try {", "catch ("],
            ),
            (
                "Unit testing",
                "describe/it based tests",
                &["describe(", "it(", "test("],
            ),
            (
                "React hooks",
                "React functional component hooks",
                &["useState", "useEffect"],
            ),
        ],
        "Go" => &[
            (
                "Explicit error handling",
                "if err != nil error handling",
                &["if err != nil"],
            ),
            (
                "Concurrency",
                "Goroutines and synchronization",
                &["go func", "sync.", "chan "],
            ),
            ("Unit testing", "Go test functions", &["func Test"]),
        ],
        _ => &[],
    };

    for (name, description, needles) in rules {
        if !needles.iter().any(|needle| content.contains(needle)) {
            continue;
        }

        let example = content
            .lines()
            .map(str::trim)
            .find(|line| needles.iter().any(|needle| line.contains(needle)))
            .map(|line| truncate_chars(line, 160))
            .unwrap_or_default();

        if let Some(pattern) = patterns.iter_mut().find(|p| p.name == *name) {
            if pattern.examples.len() < MAX_PATTERN_EXAMPLES
                && !example.is_empty()
                && !pattern.examples.contains(&example)
            {
                pattern.examples.push(example);
            }
            if pattern.files.len() < MAX_PATTERN_FILES
                && !pattern.files.contains(&rel_path.to_path_buf())
            {
                pattern.files.push(rel_path.to_path_buf());
            }
        } else {
            patterns.push(CodePattern {
                name: name.to_string(),
                description: description.to_string(),
                examples: if example.is_empty() {
                    Vec::new()
                } else {
                    vec![example]
                },
                files: vec![rel_path.to_path_buf()],
            });
        }
    }
}

fn is_test_file(path: &Path) -> bool {
    let in_test_dir = path.components().any(|component| match component {
        Component::Normal(name) => matches!(
            name.to_string_lossy().to_lowercase().as_str(),
            "test" | "tests" | "__tests__" | "spec" | "specs"
        ),
        _ => false,
    });
    if in_test_dir {
        return true;
    }

    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    name.starts_with("test_")
        || name.ends_with("_test.rs")
        || name.ends_with("_test.py")
        || name.ends_with("_test.go")
        || name.ends_with("_spec.rb")
        || name.contains(".test.")
        || name.contains(".spec.")
}

/// Remove the most common inline Markdown decorations from a line of prose.
fn clean_markdown(text: &str) -> String {
    text.replace("**", "")
        .replace("__", "")
        .replace('`', "")
        .trim()
        .to_string()
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

fn push_unique(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|item| item == value) {
        list.push(value.to_string());
    }
}

/// Format project context for LLM consumption
pub fn format_context_for_llm(ctx: &ProjectContext) -> String {
    let mut parts = Vec::new();

    parts.push(format!("=== PROJECT: {} ===", ctx.name));
    parts.push(format!("Type: {:?}", ctx.project_type));
    parts.push(format!("Path: {}", ctx.root_path.display()));
    if let Some(readme) = &ctx.readme {
        let summary = extract_summary(readme);
        if !summary.is_empty() {
            parts.push(format!("Summary: {summary}"));
        }
    }
    parts.push(String::new());

    // Tech Stack
    if !ctx.tech_stack.languages.is_empty() {
        parts.push("=== TECH STACK ===".to_string());
        parts.push(format!(
            "Languages: {}",
            ctx.tech_stack.languages.join(", ")
        ));
        if !ctx.tech_stack.frameworks.is_empty() {
            parts.push(format!(
                "Frameworks: {}",
                ctx.tech_stack.frameworks.join(", ")
            ));
        }
        if !ctx.tech_stack.databases.is_empty() {
            parts.push(format!(
                "Databases: {}",
                ctx.tech_stack.databases.join(", ")
            ));
        }
        if !ctx.tech_stack.tools.is_empty() {
            parts.push(format!("Tools: {}", ctx.tech_stack.tools.join(", ")));
        }
        if !ctx.tech_stack.ci_cd.is_empty() {
            parts.push(format!("CI/CD: {}", ctx.tech_stack.ci_cd.join(", ")));
        }
        parts.push(String::new());
    }

    // Code Structure Summary
    if ctx.code_structure.total_files > 0 {
        parts.push("=== CODE STRUCTURE ===".to_string());
        parts.push(format!("Source files: {}", ctx.code_structure.total_files));
        parts.push(format!("Lines of code: {}", ctx.code_structure.total_lines));

        let mut langs: Vec<(&String, &LanguageStats)> =
            ctx.code_structure.languages.iter().collect();
        langs.sort_by(|a, b| b.1.files.cmp(&a.1.files).then_with(|| a.0.cmp(b.0)));
        parts.push(format!(
            "Languages: {}",
            langs
                .iter()
                .map(|(name, stats)| format!(
                    "{} ({} files, {} lines)",
                    name, stats.files, stats.lines
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        parts.push(format!("Modules: {}", ctx.code_structure.modules.len()));
        if !ctx.code_structure.entry_points.is_empty() {
            parts.push(format!(
                "Entry points: {}",
                ctx.code_structure.entry_points.join(", ")
            ));
        }
        parts.push(format!(
            "Test files: {}",
            ctx.code_structure.test_files.len()
        ));

        if !ctx.code_structure.modules.is_empty() {
            parts.push("Main modules:".to_string());
            for module in ctx.code_structure.modules.iter().take(12) {
                let mut details = Vec::new();
                if !module.functions.is_empty() {
                    details.push(format!("{} fn", module.functions.len()));
                }
                if !module.classes.is_empty() {
                    details.push(format!("{} types", module.classes.len()));
                }
                if module.is_test {
                    details.push("test".to_string());
                }
                if module.is_entry {
                    details.push("entry".to_string());
                }
                parts.push(format!(
                    "  - {}{}",
                    module.path.display(),
                    if details.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", details.join(", "))
                    }
                ));
            }
        }
        parts.push(String::new());
    }

    // Objectives
    if !ctx.objectives.is_empty() {
        parts.push("=== OBJECTIVES ===".to_string());
        for obj in ctx.objectives.iter().take(10) {
            parts.push(format!(
                "- [{:?}] {} ({})",
                obj.priority, obj.title, obj.source
            ));
        }
        parts.push(String::new());
    }

    // Patterns
    if !ctx.patterns.is_empty() {
        parts.push("=== CODE PATTERNS ===".to_string());
        for pattern in &ctx.patterns {
            parts.push(format!(
                "- {}: {} ({} files)",
                pattern.name,
                pattern.description,
                pattern.files.len()
            ));
        }
        parts.push(String::new());
    }

    // Roles
    if !ctx.roles.is_empty() {
        parts.push("=== PROJECT ROLES ===".to_string());
        for role in &ctx.roles {
            parts.push(format!("- {}: {}", role.name, role.description));
            for responsibility in role.responsibilities.iter().take(3) {
                parts.push(format!("  - {responsibility}"));
            }
        }
        parts.push(String::new());
    }

    // Key documentation
    if !ctx.docs.is_empty() {
        parts.push("=== DOCUMENTATION ===".to_string());
        for doc in &ctx.docs {
            parts.push(format!(
                "- {} ({:?}): {}",
                doc.title, doc.content_type, doc.summary
            ));
        }
        parts.push(String::new());
    }

    // Documentation excerpts (most useful signal for the model)
    if let Some(readme) = &ctx.readme {
        parts.push("=== README EXCERPT ===".to_string());
        parts.push(readme.chars().take(MAX_DOC_EXCERPT_CHARS).collect());
        parts.push(String::new());
    }
    if let Some(agents) = &ctx.agents_md {
        parts.push("=== AGENTS.md EXCERPT ===".to_string());
        parts.push(agents.chars().take(MAX_DOC_EXCERPT_CHARS).collect());
        parts.push(String::new());
    }

    // Manifest excerpts (dependency information is useful for code generation)
    const MANIFEST_NAMES: &[&str] = &["Cargo.toml", "pyproject.toml", "package.json", "go.mod"];
    let mut manifest_names: Vec<&String> = ctx
        .config_files
        .keys()
        .filter(|name| {
            Path::new(name.as_str())
                .file_name()
                .and_then(|file| file.to_str())
                .is_some_and(|file| MANIFEST_NAMES.contains(&file))
        })
        .collect();
    manifest_names.sort();
    for name in manifest_names.into_iter().take(MAX_MANIFEST_EXCERPTS) {
        if let Some(content) = ctx.config_files.get(name) {
            parts.push(format!("=== {name} (excerpt) ==="));
            parts.push(content.chars().take(MAX_MANIFEST_EXCERPT_CHARS).collect());
            parts.push(String::new());
        }
    }

    // Configuration summary
    if !ctx.config_files.is_empty() {
        parts.push("=== KEY CONFIG FILES ===".to_string());
        let mut names: Vec<&String> = ctx.config_files.keys().collect();
        names.sort();
        for name in names {
            parts.push(format!("- {name}"));
        }
        parts.push(String::new());
    }

    let mut output = parts.join("\n");
    if output.chars().count() > MAX_CONTEXT_CHARS {
        output = output.chars().take(MAX_CONTEXT_CHARS).collect();
        output.push_str("\n... (project context truncated)");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_file(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_context_analysis() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(
            root,
            "Cargo.toml",
            r#"
[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
"#,
        );
        write_file(
            root,
            "src/main.rs",
            r#"
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
"#,
        );
        write_file(
            root,
            "README.md",
            "# Test Project\n\nA simple Rust project using Tokio and Serde.\n\n## Objectives\n- Build a simple async user fetcher\n- Demonstrate serde serialization\n",
        );

        let analyzer = ContextAnalyzer::new(root.to_path_buf());
        let ctx = analyzer.analyze().unwrap();

        assert_eq!(ctx.project_type, ProjectType::Rust);
        assert!(ctx.tech_stack.languages.contains(&"Rust".to_string()));
        assert!(ctx.tech_stack.frameworks.contains(&"Tokio".to_string()));
        assert!(ctx.readme.is_some());
        assert!(!ctx.objectives.is_empty());
        assert_eq!(ctx.code_structure.total_files, 1);
        assert!(!ctx.code_structure.modules.is_empty());
        assert!(ctx
            .code_structure
            .entry_points
            .iter()
            .any(|e| e.contains("main.rs")));
        assert!(ctx.code_structure.languages["Rust"].functions >= 2);
        assert!(ctx.code_structure.languages["Rust"].classes >= 1);
        assert!(ctx.patterns.iter().any(|p| p.name == "Async/Await"));
        assert!(ctx
            .patterns
            .iter()
            .any(|p| p.name == "Result/Option error handling"));
        assert!(ctx.roles.iter().any(|r| r.name == "Backend Developer"));

        let formatted = format_context_for_llm(&ctx);
        assert!(formatted.contains("=== CODE STRUCTURE ==="));
        assert!(formatted.contains("=== README EXCERPT ==="));
    }

    #[test]
    fn test_ignores_build_dirs_and_binary_files() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(root, "Cargo.toml", "[package]\nname = \"demo\"\n");
        write_file(root, "src/main.rs", "fn main() {}\n");
        write_file(root, "target/generated.rs", "fn nope() {}\n");
        write_file(root, ".git/config", "not source\n");
        write_file(root, "node_modules/pkg/index.js", "function nope() {}\n");
        fs::write(root.join("src/binary.rs"), [0xffu8, 0xfe, 0x00]).unwrap();

        let analyzer = ContextAnalyzer::new(root.to_path_buf());
        let ctx = analyzer.analyze().unwrap();

        assert_eq!(ctx.code_structure.total_files, 1);
        assert_eq!(ctx.code_structure.languages["Rust"].files, 1);
        assert!(!ctx
            .code_structure
            .modules
            .iter()
            .any(|m| m.path.to_string_lossy().contains("target")));
        assert!(!ctx
            .code_structure
            .modules
            .iter()
            .any(|m| m.path.to_string_lossy().contains("node_modules")));
    }

    #[test]
    fn test_detects_nested_manifests() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(
            root,
            "app/Cargo.toml",
            "[package]\nname = \"app\"\n\n[dependencies]\ntokio = \"1\"\nserde = \"1\"\n",
        );
        write_file(
            root,
            "app/src/lib.rs",
            "pub async fn run() -> Result<(), Box<dyn std::error::Error>> { Ok(()) }\n",
        );

        let analyzer = ContextAnalyzer::new(root.to_path_buf());
        let ctx = analyzer.analyze().unwrap();

        assert!(ctx.config_files.contains_key("app/Cargo.toml"));
        assert!(ctx.tech_stack.languages.contains(&"Rust".to_string()));
        assert!(ctx.tech_stack.frameworks.contains(&"Tokio".to_string()));
        assert!(ctx.tech_stack.tools.contains(&"Serde".to_string()));
    }

    #[test]
    fn test_dominant_language_wins_over_root_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(root, "pyproject.toml", "[project]\nname = \"mixed\"\n");
        write_file(root, "src/main.rs", "fn main() {}\n");
        write_file(root, "src/lib.rs", "pub fn helper() {}\n");
        write_file(root, "src/util.rs", "pub fn util() {}\n");

        let analyzer = ContextAnalyzer::new(root.to_path_buf());
        let ctx = analyzer.analyze().unwrap();

        assert_eq!(ctx.project_type, ProjectType::Rust);
    }

    #[test]
    fn test_objectives_are_section_bullets_and_capped() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let mut readme = String::from("# Project\n\nIntro paragraph.\n\n## Goals\n");
        for i in 0..40 {
            readme.push_str(&format!("- Ship feature {i}\n"));
        }
        readme.push_str("\n## Other\n- Not an objective\n");
        write_file(root, "README.md", &readme);

        let analyzer = ContextAnalyzer::new(root.to_path_buf());
        let ctx = analyzer.analyze().unwrap();

        assert_eq!(ctx.objectives.len(), MAX_OBJECTIVES);
        assert!(ctx
            .objectives
            .iter()
            .all(|o| o.title.starts_with("Ship feature")));
        assert!(!ctx.objectives.iter().any(|o| o.title == "Not an objective"));
    }

    #[test]
    fn test_format_context_is_bounded_and_includes_excerpts() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let long_readme = format!("# Big\n\n{}\n", "lorem ipsum ".repeat(5_000));
        write_file(root, "README.md", &long_readme);
        write_file(
            root,
            "AGENTS.md",
            "# Rules\n\nAlways test before committing.\n",
        );
        write_file(root, "src/main.rs", "fn main() {}\n");

        let analyzer = ContextAnalyzer::new(root.to_path_buf());
        let ctx = analyzer.analyze().unwrap();
        let formatted = format_context_for_llm(&ctx);

        assert!(formatted.contains("=== README EXCERPT ==="));
        assert!(formatted.contains("=== AGENTS.md EXCERPT ==="));
        assert!(formatted.chars().count() <= MAX_CONTEXT_CHARS + 100);
    }

    #[test]
    fn test_ignores_code_inside_raw_strings() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(
            root,
            "src/lib.rs",
            r##"
pub const SAMPLE: &str = r#"
async fn main() {
    println!("fake entry point");
}
"#;

pub fn real_function() {}
"##,
        );

        let analyzer = ContextAnalyzer::new(root.to_path_buf());
        let ctx = analyzer.analyze().unwrap();

        assert!(ctx.code_structure.entry_points.is_empty());
        assert_eq!(ctx.code_structure.languages["Rust"].functions, 1);
        assert!(ctx.code_structure.modules[0]
            .functions
            .contains(&"real_function".to_string()));
    }

    #[test]
    fn test_objectives_strip_inline_markdown() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(
            root,
            "README.md",
            "# Project\n\n## Mission\n- **Open source first:** no cloud lock-in\n",
        );

        let analyzer = ContextAnalyzer::new(root.to_path_buf());
        let ctx = analyzer.analyze().unwrap();

        assert_eq!(ctx.objectives.len(), 1);
        assert_eq!(
            ctx.objectives[0].title,
            "Open source first: no cloud lock-in"
        );
    }

    #[test]
    fn test_is_test_file() {
        assert!(is_test_file(Path::new("tests/foo.py")));
        assert!(is_test_file(Path::new("src/foo_test.rs")));
        assert!(is_test_file(Path::new("app.test.ts")));
        assert!(!is_test_file(Path::new("src/latest.rs")));
        assert!(!is_test_file(Path::new("src/contest.py")));
    }
}
