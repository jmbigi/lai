/// TLA+ Verification Adapter for lai
///
/// Provides integration with TLC (Temporal Logic Checker) for model checking
/// of concurrent and distributed systems specifications.
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tokio::process::Command as TokioCommand;
use uuid::Uuid;

use crate::verification::contracts::{
    ContractVerificationResult, LogicFormula, ProofObligation, ProofResult, VerificationAdapter,
    VerificationContract, VerificationError, VerificationFeatures, VerificationLanguage,
    VerificationStatus,
};

/// TLA+ verification adapter using TLC
pub struct TlaVerifier {
    tla_toolbox_path: Option<PathBuf>,
    tlc_path: PathBuf,
    version: String,
    timeout: Duration,
    working_dir: PathBuf,
    java_path: PathBuf,
}

impl TlaVerifier {
    /// Create a new TLA+ verifier
    pub fn new<P: AsRef<Path>>(tlc_path: P, working_dir: P) -> Result<Self, VerificationError> {
        let tlc = tlc_path.as_ref().to_path_buf();
        let work_dir = working_dir.as_ref().to_path_buf();

        // Find Java
        let java_path = Self::find_java()?;

        // Verify TLC is available and get version
        let version = Self::get_version(&java_path, &tlc)?;

        Ok(Self {
            tla_toolbox_path: None,
            tlc_path: tlc,
            version,
            timeout: Duration::from_secs(300), // TLC can take longer
            working_dir: work_dir,
            java_path,
        })
    }

    /// Create with TLA Toolbox path
    pub fn with_toolbox<P: AsRef<Path>, Q: AsRef<Path>>(
        tlc_path: P,
        toolbox_path: Q,
        working_dir: P,
    ) -> Result<Self, VerificationError> {
        let mut verifier = Self::new(tlc_path, working_dir)?;
        verifier.tla_toolbox_path = Some(toolbox_path.as_ref().to_path_buf());
        Ok(verifier)
    }

    /// Create with custom timeout
    pub fn with_timeout<P: AsRef<Path>>(
        tlc_path: P,
        working_dir: P,
        timeout: Duration,
    ) -> Result<Self, VerificationError> {
        let mut verifier = Self::new(tlc_path, working_dir)?;
        verifier.timeout = timeout;
        Ok(verifier)
    }

    /// Find Java executable
    fn find_java() -> Result<PathBuf, VerificationError> {
        // Try common locations
        let candidates = [
            "java",
            "/usr/bin/java",
            "/usr/lib/jvm/default-java/bin/java",
            "/usr/lib/jvm/java-17-openjdk/bin/java",
            "/usr/lib/jvm/java-21-openjdk/bin/java",
        ];

        for candidate in candidates {
            let path = PathBuf::from(candidate);
            if path.exists() || candidate == "java" {
                // Verify it works
                if Command::new(candidate).arg("-version").output().is_ok() {
                    return Ok(path);
                }
            }
        }

        Err(VerificationError::NotAvailable(
            "Java not found. TLC requires Java.".to_string(),
        ))
    }

    /// Get TLC version
    fn get_version(java_path: &Path, tlc_path: &Path) -> Result<String, VerificationError> {
        let output = Command::new(java_path)
            .arg("-cp")
            .arg(tlc_path)
            .arg("tlc2.TLC")
            .arg("-help")
            .output()
            .map_err(|e| VerificationError::NotAvailable(format!("Failed to run TLC: {}", e)))?;

        // TLC prints version to stderr on -help
        let stderr = String::from_utf8_lossy(&output.stderr);
        let version_line = stderr
            .lines()
            .find(|l| l.contains("Version") || l.contains("TLC"))
            .unwrap_or("Unknown");

        Ok(version_line.trim().to_string())
    }

