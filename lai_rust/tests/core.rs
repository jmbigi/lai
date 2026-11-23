//! Unit tests for LAI core modules

use lai::policy::{PolicyEngine, PolicyConfig, Operation, RiskLevel};
use lai::verification::*;
use std::path::PathBuf;

#[test]
fn test_policy_engine_creation() {
    let config = PolicyConfig::default();
    let mut engine = PolicyEngine::new(config);
    
    // Should create without panic
    // Test via behavior instead of private field
    let decision = engine.check(&Operation::FileWrite {
        path: "test.txt".to_string(),
    });
    assert!(decision.allowed);
}

#[test]
fn test_policy_engine_file_write() {
    let mut engine = PolicyEngine::default();
    let workspace = std::env::current_dir().unwrap();
    
    // Write within workspace should be allowed
    let decision = engine.check(&Operation::FileWrite {
        path: workspace.join("test.txt").to_string_lossy().to_string(),
    });
    
    assert!(decision.allowed);
    assert_eq!(decision.risk_level, RiskLevel::Medium);
}

#[test]
fn test_policy_engine_protected_path() {
    let mut engine = PolicyEngine::default();
    
    // Write to protected path should be denied
    let decision = engine.check(&Operation::FileWrite {
        path: "lai_rust/src/main.rs".to_string(),
    });
    
    assert!(!decision.allowed);
    assert_eq!(decision.risk_level, RiskLevel::Critical);
    assert!(decision.reason.contains("Protected"));
}

#[test]
fn test_policy_engine_outside_workspace() {
    let mut engine = PolicyEngine::default();
    
    // Write outside workspace should be denied
    let decision = engine.check(&Operation::FileWrite {
        path: "/etc/passwd".to_string(),
    });
    
    assert!(!decision.allowed);
    assert_eq!(decision.risk_level, RiskLevel::High);
}

#[test]
fn test_policy_engine_command_execution() {
    let mut engine = PolicyEngine::default();
    
    // Allowed command
    let decision = engine.check(&Operation::CommandExec {
        command: "cargo build".to_string(),
    });
    
    assert!(decision.allowed);
    
    // Disallowed command (not in allowlist)
    let decision = engine.check(&Operation::CommandExec {
        command: "rm -rf /".to_string(),
    });
    
    assert!(!decision.allowed);
    // Not in allowlist, so returns High (not Critical which is for dangerous patterns)
    assert_eq!(decision.risk_level, RiskLevel::High);
    
    // Test a command that IS in allowlist but is dangerous
    let decision = engine.check(&Operation::CommandExec {
        command: "cargo run -- rm -rf /tmp/test".to_string(),
    });
    // cargo is allowed, but contains dangerous pattern
    assert!(!decision.allowed);
    assert_eq!(decision.risk_level, RiskLevel::Critical);
}

#[test]
fn test_policy_engine_network_blocked() {
    let mut engine = PolicyEngine::default();
    
    // Network access should be blocked by default
    let decision = engine.check(&Operation::NetworkAccess {
        url: "https://api.github.com".to_string(),
    });
    
    assert!(!decision.allowed);
    assert_eq!(decision.risk_level, RiskLevel::Critical);
}

#[test]
fn test_verification_status() {
    let status = VerificationStatus::Verified;
    assert!(matches!(status, VerificationStatus::Verified));
    
    let status = VerificationStatus::Falsified;
    assert!(matches!(status, VerificationStatus::Falsified));
}

#[test]
fn test_verification_language() {
    assert_eq!(VerificationLanguage::Dafny.as_str(), "dafny");
    assert_eq!(VerificationLanguage::Tla.as_str(), "tla");
    assert_eq!(VerificationLanguage::Z3.as_str(), "z3");
    assert_eq!(VerificationLanguage::Cvc5.as_str(), "cvc5");
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
    assert_eq!(obligation.priority, ProofPriority::Normal);
    assert!(obligation.location.is_none());
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
    assert_eq!(contract.component, "math_utils");
}