//! testing - Property-based testing y mutation testing

pub mod property;
pub mod mutation;

// Re-exports
pub use property::{
    PropertyTestConfig, PropertyTestResult, TestableProperty,
    IdempotenceProperty, CommutativityProperty, AssociativityProperty, BoundaryProperty,
    PropertyTestRunner, strategies,
};
pub use mutation::{
    MutationTestConfig, MutationTestResult, MutantResult, MutantStatus, Mutator,
    MutationTestEngine, MutationTestError, MutationReportGenerator, PRIORITY_MUTATIONS,
};