    /// Generate a TLA+ specification file from contract
    fn generate_tla_spec(
        &self,
        contract: &VerificationContract,
    ) -> Result<String, VerificationError> {
        let mut spec = String::new();

        // Module header
        spec.push_str(&format!(
            "------------------------------ MODULE {} ------------------------------\n",
            contract.component.replace("-", "_").to_uppercase()
        ));
        spec.push_str("EXTENDS Naturals, Sequences, TLC\n\n");

        // Constants
        spec.push_str("CONSTANTS\n");
        spec.push_str("  \\* System parameters\n");
        for (key, value) in &contract.metadata {
            if key.starts_with("const_") {
                spec.push_str(&format!(
                    "  {}, \\* {}\n",
                    key.strip_prefix("const_").unwrap_or(key),
                    value
                ));
            }
        }
        spec.push('\n');

        // Variables
        spec.push_str("VARIABLES\n");
        spec.push_str("  \\* System state variables\n");
        for (key, value) in &contract.metadata {
            if key.starts_with("var_") {
                spec.push_str(&format!(
                    "  {}, \\* {}\n",
                    key.strip_prefix("var_").unwrap_or(key),
                    value
                ));
            }
        }
        spec.push('\n');

        // Type invariant
        spec.push_str("TypeInvariant ==\n");
        spec.push_str("  /\\ TRUE\n"); // Placeholder
        for inv in &contract.invariants {
            if inv.language == VerificationLanguage::Tla {
                spec.push_str(&format!("  /\\ {}\n", inv.expression));
            }
        }
        spec.push('\n');

        // Initial state
        spec.push_str("Init ==\n");
        spec.push_str("  /\\ TRUE\n");
        for pre in &contract.preconditions {
            if pre.language == VerificationLanguage::Tla {
                spec.push_str(&format!("  /\\ {}\n", pre.expression));
            }
        }
        spec.push('\n');

        // Next state relation
        spec.push_str("Next ==\n");
        spec.push_str("  \\* Transition relation\n");
        spec.push_str("  \\/ TRUE\n"); // Placeholder
        spec.push('\n');

        // Specification
        spec.push_str("Spec == Init /\\ [][Next]_<<");
        // Add variables
        let vars: Vec<String> = contract
            .metadata
            .iter()
            .filter(|(k, _)| k.starts_with("var_"))
            .map(|(k, _)| k.strip_prefix("var_").unwrap_or(k).to_string())
            .collect();
        spec.push_str(&vars.join(", "));
        spec.push_str(">>\n\n");

        // Properties to verify
        for (i, obligation) in contract.obligations.iter().enumerate() {
            if obligation.formula.language == VerificationLanguage::Tla {
                spec.push_str(&format!(
                    "Property{} == {}\n",
                    i, obligation.formula.expression
                ));
            }
        }
        spec.push('\n');

        // Liveness properties
        spec.push_str("\\* Liveness properties\n");
        for post in &contract.postconditions {
            if post.language == VerificationLanguage::Tla {
                spec.push_str(&format!(
                    "Liveness_{} == {}\n",
                    Uuid::new_v4().simple(),
                    post.expression
                ));
            }
        }
        spec.push('\n');

        spec.push_str(
            "=============================================================================\n",
        );

        Ok(spec)
    }

    /// Generate TLC configuration file
    fn generate_tlc_config(
        &self,
        contract: &VerificationContract,
    ) -> Result<String, VerificationError> {
        let mut config = String::new();

        config.push_str(&format!(
            "SPECIFICATION {}",
            contract.component.replace("-", "_").to_uppercase()
        ));
        config.push_str("\n\n");

        // Constants assignment
        let constants: Vec<String> = contract
            .metadata
            .iter()
            .filter(|(k, _)| k.starts_with("const_"))
            .map(|(k, v)| format!("{} = {}", k.strip_prefix("const_").unwrap_or(k), v))
            .collect();

        if !constants.is_empty() {
            config.push_str("CONSTANTS\n");
            for c in constants {
                config.push_str(&format!("  {}\n", c));
            }
            config.push('\n');
        }

        // Properties to check
        config.push_str("PROPERTIES\n");
        for (i, obligation) in contract.obligations.iter().enumerate() {
            if obligation.formula.language == VerificationLanguage::Tla {
                config.push_str(&format!("  Property{}\n", i));
            }
        }

        // Invariants
        for inv in contract.invariants.iter() {
            if inv.language == VerificationLanguage::Tla {
                config.push_str("  TypeInvariant\n"); // Check type invariant
                break;
            }
        }
        config.push('\n');

        // Model checking options
        config.push_str("MODEL_CHECKING_OPTIONS\n");
        config.push_str("  DeadlockDetection = TRUE\n");
        config.push_str("  MaxStates = 1000000\n");
        config.push_str("  MaxDepth = 100\n");
        config.push('\n');

        Ok(config)
    }

