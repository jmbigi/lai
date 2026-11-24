/// Verification contracts and proof obligations for formal verification
/// 
/// This module defines the core types for formal verification in lai,
/// including verification contracts, proof obligations, and verification results.

use std::collections::HashMap;
use std::time::SystemTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Supported verification languages/tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerificationLanguage {
    Dafny,
    Tla,
    Z3,
    Cvc5,
}

impl VerificationLanguage {
    pub fn as_str(&self) -> &'static str {
        match self {
            VerificationLanguage::Dafny => "dafny",
            VerificationLanguage::Tla => "tla",
            VerificationLanguage::Z3 => "z3",
            VerificationLanguage::Cvc5 => "cvc5",
        }
    }
}

/// Verification status/result
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    /// Property verified (formally proven)
    Verified,
    /// Property failed (counterexample found)
    Falsified,
    /// Verification timed out
    Timeout,
    /// Verification error (tool error, syntax error, etc.)
    Error,
    /// Unknown/undetermined
    Unknown,
    /// Partially verified (some properties proven, others not)
    Partial,
}

/// A logical formula/expression in a verification language
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicFormula {
    pub language: VerificationLanguage,
    pub expression: String,
    pub variables: HashMap<String, String>, // name -> type
    pub assumptions: Vec<String>,
}

impl LogicFormula {
    pub fn new(language: VerificationLanguage, expression: String) -> Self {
        Self {
            language,
            expression,
            variables: HashMap::new(),
            assumptions: Vec::new(),
        }
    }

    pub fn with_variables(mut self, variables: HashMap<String, String>) -> Self {
        self.variables = variables;
        self
    }

    pub fn with_assumptions(mut self, assumptions: Vec<String>) -> Self {
        self.assumptions = assumptions;
        self
    }
}

/// A single proof obligation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofObligation {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub formula: LogicFormula,
    pub location: Option<SourceLocation>,
    pub priority: ProofPriority,
    pub metadata: HashMap<String, String>,
}

impl ProofObligation {
    pub fn new(name: String, description: String, formula: LogicFormula) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            description,
            formula,
            location: None,
            priority: ProofPriority::Normal,
            metadata: HashMap::new(),
        }
    }

    pub fn with_location(mut self, location: SourceLocation) -> Self {
        self.location = Some(location);
        self
    }

    pub fn with_priority(mut self, priority: ProofPriority) -> Self {
        self.priority = priority;
        self
    }
}

/// Source code location for traceability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub function: Option<String>,
}

/// Priority of a proof obligation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProofPriority {
    Critical,
    High,
    Normal,
    Low,
}

/// Result of verifying a single proof obligation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofResult {
    pub obligation_id: Uuid,
    pub status: VerificationStatus,
    pub proof: Option<String>, // Proof script or witness
    pub counterexample: Option<String>,
    pub execution_time_ms: u64,
    pub verifier_version: String,
    pub timestamp: SystemTime,
    pub metadata: HashMap<String, String>,
}

/// A complete verification contract for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationContract {
    pub id: Uuid,
    pub name: String,
    pub component: String, // Component/module being verified
    pub version: String,
    pub specification: String, // Natural language specification
    pub obligations: Vec<ProofObligation>,
    pub invariants: Vec<LogicFormula>,
    pub preconditions: Vec<LogicFormula>,
    pub postconditions: Vec<LogicFormula>,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub metadata: HashMap<String, String>,
}

impl VerificationContract {
    pub fn new(name: String, component: String, version: String, specification: String) -> Self {
        let now = SystemTime::now();
        Self {
            id: Uuid::new_v4(),
            name,
            component,
            version,
            specification,
            obligations: Vec::new(),
            invariants: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
        }
    }

    pub fn add_obligation(&mut self, obligation: ProofObligation) {
        self.obligations.push(obligation);
        self.updated_at = SystemTime::now();
    }

    pub fn add_invariant(&mut self, invariant: LogicFormula) {
        self.invariants.push(invariant);
        self.updated_at = SystemTime::now();
    }

    pub fn add_precondition(&mut self, precondition: LogicFormula) {
        self.preconditions.push(precondition);
        self.updated_at = SystemTime::now();
    }

    pub fn add_postcondition(&mut self, postcondition: LogicFormula) {
        self.postconditions.push(postcondition);
        self.updated_at = SystemTime::now();
    }
}

/// Verification result for a complete contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractVerificationResult {
    pub contract_id: Uuid,
    pub contract_name: String,
    pub overall_status: VerificationStatus,
    pub obligation_results: Vec<ProofResult>,
    pub total_obligations: usize,
    pub verified_count: usize,
    pub falsified_count: usize,
    pub timeout_count: usize,
    pub error_count: usize,
    pub total_time_ms: u64,
    pub verified_at: SystemTime,
    pub verifier_versions: HashMap<VerificationLanguage, String>,
}

