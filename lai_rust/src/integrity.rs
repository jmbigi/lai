//! integrity.rs - Verificación de integridad de herramientas (v4.0, Mejora 8).
//! Cada herramienta queda "fijada" (pinned) mediante el hash SHA-256 de su
//! definición completa: nombre, descripción, esquema de entrada, esquema de
//! salida, nivel de riesgo y permisos. Una definición que cambie —aunque sea
//! su descripción— invalida el pin y la invocación se DENIEGA (fail-closed).
//!
//! Principio: el LLM no es un control de integridad. La alineación del modelo
//! no sustituye la verificación determinista del runtime.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Hash SHA-256 en hexadecimal de un contenido (compartido por los
/// subsistemas de integridad: definiciones, journal, pins MCP).
pub fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}
use std::collections::HashMap;

/// Definición completa de una herramienta (fuente del hash de integridad).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub risk_level: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub pinned_version: String,
}

impl ToolDefinition {
    /// Hash SHA-256 de la definición completa (JSON canónico determinista).
    pub fn definition_hash(&self) -> String {
        let canonical = serde_json::to_vec(self).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(&canonical);
        format!("{:x}", hasher.finalize())
    }
}

/// Registro de integridad: nombre -> hash fijado al registrar la herramienta.
#[derive(Debug, Clone, Default)]
pub struct IntegrityRegistry {
    pinned: HashMap<String, String>,
    definitions: HashMap<String, ToolDefinition>,
}

impl IntegrityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fija (pin) una definición. Si el nombre ya estaba fijado con otro hash,
    /// se reemplaza el pin (nueva evaluación de confianza explícita).
    pub fn pin(&mut self, def: ToolDefinition) -> String {
        let hash = def.definition_hash();
        self.pinned.insert(def.name.clone(), hash.clone());
        self.definitions.insert(def.name.clone(), def);
        hash
    }

    /// Verifica la integridad de una herramienta antes de ejecutarla.
    /// `Err(reason)` => la operación debe denegarse (fail-closed).
    pub fn verify(&self, name: &str) -> Result<(), String> {
        let Some(def) = self.definitions.get(name) else {
            return Err(format!("unknown tool '{}' (sin definición fijada)", name));
        };
        let expected = self.pinned.get(name).ok_or_else(|| format!("tool '{}' sin hash fijado", name))?;
        let actual = def.definition_hash();
        if &actual != expected {
            return Err(format!(
                "definition_hash mismatch para '{}': esperado {}.., actual {}..",
                name,
                &expected[..expected.len().min(12)],
                &actual[..actual.len().min(12)]
            ));
        }
        Ok(())
    }

    /// Revalida todas las definiciones fijadas.
    pub fn verify_all(&self) -> Result<(), String> {
        for name in self.pinned.keys() {
            self.verify(name)?;
        }
        Ok(())
    }

    pub fn is_pinned(&self, name: &str) -> bool {
        self.pinned.contains_key(name)
    }

    pub fn hash_of(&self, name: &str) -> Option<&str> {
        self.pinned.get(name).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.pinned.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pinned.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, desc: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: desc.to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            risk_level: "low".to_string(),
            permissions: vec![],
            pinned_version: "1.0.0".to_string(),
        }
    }

    #[test]
    fn test_pin_and_verify_ok() {
        let mut reg = IntegrityRegistry::new();
        let h1 = reg.pin(def("read_file", "Reads a file"));
        let h2 = reg.pin(def("read_file", "Reads a file"));
        assert_eq!(h1, h2, "hash determinista ante misma definición");
        assert!(reg.verify("read_file").is_ok());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn test_hash_changes_with_description() {
        // Tool poisoning: cambiar la descripción cambia el hash.
        let h1 = def("run_command", "Runs commands").definition_hash();
        let h2 = def("run_command", "Runs commands (malicioso)").definition_hash();
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_unknown_tool_fails_closed() {
        let reg = IntegrityRegistry::new();
        assert!(reg.verify("nonexistent").is_err());
    }

    #[test]
    fn test_tampered_definition_detected() {
        let mut reg = IntegrityRegistry::new();
        reg.pin(def("write_file", "Writes files"));
        // Simular manipulación de la definición tras el pin.
        reg.definitions.insert("write_file".to_string(), def("write_file", "Writes files (envenenada)"));
        let err = reg.verify("write_file").unwrap_err();
        assert!(err.contains("mismatch"));
    }

    #[test]
    fn test_verify_all() {
        let mut reg = IntegrityRegistry::new();
        reg.pin(def("a", "A"));
        reg.pin(def("b", "B"));
        assert!(reg.verify_all().is_ok());
        reg.definitions.insert("b".to_string(), def("b", "B alterada"));
        assert!(reg.verify_all().is_err());
    }
}