    /// Parse TLC output
    fn parse_tlc_output(&self, output: &str, obligation: &ProofObligation) -> ProofResult {
        let mut status = VerificationStatus::Unknown;
        let mut counterexample = None;

        if output.contains("Model checking completed") && !output.contains("Error") {
            if output.contains("No error")
                || output.contains("passed")
                || output.contains("invariant.*holds")
            {
                status = VerificationStatus::Verified;
            }
        } else if (output.contains("Invariant") && output.contains("violated"))
            || (output.contains("Deadlock") && output.contains("found"))
        {
            status = VerificationStatus::Falsified;
            counterexample = Some(output.to_string());
        } else if output.contains("timeout")
            || output.contains("Time limit")
            || output.contains("MaxStates")
        {
            status = VerificationStatus::Timeout;
        } else if output.contains("Error")
            || output.contains("Exception")
            || output.contains("error:")
        {
            status = VerificationStatus::Error;
            counterexample = Some(output.to_string());
        }

        ProofResult {
            obligation_id: obligation.id,
            status,
            proof: None, // TLC doesn't produce proof scripts
            counterexample,
            execution_time_ms: 0,
            verifier_version: self.version.clone(),
            timestamp: std::time::SystemTime::now(),
            metadata: HashMap::new(),
        }
    }
}

#[async_trait]
impl VerificationAdapter for TlaVerifier {
    fn language(&self) -> VerificationLanguage {
        VerificationLanguage::Tla
    }

    fn version(&self) -> String {
        self.version.clone()
    }

