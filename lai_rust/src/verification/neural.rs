/// Neural Network Verification Adapters for lai
///
/// Provides integration with neural network verification tools:
/// - α,β-CROWN: Bound propagation and branch-and-bound verification
/// - Marabou: SMT-based neural network verification
/// - ERAN: Abstract interpretation for robustness analysis
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::process::Command as TokioCommand;
use uuid::Uuid;

use crate::verification::contracts::{VerificationError, VerificationStatus};

/// Supported neural verification engines
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NeuralVerifierEngine {
    AlphaBetaCrown,
    Marabou,
    ERAN,
}

impl NeuralVerifierEngine {
    pub fn as_str(&self) -> &'static str {
        match self {
            NeuralVerifierEngine::AlphaBetaCrown => "alpha_beta_crown",
            NeuralVerifierEngine::Marabou => "marabou",
            NeuralVerifierEngine::ERAN => "eran",
        }
    }
}

/// Neural network model formats supported
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NeuralModelFormat {
    ONNX,
    PyTorch,
    TensorFlow,
    Keras,
    Marabou, // Marabou's native format
}

/// Neural network verification property types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NeuralPropertyType {
    /// Local robustness: ||f(x) - f(x+δ)|| < ε for ||δ|| < ε
    LocalRobustness,
    /// Global robustness over input region
    GlobalRobustness,
    /// Output range bounds
    OutputBounds,
    /// Safety property: output never in unsafe region
    Safety,
    /// Specification in VNN-LIB format
    VnnLib,
}

/// Neural network verification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuralVerificationConfig {
    pub engine: NeuralVerifierEngine,
    pub model_path: PathBuf,
    pub model_format: NeuralModelFormat,
    pub property_type: NeuralPropertyType,
    pub property_spec: String, // VNN-LIB or custom specification
    pub input_bounds: Option<Vec<(f64, f64)>>, // (min, max) per input dimension
    pub output_bounds: Option<Vec<(f64, f64)>>,
    pub timeout_seconds: u64,
    pub precision: Option<String>, // e.g., "fp32", "fp64"
    pub gpu_enabled: bool,
    pub gpu_id: Option<u32>,
    pub additional_args: Vec<String>,
}

impl Default for NeuralVerificationConfig {
    fn default() -> Self {
        Self {
            engine: NeuralVerifierEngine::AlphaBetaCrown,
            model_path: PathBuf::new(),
            model_format: NeuralModelFormat::ONNX,
            property_type: NeuralPropertyType::LocalRobustness,
            property_spec: String::new(),
            input_bounds: None,
            output_bounds: None,
            timeout_seconds: 300,
            precision: None,
            gpu_enabled: true,
            gpu_id: None,
            additional_args: Vec::new(),
        }
    }
}

