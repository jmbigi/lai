//! spec.rs - Spec-Driven Development (v4.0, Mejora 4).
//! Transforma la petición del usuario en una especificación estructurada
//! (TaskSpec ↔ SPEC.md) que funciona como contrato de aceptación.
//! La especificación declara su `verification_scope`: qué niveles de
//! verificación están en alcance y cuáles quedan fuera.

use crate::harness::TaskSpec;
use crate::llm::LlmClient;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Genera la especificación con validación estructural y hasta 2 reintentos.
pub async fn generate_spec(llm: &LlmClient, description: &str, task_type: &str) -> Result<TaskSpec> {
    let mut prompt = build_prompt(description, task_type);
    let mut last_error = String::new();

    for attempt in 0..3 {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(300),
            llm.chat(&prompt, false, true),
        )
        .await;

        let content = match result {
            Ok(r) if r.error.is_none() => r.content,
            Ok(r) => {
                last_error = format!("LLM error: {}", r.error.unwrap_or_default());
                prompt = format!("{}\n\nError anterior: {}\nCorrige y devuelve SOLO el JSON.", prompt, last_error);
                continue;
            }
            Err(_) => {
                last_error = "timeout".to_string();
                prompt = format!("{}\n\nError anterior: timeout\nDevuelve SOLO el JSON.", prompt);
                continue;
            }
        };

        match parse_spec(&content) {
            Ok(spec) => return Ok(spec),
            Err(e) => {
                last_error = e;
                let _ = attempt;
                prompt = format!(
                    "{}\n\nTu respuesta anterior no era válida: {}\nDevuelve SOLO el JSON con TODOS los campos.",
                    prompt, last_error
                );
            }
        }
    }

    anyhow::bail!("No se pudo generar una especificación válida: {}", last_error)
}

fn build_prompt(description: &str, task_type: &str) -> String {
    format!(
        r#"You are lai, an autonomous software engineering agent. Transform this request into a structured specification.

Task Type: {}
Request: {}

Return ONLY valid JSON with these fields (all required):
- objective: clear, concise objective
- current_state: what is known about the current state
- problem: the problem to solve (or null)
- scope: array of areas/files likely affected
- constraints: array of technical/business constraints
- affected_files: array of specific files likely to be modified
- dependencies: array of external dependencies
- acceptance_criteria: array of measurable success criteria
- required_tests: array of test commands that must pass (e.g. "python3 -m pytest tests/ -q")
- risks: array of potential risks and side effects
- verification_scope: array of verification levels IN SCOPE, from this list:
  [syntax, lint, static, types, unit_tests, integration, functional, regression, security, contracts]
  Include ONLY the levels that apply to this task. Anything not listed is OUT OF SCOPE
  and will be reported as a declared gap.

Return ONLY valid JSON."#,
        task_type, description
    )
}

/// Parsea y valida la respuesta del LLM como TaskSpec.
pub fn parse_spec(content: &str) -> std::result::Result<TaskSpec, String> {
    let json_str = extract_json(content);
    let mut spec: TaskSpec =
        serde_json::from_str(json_str.trim()).map_err(|e| format!("JSON inválido: {}", e))?;

    if spec.objective.trim().is_empty() {
        return Err("objective vacío".to_string());
    }
    if spec.acceptance_criteria.is_empty() {
        return Err("acceptance_criteria vacío".to_string());
    }
    if spec.created_at.is_empty() {
        spec.created_at = chrono::Utc::now().to_rfc3339();
    }
    spec.updated_at = chrono::Utc::now().to_rfc3339();
    Ok(spec)
}

/// Extrae el bloque JSON de una respuesta (con o sin fences markdown).
fn extract_json(content: &str) -> &str {
    if let Some(start) = content.find("```json") {
        let after = &content[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim();
        }
    }
    if let Some(start) = content.find("```") {
        let after = &content[start + 3..];
        if let Some(end) = after.find("```") {
            return after[..end].trim();
        }
    }
    content
}

