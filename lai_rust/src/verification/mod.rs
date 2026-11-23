/// Formal Verification Module for lai
/// 
/// Provides integration with formal verification tools:
/// - Dafny: Static verification for Rust/Python code
/// - TLA+/TLC: Model checking for concurrent/distributed systems
/// - α,β-CROWN, Marabou, ERAN: Neural network verification

pub mod contracts;
pub mod dafny;
pub mod neural;
pub mod tla;

pub use contracts::{
    VerificationLanguage, VerificationStatus, LogicFormula, ProofObligation,
    ProofPriority, SourceLocation, ProofResult, VerificationContract,
    ContractVerificationResult, VerificationAdapter, VerificationFeatures,
    VerificationError,
};

pub use dafny::{
    DafnyVerifier, DafnySpecBuilder, DafnyFunctionSpec, DafnyTypeSpec,
};

pub use neural::{
    NeuralVerifierEngine, NeuralModelFormat, NeuralPropertyType,
    NeuralVerificationConfig, NeuralVerificationResult, NeuralCounterexample,
    NeuralBounds, NeuralVerifierAdapter, AlphaBetaCrownVerifier,
    MarabouVerifier, EranVerifier, NeuralVerificationManager,
    NeuralVerificationConfigBuilder,
};

pub use tla::{
    TlaVerifier, TlaSpecBuilder,
};