//! integrity.rs - Tool Definition Integrity & Digest Pinning
//! 
//! Implements SHA-256 digest pinning for tool definitions to prevent
//! tool drift and unauthorized mutations (CVE-2025-54136 "MCPoison" mitigation).
//! 
//! This module validates the fingerprint of each tool definition BEFORE
//! it reaches the agent, ensuring fail-closed behavior per v5.0 contracts.

use crate::tools::ToolSchema;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration for integrity verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityConfig {
    /// Path to the pinned digests file (JSON format)
    pub pins_file: PathBuf,
    /// Whether to enforce pin verification (fail-closed)
    pub enforce: bool,
    /// Allow updating pins (for development)
    pub allow_pin_updates: bool,
    /// Hash algorithm to use (currently only sha256)
    pub algorithm: String,
}

impl Default for IntegrityConfig {
    fn default() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            pins_file: cwd.join(".lai").join("tool_pins.json"),
            enforce: true,
            allow_pin_updates: false,
            algorithm: "sha256".to_string(),
        }
    }
}

/// A pinned tool definition with its digest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedTool {
    pub name: String,
    pub digest: String,
    pub schema: ToolSchema,
    pub pinned_at: String,
    pub pinned_by: String,
}

/// Collection of pinned tool definitions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PinnedTools {
    pub tools: HashMap<String, PinnedTool>,
    pub version: u32,
    pub updated_at: String,
}

/// Integrity verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub tool_name: String,
    pub passed: bool,
    pub expected_digest: Option<String>,
    pub actual_digest: String,
    pub error: Option<String>,
}

/// Main integrity verifier
pub struct IntegrityVerifier {
    config: IntegrityConfig,
    pins: PinnedTools,
}

impl IntegrityVerifier {
    /// Create a new integrity verifier with default config
    pub fn new() -> Result<Self> {
        Self::with_config(IntegrityConfig::default())
    }

    /// Create a new integrity verifier with custom config
    pub fn with_config(config: IntegrityConfig) -> Result<Self> {
        let pins = if config.pins_file.exists() {
            let content = fs::read_to_string(&config.pins_file)
                .with_context(|| format!("Failed to read pins file: {:?}", config.pins_file))?;
            serde_json::from_str(&content)
                .with_context(|| "Failed to parse pins file")?
        } else {
            PinnedTools::default()
        };

        Ok(Self { config, pins })
    }

    /// Compute SHA-256 digest of a tool schema
    pub fn compute_digest(schema: &ToolSchema) -> String {
        let canonical = Self::canonicalize_schema(schema);
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        format!("sha256:{}", hex::encode(hasher.finalize()))
    }

    /// Create a canonical representation of a tool schema for hashing
    fn canonicalize_schema(schema: &ToolSchema) -> String {
        // Sort keys for deterministic hashing
        let mut map = serde_json::Map::new();
        map.insert("name".to_string(), serde_json::Value::String(schema.name.clone()));
        map.insert("description".to_string(), serde_json::Value::String(schema.description.clone()));
        map.insert("parameters".to_string(), schema.parameters.clone());
        
        // Sort parameters recursively
        let sorted_params = Self::sort_json_value(schema.parameters.clone());
        map.insert("parameters".to_string(), sorted_params);
        
        serde_json::to_string(&map).unwrap_or_default()
    }

