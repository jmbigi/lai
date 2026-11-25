/// Dafny Verification Adapter for lai
///
/// Provides integration with the Dafny verifier for formal verification
/// of Rust and Python code through Dafny specifications.
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tokio::process::Command as TokioCommand;
use uuid::Uuid;

use crate::verification::contracts::{
    ContractVerificationResult, ProofObligation, ProofResult, VerificationAdapter,
    VerificationContract, VerificationError, VerificationFeatures, VerificationLanguage,
    VerificationStatus,
};

/// Dafny verification adapter
pub struct DafnyVerifier {
    dafny_path: PathBuf,
    version: String,
    timeout: Duration,
    working_dir: PathBuf,
}

impl DafnyVerifier {
    /// Create a new Dafny verifier
    pub fn new<P: AsRef<Path>>(dafny_path: P, working_dir: P) -> Result<Self, VerificationError> {
        let path = dafny_path.as_ref().to_path_buf();
        let work_dir = working_dir.as_ref().to_path_buf();

        // Verify Dafny is available and get version
        let version = Self::get_version(&path)?;

        Ok(Self {
            dafny_path: path,
            version,
            timeout: Duration::from_secs(120),
            working_dir: work_dir,
        })
    }

    /// Create with custom timeout
    pub fn with_timeout<P: AsRef<Path>>(
        dafny_path: P,
        working_dir: P,
        timeout: Duration,
    ) -> Result<Self, VerificationError> {
        let mut verifier = Self::new(dafny_path, working_dir)?;
        verifier.timeout = timeout;
        Ok(verifier)
    }

    /// Get Dafny version
    fn get_version(dafny_path: &Path) -> Result<String, VerificationError> {
        let output = Command::new(dafny_path)
            .arg("--version")
            .output()
            .map_err(|e| VerificationError::NotAvailable(format!("Failed to run Dafny: {}", e)))?;

        if !output.status.success() {
            return Err(VerificationError::NotAvailable(
                "Dafny version check failed".to_string(),
            ));
        }

        let version_str = String::from_utf8_lossy(&output.stdout);
        Ok(version_str.trim().to_string())
    }

    /// Generate a Dafny file from a proof obligation
    fn generate_dafny_file(
        &self,
        obligation: &ProofObligation,
        contract: &VerificationContract,
    ) -> Result<String, VerificationError> {
        let mut dafny_code = String::new();

        // Module header
        dafny_code.push_str(&format!(
            "module {} {{\n\n",
            contract.component.replace("-", "_")
        ));

        // Add imports if needed
        dafny_code.push_str("  // Import standard library\n");

        // Add ghost variables and assumptions
        for assumption in &obligation.formula.assumptions {
            dafny_code.push_str(&format!("  ghost var {}: bool;\n", assumption));
            dafny_code.push_str(&format!("  assume {};\n", assumption));
        }

        // Add the main verification target
        match obligation.formula.language {
            VerificationLanguage::Dafny => {
                // The expression is already in Dafny syntax
                dafny_code.push_str(&format!("  // Verification: {}\n", obligation.name));
                dafny_code.push_str(&format!(
                    "  method Verify{}() {{\n",
                    obligation.name.replace(" ", "_")
                ));

                // Add preconditions
                for pre in &contract.preconditions {
                    if pre.language == VerificationLanguage::Dafny {
                        dafny_code.push_str(&format!("    requires {};\n", pre.expression));
                    }
                }

                // Add the main expression as assertion or ensures
                if obligation.formula.expression.starts_with("ensures") {
                    dafny_code.push_str(&format!("    {};\n", obligation.formula.expression));
                } else {
                    dafny_code
                        .push_str(&format!("    assert {};\n", obligation.formula.expression));
                }

                // Add postconditions
                for post in &contract.postconditions {
                    if post.language == VerificationLanguage::Dafny {
                        dafny_code.push_str(&format!("    ensures {};\n", post.expression));
                    }
                }

                dafny_code.push_str("  }\n\n");
            }
            _ => {
                return Err(VerificationError::SyntaxError(format!(
                    "Unsupported formula language for Dafny: {:?}",
                    obligation.formula.language
                )));
            }
        }

        // Add invariants as ghost methods
        for invariant in &contract.invariants {
            if invariant.language == VerificationLanguage::Dafny {
                dafny_code.push_str(&format!("  // Invariant: {}\n", invariant.expression));
                dafny_code.push_str(&format!(
                    "  ghost method CheckInvariant{}() {{\n",
                    Uuid::new_v4().to_string().replace("-", "_")
                ));
                dafny_code.push_str(&format!("    assert {};\n", invariant.expression));
                dafny_code.push_str("  }\n\n");
            }
        }

        dafny_code.push_str("}\n");

        Ok(dafny_code)
    }

