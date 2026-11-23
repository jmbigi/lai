//! spec.rs - Spec-driven development (Tarea 5).
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    pub goal: String,
    pub current_state: String,
    pub problem: String,
    pub scope: String,
    pub constraints: Vec<String>,
    pub affected_files: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub risks: Vec<String>,
    pub verification_plan: Vec<String>,
}

impl Spec {
    pub fn new_from_goal(goal: &str, context_files: &[String]) -> Spec {
        Spec {
            goal: goal.to_string(),
            current_state: "Repositorio en estado actual (pendiente de inspección)".to_string(),
            problem: format!("Implementar: {}", goal),
            scope: "Modificaciones limitadas a archivos relacionados con el objetivo".to_string(),
            constraints: vec!["No modificar archivos fuera del alcance definido".to_string(), "No introducir regresiones en tests existentes".to_string()],
            affected_files: context_files.clone(),
            acceptance_criteria: vec!["El código compila sin errores".to_string(), "Las pruebas existentes pasan".to_string(), "El código cumple los criterios de aceptación definidos".to_string()],
            risks: vec!["Posibles efectos secundarios en módulos dependientes".to_string()],
            verification_plan: vec!["Ejecutar suite de pruebas existente".to_string(), "Verificar sintaxis del código modificado".to_string()],
        }
    }
}

pub fn generate_spec(spec: &Spec, output_path: &Path) -> Result<(), anyhow::Error> {
    let mut content = String::new();
    content.push_str("# SPEC.md\n\n");
    content.push_str(&format!("# Meta\n{}\n\n", spec.goal));
    content.push_str(&format!("# Estado actual\n{}\n\n", spec.current_state));
    content.push_str(&format!("# Problema\n{}\n\n", spec.problem));
    content.push_str(&format!("# Alcance\n{}\n\n", spec.scope));
    content.push_str("# Restricciones\n");
    for c in &spec.constraints { content.push_str(&format!("- {}\n", c)); }
    content.push_str("\n# Archivos afectados\n");
    for f in &spec.affected_files { content.push_str(&format!("- {}\n", f)); }
    content.push_str("# Criterios de aceptación\n");
    for c in &spec.acceptance_criteria { content.push_str(&format!("- {}\n", c)); }
    content.push_str("# Riesgos\n");
    for r in &spec.risks { content.push_str(&format!("- {}\n", r)); }
    content.push_str("# Plan de verificación\n");
    for v in &spec.verification_plan { content.push_str(&format!("- {}\n", v)); }
    content.push_str("\n---\n*Generado por lai - Desarrollo guiado por especificación*\n");
    fs::write(output_path, content)?;
    Ok(())
}