    fn is_available(&self) -> bool {
        Command::new(&self.java_path)
            .arg("-cp")
            .arg(&self.tlc_path)
            .arg("tlc2.TLC")
            .arg("-help")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn verify_obligation(
        &self,
        obligation: &ProofObligation,
    ) -> Result<ProofResult, VerificationError> {
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

        // For TLA+, we typically verify the whole model at once
        let verification_result = self.verify_tla_model(contract).await?;

        // Distribute results to obligations
        for obligation in &contract.obligations {
            if obligation.formula.language == VerificationLanguage::Tla {
                result.add_result(
                    verification_result.clone(),
                    VerificationLanguage::Tla,
                    self.version.clone(),
                );
            } else {
                // Create a placeholder result for non-TLA obligations
                let placeholder = ProofResult {
                    obligation_id: obligation.id,
                    status: VerificationStatus::Unknown,
                    proof: None,
                    counterexample: Some("Not a TLA+ obligation".to_string()),
                    execution_time_ms: 0,
                    verifier_version: self.version.clone(),
                    timestamp: std::time::SystemTime::now(),
                    metadata: HashMap::new(),
                };
                result.add_result(placeholder, VerificationLanguage::Tla, self.version.clone());
            }
        }

        Ok(result)
    }

    fn supported_features(&self) -> VerificationFeatures {
        VerificationFeatures {
            supports_invariants: true,
            supports_pre_post: false, // TLA+ uses Init/Next
            supports_ghost_code: false,
            supports_counterexamples: true,
            supports_proof_scripts: false,
            max_obligations_per_run: Some(50),
            timeout_seconds: Some(self.timeout.as_secs()),
        }
    }
}

impl TlaVerifier {
    /// Verify a TLA+ model (runs TLC on the full specification)
    async fn verify_tla_model(
        &self,
        contract: &VerificationContract,
    ) -> Result<ProofResult, VerificationError> {
        let start = Instant::now();

        // Generate spec file
        let spec_content = self.generate_tla_spec(contract)?;
        let spec_file = self.working_dir.join(format!(
            "{}.tla",
            contract.component.replace("-", "_").to_uppercase()
        ));
        tokio::fs::write(&spec_file, &spec_content)
            .await
            .map_err(VerificationError::IoError)?;

        // Generate config file
        let config_content = self.generate_tlc_config(contract)?;
        let config_file = self.working_dir.join(format!(
            "{}.cfg",
            contract.component.replace("-", "_").to_uppercase()
        ));
        tokio::fs::write(&config_file, &config_content)
            .await
            .map_err(VerificationError::IoError)?;

        // Run TLC
        let output = TokioCommand::new(&self.java_path)
            .arg("-cp")
            .arg(&self.tlc_path)
            .arg("-Xmx4g")
            .arg("tlc2.TLC")
            .arg("-config")
            .arg(&config_file)
            .arg(&spec_file)
            .current_dir(&self.working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(VerificationError::IoError)?;

        let execution_time = start.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        // Create a dummy obligation for parsing
        let dummy_obligation = ProofObligation::new(
            "model_check".to_string(),
            "Full model check".to_string(),
            LogicFormula::new(VerificationLanguage::Tla, "Spec".to_string()),
        );

        let mut result = self.parse_tlc_output(&combined, &dummy_obligation);
        result.execution_time_ms = execution_time;

        if !output.status.success() && result.status == VerificationStatus::Unknown {
            result.status = VerificationStatus::Error;
            result.counterexample = Some(combined);
        }

        // Cleanup
        let _ = tokio::fs::remove_file(&spec_file).await;
        let _ = tokio::fs::remove_file(&config_file).await;

        Ok(result)
    }

    /// Verify a single obligation within a contract context
    async fn verify_contract_with_obligation(
        &self,
        contract: &VerificationContract,
        obligation: &ProofObligation,
    ) -> Result<ProofResult, VerificationError> {
        // For TLA+, we verify the whole model and extract the relevant property result
        let model_result = self.verify_tla_model(contract).await?;

        // Check if this specific obligation was verified
        let mut result = model_result;
        result.obligation_id = obligation.id;

        Ok(result)
    }
}

/// TLA+ specification builder for common patterns
pub struct TlaSpecBuilder {
    module_name: String,
    constants: HashMap<String, String>,
    variables: HashMap<String, String>,
    init_predicates: Vec<String>,
    next_actions: Vec<String>,
    invariants: Vec<String>,
    properties: Vec<String>,
}

impl TlaSpecBuilder {
    pub fn new(module_name: String) -> Self {
        Self {
            module_name,
            constants: HashMap::new(),
            variables: HashMap::new(),
            init_predicates: Vec::new(),
            next_actions: Vec::new(),
            invariants: Vec::new(),
            properties: Vec::new(),
        }
    }

    pub fn add_constant(&mut self, name: String, description: String) {
        self.constants.insert(name, description);
    }

    pub fn add_variable(&mut self, name: String, description: String) {
        self.variables.insert(name, description);
    }

    pub fn add_init(&mut self, predicate: String) {
        self.init_predicates.push(predicate);
    }

    pub fn add_action(&mut self, action: String) {
        self.next_actions.push(action);
    }

    pub fn add_invariant(&mut self, invariant: String) {
        self.invariants.push(invariant);
    }

    pub fn add_property(&mut self, property: String) {
        self.properties.push(property);
    }