/// Renderiza la especificación como SPEC.md.
pub fn spec_to_markdown(spec: &TaskSpec) -> String {
    let list = |items: &[String]| {
        if items.is_empty() {
            "- (ninguno)".to_string()
        } else {
            items.iter().map(|i| format!("- {}", i)).collect::<Vec<_>>().join("\n")
        }
    };
    format!(
        "# SPEC {}\n\n- **Tipo:** {}\n- **Creada:** {}\n- **Actualizada:** {}\n\n## Objetivo\n{}\n\n## Estado actual\n{}\n\n## Problema\n{}\n\n## Alcance (scope)\n{}\n\n## Restricciones\n{}\n\n## Archivos afectados\n{}\n\n## Dependencias\n{}\n\n## Criterios de aceptación\n{}\n\n## Tests requeridos\n{}\n\n## Riesgos\n{}\n\n## Alcance de verificación\n{}\n",
        spec.id,
        spec.task_type,
        spec.created_at,
        spec.updated_at,
        spec.objective,
        spec.current_state,
        spec.problem.clone().unwrap_or_else(|| "(no identificado)".to_string()),
        list(&spec.scope),
        list(&spec.constraints),
        list(&spec.affected_files),
        list(&spec.dependencies),
        list(&spec.acceptance_criteria),
        list(&spec.required_tests),
        list(&spec.risks),
        if spec.verification_scope.is_empty() {
            "- syntax, unit_tests (por defecto)".to_string()
        } else {
            list(&spec.verification_scope)
        },
    )
}

/// Persiste la especificación como SPEC.md en `<dir>/.lai/specs/<id>.md`.
pub fn save_spec(spec: &TaskSpec, dir: &Path) -> Result<PathBuf> {
    let spec_dir = dir.join(".lai").join("specs");
    std::fs::create_dir_all(&spec_dir)?;
    let path = spec_dir.join(format!("{}.md", sanitize(&spec.id)));
    std::fs::write(&path, spec_to_markdown(spec))?;
    Ok(path)
}

fn sanitize(id: &str) -> String {
    id.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{
        "objective": "Fix the bug",
        "current_state": "broken",
        "problem": "null",
        "scope": ["src"],
        "constraints": [],
        "affected_files": ["src/a.py"],
        "dependencies": [],
        "acceptance_criteria": ["tests pass"],
        "required_tests": ["python3 -m pytest -q"],
        "risks": [],
        "verification_scope": ["syntax", "unit_tests"]
    }"#;

    #[test]
    fn test_parse_valid_spec() {
        let spec = parse_spec(VALID).unwrap();
        assert_eq!(spec.objective, "Fix the bug");
        assert_eq!(spec.verification_scope.len(), 2);
        assert!(!spec.created_at.is_empty());
    }

    #[test]
    fn test_parse_spec_from_markdown_fence() {
        let content = format!("Here is the spec:\n```json\n{}\n```\nDone.", VALID);
        let spec = parse_spec(&content).unwrap();
        assert!(spec.acceptance_criteria.contains(&"tests pass".to_string()));
    }

    #[test]
    fn test_parse_spec_rejects_empty_objective() {
        let bad = VALID.replace("Fix the bug", "");
        assert!(parse_spec(&bad).is_err());
    }

    #[test]
    fn test_spec_to_markdown() {
        let spec = parse_spec(VALID).unwrap();
        let md = spec_to_markdown(&spec);
        assert!(md.contains("# SPEC"));
        assert!(md.contains("tests pass"));
        assert!(md.contains("unit_tests"));
    }

    #[test]
    fn test_save_spec() {
        let dir = std::env::temp_dir().join(format!("lai-spec-test-{}", uuid::Uuid::new_v4()));
        let spec = parse_spec(VALID).unwrap();
        let path = save_spec(&spec, &dir).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Fix the bug"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}