    /// Parse Dafny verification output
    fn parse_dafny_output(&self, output: &str, obligation: &ProofObligation) -> ProofResult {
        let mut status = VerificationStatus::Unknown;
        let mut proof = None;
        let mut counterexample = None;

        // Check for verification success
        if output.contains("verification succeeded") || output.contains("verified") {
            status = VerificationStatus::Verified;
            // Extract proof if available
            if let Some(start) = output.find("Proof:") {
                proof = Some(output[start..].to_string());
            }
        }
        // Check for counterexample
        else if output.contains("counterexample") || output.contains("assertion violation") {
            status = VerificationStatus::Falsified;
            counterexample = Some(output.to_string());
        }
        // Check for timeout
        else if output.contains("timeout") || output.contains("time limit") {
            status = VerificationStatus::Timeout;
        }
        // Check for errors
        else if output.contains("error") || output.contains("Error:") {
            status = VerificationStatus::Error;
        }

        ProofResult {
            obligation_id: obligation.id,
            status,
            proof,
            counterexample,
            execution_time_ms: 0, // Will be set by caller
            verifier_version: self.version.clone(),
            timestamp: std::time::SystemTime::now(),
            metadata: HashMap::new(),
        }
    }
}

#[async_trait]
impl VerificationAdapter for DafnyVerifier {
    fn language(&self) -> VerificationLanguage {
        VerificationLanguage::Dafny
    }

    fn version(&self) -> String {
        self.version.clone()
    }

    fn is_available(&self) -> bool {
        Command::new(&self.dafny_path)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn verify_obligation(
        &self,
        obligation: &ProofObligation,
    ) -> Result<ProofResult, VerificationError> {
        // Create a minimal contract for single obligation
        let contract = VerificationContract::new(
            "single_obligation".to_string(),
            "temp".to_string(),
            "1.0".to_string(),
            "Single obligation verification".to_string(),
        );

        self.verify_contract_with_obligation(&contract, obligation)
            .await
    }

    async fn verify_contract(
        &self,
        contract: &VerificationContract,
    ) -> Result<ContractVerificationResult, VerificationError> {
        let mut result = ContractVerificationResult::new(contract);

        for obligation in &contract.obligations {
            let obligation_result = self
                .verify_contract_with_obligation(contract, obligation)
                .await?;
            result.add_result(
                obligation_result,
                VerificationLanguage::Dafny,
                self.version.clone(),
            );
        }

        Ok(result)
    }

    fn supported_features(&self) -> VerificationFeatures {
        VerificationFeatures {
            supports_invariants: true,
            supports_pre_post: true,
            supports_ghost_code: true,
            supports_counterexamples: true,
            supports_proof_scripts: false,
            max_obligations_per_run: Some(100),
            timeout_seconds: Some(self.timeout.as_secs()),
        }
    }
}

impl DafnyVerifier {
    /// Verify a single obligation within a contract context
    async fn verify_contract_with_obligation(
        &self,
        contract: &VerificationContract,
        obligation: &ProofObligation,
    ) -> Result<ProofResult, VerificationError> {
        let start = Instant::now();

        // Generate Dafny file
        let dafny_code = self.generate_dafny_file(obligation, contract)?;

        // Write to temporary file
        let temp_file = self
            .working_dir
            .join(format!("verify_{}.dfy", Uuid::new_v4()));
        tokio::fs::write(&temp_file, &dafny_code)
            .await
            .map_err(VerificationError::IoError)?;

        // Run Dafny verification
        let output = TokioCommand::new(&self.dafny_path)
            .arg("verify")
            .arg(&temp_file)
            .arg("--verification-error-limit=10")
            .current_dir(&self.working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(VerificationError::IoError)?;

        let execution_time = start.elapsed().as_millis() as u64;

        // Clean up temp file
        let _ = tokio::fs::remove_file(&temp_file).await;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        let mut result = self.parse_dafny_output(&combined, obligation);
        result.execution_time_ms = execution_time;

        if !output.status.success() && result.status == VerificationStatus::Unknown {
            result.status = VerificationStatus::Error;
            result.counterexample = Some(combined);
        }

        Ok(result)
    }
}

/// Builder for Dafny specifications from Rust/Python code
pub struct DafnySpecBuilder {
    module_name: String,
    functions: Vec<DafnyFunctionSpec>,
    types: Vec<DafnyTypeSpec>,
}

#[derive(Debug, Clone)]
pub struct DafnyFunctionSpec {
    pub name: String,
    pub params: Vec<(String, String)>, // (name, type)
    pub return_type: String,
    pub requires: Vec<String>,
    pub ensures: Vec<String>,
    pub body: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DafnyTypeSpec {
    pub name: String,
    pub definition: String,
}

impl DafnySpecBuilder {
    pub fn new(module_name: String) -> Self {
        Self {
            module_name,
            functions: Vec::new(),
            types: Vec::new(),
        }
    }

