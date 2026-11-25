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
    ContractVerificationResult, LogicFormula, ProofObligation, ProofPriority, ProofResult,
    SourceLocation, VerificationAdapter, VerificationContract, VerificationError,
    VerificationFeatures, VerificationLanguage, VerificationStatus,
};

pub use dafny::{DafnyFunctionSpec, DafnySpecBuilder, DafnyTypeSpec, DafnyVerifier};

pub use neural::{
    AlphaBetaCrownVerifier, EranVerifier, MarabouVerifier, NeuralBounds, NeuralCounterexample,
    NeuralModelFormat, NeuralPropertyType, NeuralVerificationConfig,
    NeuralVerificationConfigBuilder, NeuralVerificationManager, NeuralVerificationResult,
    NeuralVerifierAdapter, NeuralVerifierEngine,
};

pub use tla::{TlaSpecBuilder, TlaVerifier};
