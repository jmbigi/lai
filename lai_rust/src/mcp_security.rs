//! MCP Hangar / digest pinning (v5.0, herramienta 7).
//!
//! Operacionaliza el `definition_hash` que la v4.0 exige para las herramientas
//! internas, extendiéndolo a los servidores MCP externos: la definición de
//! cada herramienta anunciada por un servidor debe coincidir con el
//! fingerprint SHA-256 fijado (pinned) antes de que llegue al agente.
//! Previene *tool drift* y mutaciones no autorizadas (cf. CVE-2025-54136,
//! "MCPoison": un servidor que cambia su definición tras el pin).
//!
//! Contrato v5.0: **fail-closed** — si un pin no puede verificarse (pin
//! ausente, hash distinto, servidor caído), la herramienta NO se expone.

use crate::integrity::sha256_hex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Definición de herramienta anunciada por un servidor MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    pub server: String,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl McpToolDef {
    /// Hash canónico de la definición completa (servidor + nombre +
    /// descripción + esquema de entrada).
    pub fn definition_hash(&self) -> String {
        let canonical = serde_json::json!({
            "server": self.server,
            "name": self.name,
            "description": self.description,
            "input_schema": self.input_schema,
        });
        sha256_hex(&serde_json::to_string(&canonical).unwrap_or_default())
    }
}

/// Pin fijado para una herramienta MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPin {
    pub server: String,
    pub name: String,
    pub definition_hash: String,
    pub pinned_at: String,
}

/// Registro de pins MCP con verificación fail-closed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpPinRegistry {
    pub pins: HashMap<String, McpPin>,
}

impl McpPinRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(server: &str, name: &str) -> String {
        format!("{}::{}", server, name)
    }

    /// Fija el pin de una definición (operador, tras revisión).
    pub fn pin(&mut self, def: &McpToolDef) -> McpPin {
        let pin = McpPin {
            server: def.server.clone(),
            name: def.name.clone(),
            definition_hash: def.definition_hash(),
            pinned_at: chrono::Utc::now().to_rfc3339(),
        };
        self.pins.insert(Self::key(&def.server, &def.name), pin.clone());
        pin
    }

    pub fn is_pinned(&self, server: &str, name: &str) -> bool {
        self.pins.contains_key(&Self::key(server, name))
    }

    /// Verifica una definición anunciada contra el pin. Fail-closed:
    /// - Sin pin → error (herramienta no autorizada).
    /// - Hash distinto → error (tool drift / mutación no autorizada).
    pub fn verify(&self, def: &McpToolDef) -> Result<(), String> {
        let pin = self.pins.get(&Self::key(&def.server, &def.name)).ok_or_else(|| {
            format!(
                "MCP fail-closed: '{}/{}' no está pineada (pin requerido)",
                def.server, def.name
            )
        })?;
        let actual = def.definition_hash();
        if actual != pin.definition_hash {
            return Err(format!(
                "MCP fail-closed: definición de '{}/{}' NO coincide con el pin \
                 (esperado {}, actual {}) — posible tool drift",
                def.server, def.name, pin.definition_hash, actual
            ));
        }
        Ok(())
    }

    /// Filtra un lote de definiciones anunciadas: solo pasan las verificadas.
    /// Devuelve las exponibles y los rechazos con su motivo.
    pub fn filter_verified(
        &self,
        defs: &[McpToolDef],
    ) -> (Vec<McpToolDef>, Vec<(String, String)>) {
        let mut ok = Vec::new();
        let mut denied = Vec::new();
        for def in defs {
            match self.verify(def) {
                Ok(()) => ok.push(def.clone()),
                Err(reason) => denied.push((format!("{}/{}", def.server, def.name), reason)),
            }
        }
        (ok, denied)
    }

    /// Carga/genera el fichero de pins (`<workspace>/.lai/mcp-pins.json`).
    pub fn load_or_init(path: &Path) -> std::io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(content) => Ok(serde_json::from_str(&content).unwrap_or_default()),
            Err(_) => Ok(Self::new()),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(server: &str, name: &str) -> McpToolDef {
        McpToolDef {
            server: server.to_string(),
            name: name.to_string(),
            description: "lee un archivo".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        }
    }

    #[test]
    fn test_pin_and_verify_ok() {
        let mut reg = McpPinRegistry::new();
        let def = sample("fs", "read");
        reg.pin(&def);
        assert!(reg.verify(&def).is_ok());
    }

    #[test]
    fn test_unpinned_is_denied() {
        let reg = McpPinRegistry::new();
        let def = sample("fs", "read");
        assert!(reg.verify(&def).is_err());
    }

    #[test]
    fn test_drift_is_denied() {
        let mut reg = McpPinRegistry::new();
        let def = sample("fs", "read");
        reg.pin(&def);
        // El servidor cambia la descripción tras el pin (MCPoison).
        let mut drifted = def.clone();
        drifted.description = "ahora borra archivos".to_string();
        let err = reg.verify(&drifted).unwrap_err();
        assert!(err.contains("NO coincide"));
    }

    #[test]
    fn test_filter_verified_separates_denials() {
        let mut reg = McpPinRegistry::new();
        reg.pin(&sample("fs", "read"));
        let defs = vec![sample("fs", "read"), sample("fs", "write")];
        let (ok, denied) = reg.filter_verified(&defs);
        assert_eq!(ok.len(), 1);
        assert_eq!(denied.len(), 1);
        assert!(denied[0].0.contains("write"));
    }

    #[test]
    fn test_hash_changes_with_schema() {
        let mut a = sample("fs", "read");
        let b = a.clone();
        a.input_schema = serde_json::json!({"type": "object", "required": ["path"]});
        assert_ne!(a.definition_hash(), b.definition_hash());
    }
}