    pub fn add_function(&mut self, spec: DafnyFunctionSpec) {
        self.functions.push(spec);
    }

    pub fn add_type(&mut self, spec: DafnyTypeSpec) {
        self.types.push(spec);
    }

    pub fn build(&self) -> String {
        let mut code = String::new();
        code.push_str(&format!("module {} {{\n\n", self.module_name));

        // Types
        for ty in &self.types {
            code.push_str(&format!("  type {} = {}\n\n", ty.name, ty.definition));
        }

        // Functions
        for func in &self.functions {
            code.push_str(&format!(
                "  method {}({}",
                func.name,
                func.params
                    .iter()
                    .map(|(n, t)| format!("{}: {}", n, t))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            code.push_str(&format!(") returns (result: {})\n", func.return_type));

            for req in &func.requires {
                code.push_str(&format!("    requires {};\n", req));
            }

            for ens in &func.ensures {
                code.push_str(&format!("    ensures {};\n", ens));
            }

            code.push_str("  {\n");
            if let Some(body) = &func.body {
                code.push_str(&format!("    {}\n", body));
            } else {
                code.push_str("    // Implementation omitted for verification\n");
            }
            code.push_str("  }\n\n");
        }

        code.push_str("}\n");
        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verification::contracts::LogicFormula;

    #[test]
    fn test_dafny_spec_builder() {
        let mut builder = DafnySpecBuilder::new("TestModule".to_string());

        builder.add_function(DafnyFunctionSpec {
            name: "add".to_string(),
            params: vec![
                ("a".to_string(), "int".to_string()),
                ("b".to_string(), "int".to_string()),
            ],
            return_type: "int".to_string(),
            requires: vec!["a >= 0".to_string(), "b >= 0".to_string()],
            ensures: vec!["result == a + b".to_string(), "result >= 0".to_string()],
            body: Some("return a + b;".to_string()),
        });

        let code = builder.build();
        assert!(code.contains("module TestModule"));
        assert!(code.contains("method add"));
        assert!(code.contains("requires a >= 0"));
        assert!(code.contains("ensures result == a + b"));
    }

    #[test]
    fn test_logic_formula_dafny() {
        let formula = LogicFormula::new(VerificationLanguage::Dafny, "result >= 0".to_string());
        assert_eq!(formula.language, VerificationLanguage::Dafny);
    }
}
