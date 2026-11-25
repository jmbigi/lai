//! planner.rs - DAG-based planner with SPEC.md generation (v5.0)
//! Principles: Determinism, Verification scope explicit, SPEC.md immutable after approval

use crate::telemetry::{end_task_span, task_span};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::Instant;
use uuid::Uuid;

#[derive(clap::Subcommand, Debug, Clone, Serialize, Deserialize)]
pub enum PlanCommands {
    /// Generate plan from task
    Generate {
        #[arg(help = "Task description")]
        task: String,
    },
    /// Show SPEC.md
    Show {
        #[arg(help = "Plan ID")]
        id: String,
    },
    /// Execute plan
    Execute {
        #[arg(help = "Plan ID")]
        id: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum VerificationLevel {
    Syntax = 1,
    Lint = 2,
    TypeCheck = 3,
    UnitTests = 4,
    IntegrationTests = 5,
    RegressionTests = 6,
    SecurityScan = 7,
    Performance = 8,
}

impl VerificationLevel {
    pub fn from_u32(n: u32) -> Option<Self> {
        match n {
            1 => Some(VerificationLevel::Syntax),
            2 => Some(VerificationLevel::Lint),
            3 => Some(VerificationLevel::TypeCheck),
            4 => Some(VerificationLevel::UnitTests),
            5 => Some(VerificationLevel::IntegrationTests),
            6 => Some(VerificationLevel::RegressionTests),
            7 => Some(VerificationLevel::SecurityScan),
            8 => Some(VerificationLevel::Performance),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> u32 {
        *self as u32
    }

    pub fn name(&self) -> &'static str {
        match self {
            VerificationLevel::Syntax => "syntax",
            VerificationLevel::Lint => "lint",
            VerificationLevel::TypeCheck => "type_check",
            VerificationLevel::UnitTests => "unit_tests",
            VerificationLevel::IntegrationTests => "integration_tests",
            VerificationLevel::RegressionTests => "regression_tests",
            VerificationLevel::SecurityScan => "security_scan",
            VerificationLevel::Performance => "performance",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            VerificationLevel::Syntax => "Syntax validation (compile/parse)",
            VerificationLevel::Lint => "Static analysis (linting, formatting)",
            VerificationLevel::TypeCheck => "Type checking (mypy, rustc --emit=metadata)",
            VerificationLevel::UnitTests => "Unit test execution",
            VerificationLevel::IntegrationTests => "Integration test execution",
            VerificationLevel::RegressionTests => "Regression test suite",
            VerificationLevel::SecurityScan => "Security scanning (bandit, cargo audit)",
            VerificationLevel::Performance => "Performance benchmarks",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationScope {
    pub in_scope: Vec<VerificationLevel>,
    pub out_of_scope: Vec<VerificationLevel>,
    pub rationale: String,
}

impl Default for VerificationScope {
    fn default() -> Self {
        Self {
            in_scope: vec![
                VerificationLevel::Syntax,
                VerificationLevel::Lint,
                VerificationLevel::TypeCheck,
                VerificationLevel::UnitTests,
            ],
            out_of_scope: vec![
                VerificationLevel::IntegrationTests,
                VerificationLevel::RegressionTests,
                VerificationLevel::SecurityScan,
                VerificationLevel::Performance,
            ],
            rationale: "Default scope covers basic correctness verification".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tool: Option<String>,
    pub arguments: serde_json::Value,
    pub depends_on: Vec<String>,
    pub verification_scope: VerificationScope,
    pub estimated_tokens: u32,
    pub risk_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub task_description: String,
    pub created_at: String,
    pub updated_at: String,
    pub steps: Vec<PlanStep>,
    pub verification_scope: VerificationScope,
    pub total_estimated_tokens: u32,
    pub status: PlanStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PlanStatus {
    Draft,
    Approved,
    Executing,
    Completed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    pub id: String,
    pub version: u32,
    pub task_description: String,
    pub acceptance_criteria: Vec<String>,
    pub verification_scope: VerificationScope,
    pub constraints: Vec<String>,
    pub assumptions: Vec<String>,
    pub created_at: String,
    pub approved_at: Option<String>,
    pub approved_by: Option<String>,
    pub plan_id: Option<String>,
}

impl Spec {
    pub fn new(task_description: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            version: 1,
            task_description: task_description.to_string(),
            acceptance_criteria: Vec::new(),
            verification_scope: VerificationScope::default(),
            constraints: Vec::new(),
            assumptions: Vec::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            approved_at: None,
            approved_by: None,
            plan_id: None,
        }
    }

    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str(&format!("# SPEC: {}\n\n", self.task_description));
        md.push_str(&format!("**ID**: {}\n", self.id));
        md.push_str(&format!("**Version**: {}\n", self.version));
        md.push_str(&format!("**Created**: {}\n", self.created_at));
        if let Some(approved_at) = &self.approved_at {
            md.push_str(&format!(
                "**Approved**: {} by {}\n",
                approved_at,
                self.approved_by.as_deref().unwrap_or("unknown")
            ));
        }
        md.push('\n');

        md.push_str("## Acceptance Criteria\n\n");
        if self.acceptance_criteria.is_empty() {
            md.push_str("*None specified*\n\n");
        } else {
            for (i, criterion) in self.acceptance_criteria.iter().enumerate() {
                md.push_str(&format!("{}. {}\n", i + 1, criterion));
            }
            md.push('\n');
        }

        md.push_str("## Verification Scope\n\n");
        md.push_str("### In Scope\n\n");
        for level in &self.verification_scope.in_scope {
            md.push_str(&format!(
                "- **Level {} - {}**: {}\n",
                level.as_u32(),
                level.name(),
                level.description()
            ));
        }
        md.push_str("\n### Out of Scope\n\n");
        for level in &self.verification_scope.out_of_scope {
            md.push_str(&format!(
                "- **Level {} - {}**: {}\n",
                level.as_u32(),
                level.name(),
                level.description()
            ));
        }
        md.push_str(&format!(
            "\n**Rationale**: {}\n\n",
            self.verification_scope.rationale
        ));

        if !self.constraints.is_empty() {
            md.push_str("## Constraints\n\n");
            for constraint in &self.constraints {
                md.push_str(&format!("- {}\n", constraint));
            }
            md.push('\n');
        }

        if !self.assumptions.is_empty() {
            md.push_str("## Assumptions\n\n");
            for assumption in &self.assumptions {
                md.push_str(&format!("- {}\n", assumption));
            }
            md.push('\n');
        }

        md.push_str("---\n");
        md.push_str("*This SPEC is immutable after approval. Any changes require human review.*\n");

        md
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        fs::write(path, self.to_markdown())
    }
}

pub struct Planner {
    llm_base_url: String,
    llm_model: String,
}

impl Planner {
    pub fn new(llm_base_url: &str, llm_model: &str) -> Self {
        Self {
            llm_base_url: llm_base_url.to_string(),
            llm_model: llm_model.to_string(),
        }
    }

    pub async fn generate_spec(&self, task: &str, context: &str) -> Result<Spec, String> {
        let prompt = format!(
            r#"You are a senior software engineer creating a SPEC.md for a development task.

TASK: {}

CONTEXT:
{}

Create a comprehensive SPEC.md with:
1. Clear acceptance criteria (numbered, testable)
2. Verification scope (which levels 1-8 are in/out of scope)
3. Constraints (technical, architectural, security)
4. Assumptions

Output ONLY valid JSON matching this schema:
{{
  "acceptance_criteria": ["criterion 1", "criterion 2"],
  "verification_scope": {{
    "in_scope": [1, 2, 3, 4],
    "out_of_scope": [5, 6, 7, 8],
    "rationale": "explanation"
  }},
  "constraints": ["constraint 1"],
  "assumptions": ["assumption 1"]
}}

No markdown, no explanations, only JSON."#,
            task, context
        );

        let client = crate::llm::LlmClient::new(self.llm_base_url.clone(), self.llm_model.clone());
        let response = client.chat(&prompt).await.map_err(|e| e.to_string())?;

        let parsed: serde_json::Value = serde_json::from_str(&response)
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        let mut spec = Spec::new(task);
        spec.acceptance_criteria = parsed["acceptance_criteria"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        if let Some(scope) = parsed["verification_scope"].as_object() {
            let in_scope = scope["in_scope"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            v.as_u64()
                                .and_then(|n| VerificationLevel::from_u32(n as u32))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let out_of_scope = scope["out_of_scope"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            v.as_u64()
                                .and_then(|n| VerificationLevel::from_u32(n as u32))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let rationale = scope["rationale"].as_str().unwrap_or("").to_string();
            spec.verification_scope = VerificationScope {
                in_scope,
                out_of_scope,
                rationale,
            };
        }

        spec.constraints = parsed["constraints"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        spec.assumptions = parsed["assumptions"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(spec)
    }

    pub async fn generate_plan(&self, spec: &Spec, repo_map: &str) -> Result<Plan, String> {
        let prompt = format!(
            r#"You are a senior software engineer creating an execution plan from a SPEC.

SPEC:
{}

REPO MAP:
{}

Create a detailed execution plan as a DAG of steps. Each step must:
1. Have a unique ID
2. Have a clear name and description
3. Specify which tool to use (read_file, write_file, run_command, grep, etc.)
4. List dependencies (step IDs this step depends on)
5. Declare verification scope (which levels 1-8 are in/out of scope for this step)
6. Estimate token cost
7. Assess risk level (low/medium/high/critical)

Output ONLY valid JSON matching this schema:
{{
  "steps": [
    {{
      "id": "step-1",
      "name": "Analyze codebase",
      "description": "Read key files to understand structure",
      "tool": "read_file",
      "arguments": {{"path": "src/main.rs"}},
      "depends_on": [],
      "verification_scope": {{
        "in_scope": [1],
        "out_of_scope": [2,3,4,5,6,7,8],
        "rationale": "Initial exploration only"
      }},
      "estimated_tokens": 2000,
      "risk_level": "low"
    }}
  ],
  "total_estimated_tokens": 5000
}}

No markdown, no explanations, only JSON."#,
            spec.to_markdown(),
            repo_map
        );

        let client = crate::llm::LlmClient::new(self.llm_base_url.clone(), self.llm_model.clone());
        let response = client.chat(&prompt).await.map_err(|e| e.to_string())?;

        let parsed: serde_json::Value = serde_json::from_str(&response)
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        let mut steps = Vec::new();
        let mut total_tokens = 0u32;

        if let Some(steps_arr) = parsed["steps"].as_array() {
            for step_val in steps_arr {
                let verification_scope =
                    if let Some(scope) = step_val["verification_scope"].as_object() {
                        let in_scope = scope["in_scope"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| {
                                        v.as_u64()
                                            .and_then(|n| VerificationLevel::from_u32(n as u32))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let out_of_scope = scope["out_of_scope"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| {
                                        v.as_u64()
                                            .and_then(|n| VerificationLevel::from_u32(n as u32))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let rationale = scope["rationale"].as_str().unwrap_or("").to_string();
                        VerificationScope {
                            in_scope,
                            out_of_scope,
                            rationale,
                        }
                    } else {
                        VerificationScope::default()
                    };

                let step = PlanStep {
                    id: step_val["id"].as_str().unwrap_or("").to_string(),
                    name: step_val["name"].as_str().unwrap_or("").to_string(),
                    description: step_val["description"].as_str().unwrap_or("").to_string(),
                    tool: step_val["tool"].as_str().map(|s| s.to_string()),
                    arguments: step_val["arguments"].clone(),
                    depends_on: step_val["depends_on"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default(),
                    verification_scope,
                    estimated_tokens: step_val["estimated_tokens"].as_u64().unwrap_or(1000) as u32,
                    risk_level: step_val["risk_level"]
                        .as_str()
                        .unwrap_or("medium")
                        .to_string(),
                };
                total_tokens += step.estimated_tokens;
                steps.push(step);
            }
        }

        let plan = Plan {
            id: Uuid::new_v4().to_string(),
            task_description: spec.task_description.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            steps,
            verification_scope: spec.verification_scope.clone(),
            total_estimated_tokens: total_tokens,
            status: PlanStatus::Draft,
        };

        Ok(plan)
    }

    pub fn validate_dag(&self, plan: &Plan) -> Result<(), String> {
        let step_ids: HashSet<String> = plan.steps.iter().map(|s| s.id.clone()).collect();

        // Check all dependencies exist
        for step in &plan.steps {
            for dep in &step.depends_on {
                if !step_ids.contains(dep) {
                    return Err(format!(
                        "Step {} depends on non-existent step {}",
                        step.id, dep
                    ));
                }
            }
        }

        // Check for cycles using Kahn's algorithm
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();

        for step in &plan.steps {
            in_degree.insert(step.id.clone(), 0);
            adj.insert(step.id.clone(), Vec::new());
        }

        for step in &plan.steps {
            for dep in &step.depends_on {
                *in_degree.get_mut(&step.id).unwrap() += 1;
                adj.get_mut(dep).unwrap().push(step.id.clone());
            }
        }

        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(k, _)| k.clone())
            .collect();
        let mut processed = 0;

        while let Some(node) = queue.pop() {
            processed += 1;
            for neighbor in adj.get(&node).unwrap_or(&Vec::new()) {
                let deg = in_degree.get_mut(neighbor).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push(neighbor.clone());
                }
            }
        }

        if processed != plan.steps.len() {
            return Err("Plan contains cycles".to_string());
        }

        Ok(())
    }

    pub fn topological_order(&self, plan: &Plan) -> Vec<PlanStep> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();

        for step in &plan.steps {
            in_degree.insert(step.id.clone(), 0);
            adj.insert(step.id.clone(), Vec::new());
        }

        for step in &plan.steps {
            for dep in &step.depends_on {
                *in_degree.get_mut(&step.id).unwrap() += 1;
                adj.get_mut(dep).unwrap().push(step.id.clone());
            }
        }

        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(k, _)| k.clone())
            .collect();
        let mut result = Vec::new();

        while let Some(node) = queue.pop() {
            if let Some(step) = plan.steps.iter().find(|s| s.id == node) {
                result.push(step.clone());
            }
            for neighbor in adj.get(&node).unwrap_or(&Vec::new()) {
                let deg = in_degree.get_mut(neighbor).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push(neighbor.clone());
                }
            }
        }

        result
    }

    pub fn save_plan(&self, plan: &Plan, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(plan).unwrap();
        fs::write(path, json)
    }

    pub fn load_plan(path: &Path) -> Result<Plan, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    }
}

pub async fn create_plan_from_task(
    task: &str,
    llm_base_url: &str,
    llm_model: &str,
    working_dir: &Path,
) -> Result<(Spec, Plan), String> {
    let task_id = format!("plan-{}", Uuid::new_v4());
    task_span(&task_id, "plan_generation", task);
    let start = Instant::now();

    let planner = Planner::new(llm_base_url, llm_model);

    // Build context
    let context = build_project_context(working_dir);

    // Generate SPEC
    let mut spec = planner.generate_spec(task, &context).await?;
    spec.id = task_id.clone();

    // Save SPEC
    let spec_path = working_dir.join("SPEC.md");
    spec.save(&spec_path).map_err(|e| e.to_string())?;

    // Build repo map
    let repo_map = crate::repomap::RepoMap::build(working_dir);
    let repo_map_text = repo_map.to_text(8000);

    // Generate Plan
    let mut plan = planner.generate_plan(&spec, &repo_map_text).await?;
    plan.id = task_id.clone();
    spec.plan_id = Some(plan.id.clone());

    // Validate DAG
    planner.validate_dag(&plan).map_err(|e| e.to_string())?;

    // Save plan
    let plan_path = working_dir.join("PLAN.json");
    planner
        .save_plan(&plan, &plan_path)
        .map_err(|e| e.to_string())?;

    // Update SPEC with plan_id
    spec.save(&spec_path).map_err(|e| e.to_string())?;

    end_task_span(
        &task_id,
        true,
        start.elapsed(),
        &format!("Generated plan with {} steps", plan.steps.len()),
    );

    Ok((spec, plan))
}

fn build_project_context(working_dir: &Path) -> String {
    let mut parts = Vec::new();

    // README
    for readme in ["README.md", "README.txt", "README.rst"] {
        let path = working_dir.join(readme);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                parts.push(format!(
                    "=== {} ===\n{}",
                    readme,
                    content.chars().take(5000).collect::<String>()
                ));
                break;
            }
        }
    }

    // AGENTS.md
    let agents_path = working_dir.join("AGENTS.md");
    if agents_path.exists() {
        if let Ok(content) = fs::read_to_string(&agents_path) {
            parts.push(format!(
                "=== AGENTS.md ===\n{}",
                content.chars().take(5000).collect::<String>()
            ));
        }
    }

    // Key files
    let key_files = [
        "Cargo.toml",
        "pyproject.toml",
        "package.json",
        "go.mod",
        "Dockerfile",
        "Makefile",
        ".gitignore",
    ];
    for kf in key_files {
        let path = working_dir.join(kf);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                parts.push(format!(
                    "=== {} ===\n{}",
                    kf,
                    content.chars().take(2000).collect::<String>()
                ));
            }
        }
    }

    // Structure
    if let Ok(entries) = fs::read_dir(working_dir) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if entry.path().is_dir() {
                dirs.push(name);
            } else {
                files.push(name);
            }
        }
        dirs.sort();
        files.sort();
        parts.push(format!(
            "=== STRUCTURE ===\nDirs: {}\nFiles: {}",
            dirs.join(", "),
            files.join(", ")
        ));
    }

    parts.join("\n\n")
}