/// Neural network verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuralVerificationResult {
    pub obligation_id: Uuid,
    pub engine: NeuralVerifierEngine,
    pub status: VerificationStatus,
    pub property_satisfied: Option<bool>,
    pub counterexample: Option<NeuralCounterexample>,
    pub bounds: Option<NeuralBounds>,
    pub execution_time_ms: u64,
    pub verifier_version: String,
    pub timestamp: SystemTime,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuralCounterexample {
    pub input: Vec<f64>,
    pub output: Vec<f64>,
    pub perturbation: Option<Vec<f64>>,
    pub property_violated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuralBounds {
    pub lower_bounds: Vec<f64>,
    pub upper_bounds: Vec<f64>,
    pub output_layer: usize,
}

/// Trait for neural network verifiers
#[async_trait]
pub trait NeuralVerifierAdapter: Send + Sync {
    fn engine(&self) -> NeuralVerifierEngine;
    fn version(&self) -> String;
    fn is_available(&self) -> bool;
    fn supported_formats(&self) -> Vec<NeuralModelFormat>;
    fn supported_properties(&self) -> Vec<NeuralPropertyType>;

    async fn verify(
        &self,
        config: &NeuralVerificationConfig,
    ) -> Result<NeuralVerificationResult, VerificationError>;
    async fn verify_batch(
        &self,
        configs: &[NeuralVerificationConfig],
    ) -> Result<Vec<NeuralVerificationResult>, VerificationError>;
}

/// α,β-CROWN Verifier
pub struct AlphaBetaCrownVerifier {
    crown_path: PathBuf,
    python_path: PathBuf,
    version: String,
    timeout: Duration,
    working_dir: PathBuf,
}

impl AlphaBetaCrownVerifier {
    pub fn new<P: AsRef<Path>, Q: AsRef<Path>>(
        crown_path: P,
        python_path: Q,
        working_dir: P,
    ) -> Result<Self, VerificationError> {
        let crown = crown_path.as_ref().to_path_buf();
        let python = python_path.as_ref().to_path_buf();
        let work_dir = working_dir.as_ref().to_path_buf();

        let version = Self::get_version(&python, &crown)?;

        Ok(Self {
            crown_path: crown,
            python_path: python,
            version,
            timeout: Duration::from_secs(300),
            working_dir: work_dir,
        })
    }

    pub fn with_timeout<P: AsRef<Path>, Q: AsRef<Path>>(
        crown_path: P,
        python_path: Q,
        working_dir: P,
        timeout: Duration,
    ) -> Result<Self, VerificationError> {
        let mut verifier = Self::new(crown_path, python_path, working_dir)?;
        verifier.timeout = timeout;
        Ok(verifier)
    }

    fn get_version(python_path: &Path, crown_path: &Path) -> Result<String, VerificationError> {
        // Try to run a simple version check
        let output = Command::new(python_path)
            .arg(crown_path)
            .arg("--version")
            .output()
            .map_err(|e| {
                VerificationError::NotAvailable(format!("Failed to run α,β-CROWN: {}", e))
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        // Extract version from output
        for line in combined.lines() {
            if line.contains("version") || line.contains("Version") || line.contains("α,β-CROWN")
            {
                return Ok(line.trim().to_string());
            }
        }

        Ok("Unknown".to_string())
    }

    fn build_command(&self, config: &NeuralVerificationConfig) -> Vec<String> {
        let mut args = vec![
            self.crown_path.to_string_lossy().to_string(),
            "--model".to_string(),
            config.model_path.to_string_lossy().to_string(),
            "--format".to_string(),
            format!("{:?}", config.model_format).to_lowercase(),
            "--property".to_string(),
            config.property_spec.clone(),
            "--timeout".to_string(),
            config.timeout_seconds.to_string(),
        ];

        if let Some(bounds) = &config.input_bounds {
            let bounds_str: Vec<String> = bounds
                .iter()
                .map(|(min, max)| format!("{}:{}", min, max))
                .collect();
            args.push("--input-bounds".to_string());
            args.push(bounds_str.join(","));
        }

        if config.gpu_enabled {
            args.push("--gpu".to_string());
            if let Some(gpu_id) = config.gpu_id {
                args.push("--gpu-id".to_string());
                args.push(gpu_id.to_string());
            }
        } else {
            args.push("--cpu".to_string());
        }

        if let Some(precision) = &config.precision {
            args.push("--precision".to_string());
            args.push(precision.clone());
        }

        args.extend(config.additional_args.clone());

        args
    }

    fn parse_output(&self, output: &str, obligation_id: Uuid) -> NeuralVerificationResult {
        let mut status = VerificationStatus::Unknown;
        let mut property_satisfied = None;
        let mut counterexample = None;
        let mut bounds = None;

        // Parse α,β-CROWN output
        if output.contains("verified") || output.contains("VERIFIED") || output.contains("sat") {
            status = VerificationStatus::Verified;
            property_satisfied = Some(true);
        } else if output.contains("falsified")
            || output.contains("FALSIFIED")
            || output.contains("unsat")
        {
            status = VerificationStatus::Falsified;
            property_satisfied = Some(false);

            // Try to extract counterexample
            counterexample = Some(NeuralCounterexample {
                input: vec![],
                output: vec![],
                perturbation: None,
                property_violated: "Robustness property violated".to_string(),
            });
        } else if output.contains("timeout")
            || output.contains("TIMEOUT")
            || output.contains("time limit")
        {
            status = VerificationStatus::Timeout;
        } else if output.contains("error") || output.contains("Error") || output.contains("ERROR") {
            status = VerificationStatus::Error;
        }

        // Try to extract bounds
        if output.contains("lower_bound") || output.contains("upper_bound") {
            bounds = Some(NeuralBounds {
                lower_bounds: vec![],
                upper_bounds: vec![],
                output_layer: 0,
            });
        }

        NeuralVerificationResult {
            obligation_id,
            engine: NeuralVerifierEngine::AlphaBetaCrown,
            status,
            property_satisfied,
            counterexample,
            bounds,
            execution_time_ms: 0,
            verifier_version: self.version.clone(),
            timestamp: SystemTime::now(),
            metadata: HashMap::new(),
        }
    }
}

#[async_trait]
impl NeuralVerifierAdapter for AlphaBetaCrownVerifier {
    fn engine(&self) -> NeuralVerifierEngine {
        NeuralVerifierEngine::AlphaBetaCrown
    }

    fn version(&self) -> String {
        self.version.clone()
    }

    fn is_available(&self) -> bool {
        Command::new(&self.python_path)
            .arg(&self.crown_path)
            .arg("--help")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn supported_formats(&self) -> Vec<NeuralModelFormat> {
        vec![
            NeuralModelFormat::ONNX,
            NeuralModelFormat::PyTorch,
            NeuralModelFormat::TensorFlow,
        ]
    }

    fn supported_properties(&self) -> Vec<NeuralPropertyType> {
        vec![
            NeuralPropertyType::LocalRobustness,
            NeuralPropertyType::GlobalRobustness,
            NeuralPropertyType::OutputBounds,
            NeuralPropertyType::Safety,
            NeuralPropertyType::VnnLib,
        ]
    }

    async fn verify(
        &self,
        config: &NeuralVerificationConfig,
    ) -> Result<NeuralVerificationResult, VerificationError> {
        let start = Instant::now();
        let obligation_id = Uuid::new_v4();

        let args = self.build_command(config);

        let output = TokioCommand::new(&self.python_path)
            .args(&args)
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

        let mut result = self.parse_output(&combined, obligation_id);
        result.execution_time_ms = execution_time;

        if !output.status.success() && result.status == VerificationStatus::Unknown {
            result.status = VerificationStatus::Error;
            result
                .metadata
                .insert("stderr".to_string(), stderr.to_string());
        }

        Ok(result)
    }

    async fn verify_batch(
        &self,
        configs: &[NeuralVerificationConfig],
    ) -> Result<Vec<NeuralVerificationResult>, VerificationError> {
        let mut results = Vec::new();
        for config in configs {
            let result = self.verify(config).await?;
            results.push(result);
        }
        Ok(results)
    }
}

/// Marabou Verifier
pub struct MarabouVerifier {
    marabou_path: PathBuf,
    version: String,
    timeout: Duration,
    working_dir: PathBuf,
}

impl MarabouVerifier {
    pub fn new<P: AsRef<Path>>(marabou_path: P, working_dir: P) -> Result<Self, VerificationError> {
        let marabou = marabou_path.as_ref().to_path_buf();
        let work_dir = working_dir.as_ref().to_path_buf();

        let version = Self::get_version(&marabou)?;

        Ok(Self {
            marabou_path: marabou,
            version,
            timeout: Duration::from_secs(600),
            working_dir: work_dir,
        })
    }

    pub fn with_timeout<P: AsRef<Path>>(
        marabou_path: P,
        working_dir: P,
        timeout: Duration,
    ) -> Result<Self, VerificationError> {
        let mut verifier = Self::new(marabou_path, working_dir)?;
        verifier.timeout = timeout;
        Ok(verifier)
    }

    fn get_version(marabou_path: &Path) -> Result<String, VerificationError> {
        let output = Command::new(marabou_path)
            .arg("--version")
            .output()
            .map_err(|e| {
                VerificationError::NotAvailable(format!("Failed to run Marabou: {}", e))
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.trim().to_string())
    }

    fn generate_marabou_query(
        &self,
        config: &NeuralVerificationConfig,
    ) -> Result<String, VerificationError> {
        // Generate Marabou query format (SMT-LIB like)
        let mut query = String::new();
        query.push_str("; Marabou query generated by lai\n");
        query.push_str(&format!("; Model: {}\n", config.model_path.display()));
        query.push_str(&format!("; Property: {:?}\n", config.property_type));
        query.push('\n');

        // This would generate proper Marabou input format
        // For now, return a placeholder
        query.push_str("(set-logic QF_NRA)\n");
        query.push_str("(exit)\n");

        Ok(query)
    }

    fn parse_output(&self, output: &str, obligation_id: Uuid) -> NeuralVerificationResult {
        let mut status = VerificationStatus::Unknown;
        let mut property_satisfied = None;
        let mut counterexample = None;

        if output.contains("UNSAT") || output.contains("unsat") || output.contains("property holds")
        {
            status = VerificationStatus::Verified;
            property_satisfied = Some(true);
        } else if output.contains("SAT")
            || output.contains("sat")
            || output.contains("property violated")
        {
            status = VerificationStatus::Falsified;
            property_satisfied = Some(false);
            counterexample = Some(NeuralCounterexample {
                input: vec![],
                output: vec![],
                perturbation: None,
                property_violated: "SAT - counterexample found".to_string(),
            });
        } else if output.contains("timeout") || output.contains("TIMEOUT") {
            status = VerificationStatus::Timeout;
        } else if output.contains("error") || output.contains("Error") {
            status = VerificationStatus::Error;
        }

        NeuralVerificationResult {
            obligation_id,
            engine: NeuralVerifierEngine::Marabou,
            status,
            property_satisfied,
            counterexample,
            bounds: None,
            execution_time_ms: 0,
            verifier_version: self.version.clone(),
            timestamp: SystemTime::now(),
            metadata: HashMap::new(),
        }
    }
}

#[async_trait]
impl NeuralVerifierAdapter for MarabouVerifier {
    fn engine(&self) -> NeuralVerifierEngine {
        NeuralVerifierEngine::Marabou
    }

    fn version(&self) -> String {
        self.version.clone()
    }

    fn is_available(&self) -> bool {
        Command::new(&self.marabou_path)
            .arg("--help")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn supported_formats(&self) -> Vec<NeuralModelFormat> {
        vec![NeuralModelFormat::ONNX, NeuralModelFormat::Marabou]
    }

    fn supported_properties(&self) -> Vec<NeuralPropertyType> {
        vec![
            NeuralPropertyType::LocalRobustness,
            NeuralPropertyType::GlobalRobustness,
            NeuralPropertyType::OutputBounds,
            NeuralPropertyType::Safety,
            NeuralPropertyType::VnnLib,
        ]
    }

    async fn verify(
        &self,
        config: &NeuralVerificationConfig,
    ) -> Result<NeuralVerificationResult, VerificationError> {
        let start = Instant::now();
        let obligation_id = Uuid::new_v4();

        // Generate query file
        let query = self.generate_marabou_query(config)?;
        let query_file = self
            .working_dir
            .join(format!("marabou_query_{}.txt", obligation_id));
        tokio::fs::write(&query_file, &query)
            .await
            .map_err(VerificationError::IoError)?;

        let output = TokioCommand::new(&self.marabou_path)
            .arg("--input-query")
            .arg(&query_file)
            .arg("--timeout")
            .arg(config.timeout_seconds.to_string())
            .current_dir(&self.working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(VerificationError::IoError)?;

        let execution_time = start.elapsed().as_millis() as u64;

        // Cleanup
        let _ = tokio::fs::remove_file(&query_file).await;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        let mut result = self.parse_output(&combined, obligation_id);
        result.execution_time_ms = execution_time;

        if !output.status.success() && result.status == VerificationStatus::Unknown {
            result.status = VerificationStatus::Error;
        }

        Ok(result)
    }

    async fn verify_batch(
        &self,
        configs: &[NeuralVerificationConfig],
    ) -> Result<Vec<NeuralVerificationResult>, VerificationError> {
        let mut results = Vec::new();
        for config in configs {
            let result = self.verify(config).await?;
            results.push(result);
        }
        Ok(results)
    }
}

/// ERAN Verifier
pub struct EranVerifier {
    eran_path: PathBuf,
    python_path: PathBuf,
    version: String,
    timeout: Duration,
    working_dir: PathBuf,
}

impl EranVerifier {
    pub fn new<P: AsRef<Path>, Q: AsRef<Path>>(
        eran_path: P,
        python_path: Q,
        working_dir: P,
    ) -> Result<Self, VerificationError> {
        let eran = eran_path.as_ref().to_path_buf();
        let python = python_path.as_ref().to_path_buf();
        let work_dir = working_dir.as_ref().to_path_buf();

        let version = Self::get_version(&python, &eran)?;

        Ok(Self {
            eran_path: eran,
            python_path: python,
            version,
            timeout: Duration::from_secs(300),
            working_dir: work_dir,
        })
    }

    pub fn with_timeout<P: AsRef<Path>, Q: AsRef<Path>>(
        eran_path: P,
        python_path: Q,
        working_dir: P,
        timeout: Duration,
    ) -> Result<Self, VerificationError> {
        let mut verifier = Self::new(eran_path, python_path, working_dir)?;
        verifier.timeout = timeout;
        Ok(verifier)
    }

    fn get_version(python_path: &Path, eran_path: &Path) -> Result<String, VerificationError> {
        let output = Command::new(python_path)
            .arg(eran_path)
            .arg("--version")
            .output()
            .map_err(|e| VerificationError::NotAvailable(format!("Failed to run ERAN: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        for line in combined.lines() {
            if line.contains("version") || line.contains("Version") || line.contains("ERAN") {
                return Ok(line.trim().to_string());
            }
        }

        Ok("Unknown".to_string())
    }

    fn build_command(&self, config: &NeuralVerificationConfig) -> Vec<String> {
        let mut args = vec![
            self.eran_path.to_string_lossy().to_string(),
            "--model".to_string(),
            config.model_path.to_string_lossy().to_string(),
            "--format".to_string(),
            format!("{:?}", config.model_format).to_lowercase(),
            "--property".to_string(),
            config.property_spec.clone(),
            "--timeout".to_string(),
            config.timeout_seconds.to_string(),
        ];

        if let Some(bounds) = &config.input_bounds {
            let bounds_str: Vec<String> = bounds
                .iter()
                .map(|(min, max)| format!("{}:{}", min, max))
                .collect();
            args.push("--input-bounds".to_string());
            args.push(bounds_str.join(","));
        }

        // ERAN specific arguments
        args.push("--domain".to_string());
        args.push("deeppoly".to_string()); // Default abstract domain

        args.extend(config.additional_args.clone());

        args
    }

    fn parse_output(&self, output: &str, obligation_id: Uuid) -> NeuralVerificationResult {
        let mut status = VerificationStatus::Unknown;
        let mut property_satisfied = None;
        let mut bounds = None;

        if output.contains("verified") || output.contains("VERIFIED") || output.contains("holds") {
            status = VerificationStatus::Verified;
            property_satisfied = Some(true);
        } else if output.contains("violated")
            || output.contains("FAILED")
            || output.contains("counterexample")
        {
            status = VerificationStatus::Falsified;
            property_satisfied = Some(false);
        } else if output.contains("timeout") || output.contains("TIMEOUT") {
            status = VerificationStatus::Timeout;
        } else if output.contains("error") || output.contains("Error") {
            status = VerificationStatus::Error;
        }

        // ERAN provides bounds
        if output.contains("lower") && output.contains("upper") {
            bounds = Some(NeuralBounds {
                lower_bounds: vec![],
                upper_bounds: vec![],
                output_layer: 0,
            });
        }

        NeuralVerificationResult {
            obligation_id,
            engine: NeuralVerifierEngine::ERAN,
            status,
            property_satisfied,
            counterexample: None,
            bounds,
            execution_time_ms: 0,
            verifier_version: self.version.clone(),
            timestamp: SystemTime::now(),
            metadata: HashMap::new(),
        }
    }
}

#[async_trait]
impl NeuralVerifierAdapter for EranVerifier {
    fn engine(&self) -> NeuralVerifierEngine {
        NeuralVerifierEngine::ERAN
    }

    fn version(&self) -> String {
        self.version.clone()
    }

    fn is_available(&self) -> bool {
        Command::new(&self.python_path)
            .arg(&self.eran_path)
            .arg("--help")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn supported_formats(&self) -> Vec<NeuralModelFormat> {
        vec![
            NeuralModelFormat::ONNX,
            NeuralModelFormat::PyTorch,
            NeuralModelFormat::TensorFlow,
            NeuralModelFormat::Keras,
        ]
    }

    fn supported_properties(&self) -> Vec<NeuralPropertyType> {
        vec![
            NeuralPropertyType::LocalRobustness,
            NeuralPropertyType::GlobalRobustness,
            NeuralPropertyType::OutputBounds,
            NeuralPropertyType::Safety,
        ]
    }

    async fn verify(
        &self,
        config: &NeuralVerificationConfig,
    ) -> Result<NeuralVerificationResult, VerificationError> {
        let start = Instant::now();
        let obligation_id = Uuid::new_v4();

        let args = self.build_command(config);

        let output = TokioCommand::new(&self.python_path)
            .args(&args)
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

        let mut result = self.parse_output(&combined, obligation_id);
        result.execution_time_ms = execution_time;

        if !output.status.success() && result.status == VerificationStatus::Unknown {
            result.status = VerificationStatus::Error;
        }

        Ok(result)
    }

    async fn verify_batch(
        &self,
        configs: &[NeuralVerificationConfig],
    ) -> Result<Vec<NeuralVerificationResult>, VerificationError> {
        let mut results = Vec::new();
        for config in configs {
            let result = self.verify(config).await?;
            results.push(result);
        }
        Ok(results)
    }
}

/// Unified neural verification manager
pub struct NeuralVerificationManager {
    verifiers: HashMap<NeuralVerifierEngine, Box<dyn NeuralVerifierAdapter>>,
}

impl NeuralVerificationManager {
    pub fn new() -> Self {
        Self {
            verifiers: HashMap::new(),
        }
    }

    pub fn register_verifier(&mut self, verifier: Box<dyn NeuralVerifierAdapter>) {
        self.verifiers.insert(verifier.engine(), verifier);
    }

    pub fn get_verifier(&self, engine: NeuralVerifierEngine) -> Option<&dyn NeuralVerifierAdapter> {
        self.verifiers.get(&engine).map(|v| v.as_ref())
    }

    pub fn available_engines(&self) -> Vec<NeuralVerifierEngine> {
        self.verifiers.keys().cloned().collect()
    }

    pub async fn verify(
        &self,
        config: &NeuralVerificationConfig,
    ) -> Result<NeuralVerificationResult, VerificationError> {
        let verifier = self.verifiers.get(&config.engine).ok_or_else(|| {
            VerificationError::NotAvailable(format!("Verifier {:?} not registered", config.engine))
        })?;

        if !verifier.is_available() {
            return Err(VerificationError::NotAvailable(format!(
                "Verifier {:?} not available",
                config.engine
            )));
        }

        verifier.verify(config).await
    }

    pub async fn verify_with_all(
        &self,
        config: &NeuralVerificationConfig,
    ) -> HashMap<NeuralVerifierEngine, Result<NeuralVerificationResult, VerificationError>> {
        let mut results = HashMap::new();

        for (engine, verifier) in &self.verifiers {
            if verifier.is_available() {
                let mut engine_config = config.clone();
                engine_config.engine = *engine;
                results.insert(*engine, verifier.verify(&engine_config).await);
            } else {
                results.insert(
                    *engine,
                    Err(VerificationError::NotAvailable(format!(
                        "{:?} not available",
                        engine
                    ))),
                );
            }
        }

        results
    }
}

impl Default for NeuralVerificationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for neural verification configurations
pub struct NeuralVerificationConfigBuilder {
    config: NeuralVerificationConfig,
}

impl NeuralVerificationConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: NeuralVerificationConfig::default(),
        }
    }

    pub fn engine(mut self, engine: NeuralVerifierEngine) -> Self {
        self.config.engine = engine;
        self
    }

    pub fn model_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.config.model_path = path.as_ref().to_path_buf();
        self
    }

    pub fn model_format(mut self, format: NeuralModelFormat) -> Self {
        self.config.model_format = format;
        self
    }

    pub fn property_type(mut self, property_type: NeuralPropertyType) -> Self {
        self.config.property_type = property_type;
        self
    }

    pub fn property_spec(mut self, spec: String) -> Self {
        self.config.property_spec = spec;
        self
    }

    pub fn input_bounds(mut self, bounds: Vec<(f64, f64)>) -> Self {
        self.config.input_bounds = Some(bounds);
        self
    }

    pub fn timeout(mut self, seconds: u64) -> Self {
        self.config.timeout_seconds = seconds;
        self
    }

    pub fn gpu_enabled(mut self, enabled: bool) -> Self {
        self.config.gpu_enabled = enabled;
        self
    }

    pub fn gpu_id(mut self, id: u32) -> Self {
        self.config.gpu_id = Some(id);
        self
    }

    pub fn build(self) -> NeuralVerificationConfig {
        self.config
    }
}

impl Default for NeuralVerificationConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neural_verification_config_builder() {
        let config = NeuralVerificationConfigBuilder::new()
            .engine(NeuralVerifierEngine::AlphaBetaCrown)
            .model_path("model.onnx")
            .model_format(NeuralModelFormat::ONNX)
            .property_type(NeuralPropertyType::LocalRobustness)
            .property_spec("(declare-fun X () Real)".to_string())
            .input_bounds(vec![(0.0, 1.0), (-1.0, 1.0)])
            .timeout(60)
            .gpu_enabled(true)
            .build();

        assert_eq!(config.engine, NeuralVerifierEngine::AlphaBetaCrown);
        assert_eq!(config.model_format, NeuralModelFormat::ONNX);
        assert_eq!(config.property_type, NeuralPropertyType::LocalRobustness);
        assert_eq!(config.timeout_seconds, 60);
        assert!(config.gpu_enabled);
    }

    #[test]
    fn test_neural_verification_manager() {
        let manager = NeuralVerificationManager::new();
        assert!(manager.available_engines().is_empty());
    }

    #[test]
    fn test_neural_property_types() {
        assert_eq!(NeuralPropertyType::LocalRobustness as u8, 0);
        assert_eq!(NeuralPropertyType::VnnLib as u8, 4);
    }

    #[test]
    fn test_verification_engines() {
        assert_eq!(
            NeuralVerifierEngine::AlphaBetaCrown.as_str(),
            "alpha_beta_crown"
        );
        assert_eq!(NeuralVerifierEngine::Marabou.as_str(), "marabou");
        assert_eq!(NeuralVerifierEngine::ERAN.as_str(), "eran");
    }
}