    /// Recursively sort JSON object keys for deterministic output
    fn sort_json_value(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(mut map) => {
                let mut sorted: Vec<_> = map.drain().collect();
                sorted.sort_by(|a, b| a.0.cmp(&b.0));
                let mut result = serde_json::Map::new();
                for (k, v) in sorted {
                    result.insert(k, Self::sort_json_value(v));
                }
                serde_json::Value::Object(result)
            }
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(arr.into_iter().map(Self::sort_json_value).collect())
            }
            other => other,
        }
    }

    /// Verify a tool definition against pinned digest
    pub fn verify(&self, schema: &ToolSchema) -> VerificationResult {
        let actual_digest = Self::compute_digest(schema);
        
        if let Some(pinned) = self.pins.tools.get(&schema.name) {
            let passed = pinned.digest == actual_digest;
            VerificationResult {
                tool_name: schema.name.clone(),
                passed,
                expected_digest: Some(pinned.digest.clone()),
                actual_digest,
                error: if passed { None } else { Some("Digest mismatch - tool definition has been modified".to_string()) },
            }
        } else if self.config.enforce {
            VerificationResult {
                tool_name: schema.name.clone(),
                passed: false,
                expected_digest: None,
                actual_digest,
                error: Some("Tool not pinned - unknown tool definition".to_string()),
            }
        } else {
            VerificationResult {
                tool_name: schema.name.clone(),
                passed: true,
                expected_digest: None,
                actual_digest,
                error: None,
            }
        }
    }

    /// Verify multiple tool definitions
    pub fn verify_all(&self, schemas: &[ToolSchema]) -> Vec<VerificationResult> {
        schemas.iter().map(|s| self.verify(s)).collect()
    }

    /// Pin a tool definition (add or update)
    pub fn pin_tool(&mut self, schema: ToolSchema) -> Result<String> {
        if !self.config.allow_pin_updates {
            return Err(anyhow::anyhow!("Pin updates not allowed. Set allow_pin_updates=true to modify pins."));
        }

        let digest = Self::compute_digest(&schema);
        let now = chrono::Utc::now().to_rfc3339();
        let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());

        let pinned = PinnedTool {
            name: schema.name.clone(),
            digest: digest.clone(),
            schema,
            pinned_at: now.clone(),
            pinned_by: user,
        };

        self.pins.tools.insert(pinned.name.clone(), pinned);
        self.pins.version += 1;
        self.pins.updated_at = now;

        self.save()?;
        Ok(digest)
    }

    /// Pin multiple tool definitions at once
    pub fn pin_tools(&mut self, schemas: Vec<ToolSchema>) -> Result<Vec<String>> {
        let mut digests = Vec::new();
        for schema in schemas {
            digests.push(self.pin_tool(schema)?);
        }
        Ok(digests)
    }

    /// Remove a pinned tool
    pub fn unpin_tool(&mut self, name: &str) -> Result<bool> {
        if !self.config.allow_pin_updates {
            return Err(anyhow::anyhow!("Pin updates not allowed"));
        }
        
        let removed = self.pins.tools.remove(name).is_some();
        if removed {
            self.pins.version += 1;
            self.pins.updated_at = chrono::Utc::now().to_rfc3339();
            self.save()?;
        }
        Ok(removed)
    }

    /// Save pins to disk
    fn save(&self) -> Result<()> {
        if let Some(parent) = self.config.pins_file.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create pins directory: {:?}", parent))?;
        }
        
        let content = serde_json::to_string_pretty(&self.pins)
            .with_context(|| "Failed to serialize pins")?;
        
        fs::write(&self.config.pins_file, content)
            .with_context(|| format!("Failed to write pins file: {:?}", self.config.pins_file))?;
        
        Ok(())
    }

    /// Get all pinned tools
    pub fn get_pins(&self) -> &PinnedTools {
        &self.pins
    }

    /// Check if a tool is pinned
    pub fn is_pinned(&self, name: &str) -> bool {
        self.pins.tools.contains_key(name)
    }

    /// Get the digest for a pinned tool
    pub fn get_digest(&self, name: &str) -> Option<String> {
        self.pins.tools.get(name).map(|p| p.digest.clone())
    }

    /// Generate a report of all verifications
    pub fn generate_report(&self, schemas: &[ToolSchema]) -> IntegrityReport {
        let results = self.verify_all(schemas);
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = total - passed;
        
        IntegrityReport {
            total_tools: total,
            passed,
            failed,
            results,
            timestamp: chrono::Utc::now().to_rfc3339(),
            pins_version: self.pins.version,
        }
    }
}

/// Integrity verification report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub total_tools: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<VerificationResult>,
    pub timestamp: String,
    pub pins_version: u32,
}

impl IntegrityReport {
    /// Check if all verifications passed
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// Print a human-readable summary
    pub fn print_summary(&self) {
        println!("=== Integrity Verification Report ===");
        println!("Timestamp: {}", self.timestamp);
        println!("Pins version: {}", self.pins_version);
        println!("Total tools: {}", self.total_tools);
        println!("Passed: {}", self.passed);
        println!("Failed: {}", self.failed);
        println!();
        
        for result in &self.results {
            let status = if result.passed { "✓ PASS" } else { "✗ FAIL" };
            println!("{} {}", status, result.tool_name);
            if !result.passed {
                if let Some(expected) = &result.expected_digest {
                    println!("  Expected: {}", expected);
                }
                println!("  Actual:   {}", result.actual_digest);
                if let Some(err) = &result.error {
                    println!("  Error:    {}", err);
                }
            }
        }
    }
}

/// Initialize integrity pins from current tool registry (bootstrap)
pub fn bootstrap_pins(tools: &crate::tools::ToolRegistry, config: Option<IntegrityConfig>) -> Result<IntegrityVerifier> {
    let config = config.unwrap_or_default();
    let mut verifier = IntegrityVerifier::with_config(config)?;
    
    let schemas = tools.get_tools_schema();
    verifier.pin_tools(schemas)?;
    
    Ok(verifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_digest_deterministic() {
        let schema = ToolSchema {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "arg1": {"type": "string"}
                },
                "required": ["arg1"]
            }),
        };

        let digest1 = IntegrityVerifier::compute_digest(&schema);
        let digest2 = IntegrityVerifier::compute_digest(&schema);
        assert_eq!(digest1, digest2);
        assert!(digest1.starts_with("sha256:"));
    }

    #[test]
    fn test_digest_changes_with_schema() {
        let schema1 = ToolSchema {
            name: "test".to_string(),
            description: "Desc 1".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        };
        let schema2 = ToolSchema {
            name: "test".to_string(),
            description: "Desc 2".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        };

        let digest1 = IntegrityVerifier::compute_digest(&schema1);
        let digest2 = IntegrityVerifier::compute_digest(&schema2);
        assert_ne!(digest1, digest2);
    }

    #[test]
    fn test_canonicalize_sorts_keys() {
        let schema = ToolSchema {
            name: "test".to_string(),
            description: "Test".to_string(),
            parameters: serde_json::json!({
                "zebra": {"type": "string"},
                "apple": {"type": "integer"}
            }),
        };

        let canonical = IntegrityVerifier::canonicalize_schema(&schema);
        assert!(canonical.contains("apple"));
        assert!(canonical.contains("zebra"));
        // Apple should come before zebra in canonical form
        let apple_pos = canonical.find("apple").unwrap();
        let zebra_pos = canonical.find("zebra").unwrap();
        assert!(apple_pos < zebra_pos);
    }
}