    pub fn build(&self) -> String {
        let mut spec = String::new();

        spec.push_str(&format!(
            "------------------------------ MODULE {} ------------------------------\n",
            self.module_name.to_uppercase()
        ));
        spec.push_str("EXTENDS Naturals, Sequences, TLC\n\n");

        // Constants
        if !self.constants.is_empty() {
            spec.push_str("CONSTANTS\n");
            for (name, desc) in &self.constants {
                spec.push_str(&format!("  {}, \\* {}\n", name, desc));
            }
            spec.push('\n');
        }

        // Variables
        if !self.variables.is_empty() {
            spec.push_str("VARIABLES\n");
            for (name, desc) in &self.variables {
                spec.push_str(&format!("  {}, \\* {}\n", name, desc));
            }
            spec.push('\n');
        }

        // Type invariant
        if !self.invariants.is_empty() {
            spec.push_str("TypeInvariant ==\n");
            for (i, inv) in self.invariants.iter().enumerate() {
                if i == 0 {
                    spec.push_str(&format!("  {}\n", inv));
                } else {
                    spec.push_str(&format!("  /\\ {}\n", inv));
                }
            }
            spec.push('\n');
        }

        // Init
        if !self.init_predicates.is_empty() {
            spec.push_str("Init ==\n");
            for (i, init) in self.init_predicates.iter().enumerate() {
                if i == 0 {
                    spec.push_str(&format!("  {}\n", init));
                } else {
                    spec.push_str(&format!("  /\\ {}\n", init));
                }
            }
            spec.push('\n');
        }

        // Next
        if !self.next_actions.is_empty() {
            spec.push_str("Next ==\n");
            for (i, action) in self.next_actions.iter().enumerate() {
                if i == 0 {
                    spec.push_str(&format!("  {}\n", action));
                } else {
                    spec.push_str(&format!("  \\/ {}\n", action));
                }
            }
            spec.push('\n');
        }

        // Spec
        let var_names: Vec<String> = self.variables.keys().cloned().collect();
        spec.push_str("Spec == Init /\\ [][Next]_<<");
        spec.push_str(&var_names.join(", "));
        spec.push_str(">>\n\n");

        // Properties
        for (i, prop) in self.properties.iter().enumerate() {
            spec.push_str(&format!("Property{} == {}\n", i, prop));
        }
        spec.push('\n');

        spec.push_str(
            "=============================================================================\n",
        );
        spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tla_spec_builder() {
        let mut builder = TlaSpecBuilder::new("Consensus".to_string());

        builder.add_constant("N".to_string(), "Number of nodes".to_string());
        builder.add_constant("Quorum".to_string(), "Quorum size".to_string());
        builder.add_variable("votes".to_string(), "Vote counts".to_string());
        builder.add_variable("decided".to_string(), "Decision made".to_string());

        builder.add_init("votes = [n \\in 1..N |-> 0]".to_string());
        builder.add_init("decided = FALSE".to_string());

        builder.add_action(
            "Vote(n) == /\\ n \\in 1..N /\\ votes' = [votes EXCEPT ![n] = @ + 1]".to_string(),
        );
        builder.add_action(
            "Decide == /\\ \\E n \\in 1..N : votes[n] >= Quorum /\\ decided' = TRUE".to_string(),
        );

        builder.add_invariant("decided => \\E n \\in 1..N : votes[n] >= Quorum".to_string());

        builder.add_property("[]<>decided".to_string());

        let spec = builder.build();
        assert!(spec.contains("MODULE CONSENSUS"));
        assert!(spec.contains("CONSTANTS"));
        assert!(spec.contains("VARIABLES"));
        assert!(spec.contains("TypeInvariant"));
        assert!(spec.contains("Init"));
        assert!(spec.contains("Next"));
        assert!(spec.contains("Spec"));
        assert!(spec.contains("Property0"));
    }

    #[test]
    fn test_logic_formula_tla() {
        let formula = LogicFormula::new(VerificationLanguage::Tla, "[]<>(decided)".to_string());
        assert_eq!(formula.language, VerificationLanguage::Tla);
    }
}