impl ContractVerificationResult {
    pub fn new(contract: &VerificationContract) -> Self {
        Self {
            contract_id: contract.id,
            contract_name: contract.name.clone(),
            overall_status: VerificationStatus::Unknown,
            obligation_results: Vec::new(),
            total_obligations: contract.obligations.len(),
            verified_count: 0,
            falsified_count: 0,
            timeout_count: 0,
            error_count: 0,
            total_time_ms: 0,
            verified_at: SystemTime::now(),
            verifier_versions: HashMap::new(),
        }
    }

    pub fn add_result(&mut self, result: ProofResult, language: VerificationLanguage, version: String) {
        match result.status {
            VerificationStatus::Verified => self.verified_count += 1,
            VerificationStatus::Falsified => self.falsified_count += 1,
            VerificationStatus::Timeout => self.timeout_count += 1,
            VerificationStatus::Error => self.error_count += 1,
            _ => {}
        }
        self.total_time_ms += result.execution_time_ms;
        self.obligation_results.push(result);
        self.verifier_versions.insert(language, version);
        self.verified_at = SystemTime::now();
        self.update_overall_status();
    }

    fn update_overall_status(&mut self) {
        if self.falsified_count > 0 {
            self.overall_status = VerificationStatus::Falsified;
        } else if self.error_count > 0 && self.verified_count == 0 {
            self.overall_status = VerificationStatus::Error;
        } else if self.timeout_count > 0 && self.verified_count + self.falsified_count == 0 {
            self.overall_status = VerificationStatus::Timeout;
        } else if self.verified_count == self.total_obligations && self.total_obligations > 0 {
            self.overall_status = VerificationStatus::Verified;
        } else if self.verified_count > 0 {
            self.overall_status = VerificationStatus::Partial;
        } else {
            self.overall_status = VerificationStatus::Unknown;
        }
    }
}

/// Capability adapter trait for verification tools
#[async_trait::async_trait]
pub trait VerificationAdapter: Send + Sync {
    fn language(&self) -> VerificationLanguage;
    fn version(&self) -> String;
    fn is_available(&self) -> bool;

    async fn verify_obligation(&self, obligation: &ProofObligation) -> Result<ProofResult, VerificationError>;
    async fn verify_contract(&self, contract: &VerificationContract) -> Result<ContractVerificationResult, VerificationError>;

    fn supported_features(&self) -> VerificationFeatures;
}

/// Features supported by a verification adapter
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerificationFeatures {
    pub supports_invariants: bool,
    pub supports_pre_post: bool,
    pub supports_ghost_code: bool,
    pub supports_counterexamples: bool,
    pub supports_proof_scripts: bool,
    pub max_obligations_per_run: Option<usize>,
    pub timeout_seconds: Option<u64>,
}

/// Verification errors
#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("Verifier not available: {0}")]
    NotAvailable(String),
    #[error("Verification timeout")]
    Timeout,
    #[error("Syntax error: {0}")]
    SyntaxError(String),
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logic_formula_creation() {
        let formula = LogicFormula::new(
            VerificationLanguage::Dafny,
            "ensures result >= 0".to_string(),
        );
        assert_eq!(formula.language, VerificationLanguage::Dafny);
    }

    #[test]
    fn test_proof_obligation_creation() {
        let formula = LogicFormula::new(
            VerificationLanguage::Dafny,
            "ensures result >= 0".to_string(),
        );
        let obligation = ProofObligation::new(
            "non_negative".to_string(),
            "Result should be non-negative".to_string(),
            formula,
        );
        assert_eq!(obligation.name, "non_negative");
    }

    #[test]
    fn test_verification_contract() {
        let mut contract = VerificationContract::new(
            "test_contract".to_string(),
            "math_utils".to_string(),
            "1.0.0".to_string(),
            "Math utilities verification".to_string(),
        );
        
        let formula = LogicFormula::new(
            VerificationLanguage::Dafny,
            "ensures result >= 0".to_string(),
        );
        let obligation = ProofObligation::new(
            "non_negative".to_string(),
            "Result should be non-negative".to_string(),
            formula,
        );
        contract.add_obligation(obligation);
        
        assert_eq!(contract.obligations.len(), 1);
    }

    #[test]
    fn test_contract_verification_result() {
        let mut contract = VerificationContract::new(
            "test".to_string(),
            "comp".to_string(),
            "1.0".to_string(),
            "spec".to_string(),
        );

        let formula = LogicFormula::new(VerificationLanguage::Dafny, "true".to_string());
        let obligation = ProofObligation::new("test".to_string(), "desc".to_string(), formula);
        contract.add_obligation(obligation.clone());

        let mut result = ContractVerificationResult::new(&contract);
        
        let proof_result = ProofResult {
            obligation_id: obligation.id,
            status: VerificationStatus::Verified,
            proof: Some("proof".to_string()),
            counterexample: None,
            execution_time_ms: 100,
            verifier_version: "Dafny 4.0".to_string(),
            timestamp: SystemTime::now(),
            metadata: HashMap::new(),
        };
        
        result.add_result(proof_result, VerificationLanguage::Dafny, "4.0".to_string());
        
        assert_eq!(result.verified_count, 1);
        assert_eq!(result.overall_status, VerificationStatus::Verified);
    }
}