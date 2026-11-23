//! testing/property.rs - Property-based testing con proptest
//! 
//! Implementa testing basado en propiedades para descubrir errores automáticamente.
//! Basado en proptest (equivalente Rust de Hypothesis).

use proptest::prelude::*;
use proptest::strategy::Strategy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuración para property-based testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyTestConfig {
    pub max_cases: u32,
    pub max_shrink_iters: u32,
    pub timeout_seconds: u64,
    pub verbose: bool,
}

impl Default for PropertyTestConfig {
    fn default() -> Self {
        Self {
            max_cases: 1000,
            max_shrink_iters: 100,
            timeout_seconds: 30,
            verbose: false,
        }
    }
}

/// Resultado de un property test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyTestResult {
    pub property_name: String,
    pub passed: bool,
    pub cases_tested: u32,
    pub counterexample: Option<String>,
    pub minimal_counterexample: Option<String>,
    pub shrink_steps: u32,
    pub duration_ms: u64,
}

/// Trait para definir propiedades testables
pub trait TestableProperty {
    fn name(&self) -> &str;
    fn run(&self, config: &PropertyTestConfig) -> PropertyTestResult;
}

/// Property test para idempotencia: f(f(x)) == f(x)
pub struct IdempotenceProperty<F, T> 
where
    F: Fn(&T) -> T,
    T: Clone + std::fmt::Debug + 'static,
{
    name: String,
    function: F,
    strategy: Box<dyn Strategy<Value = T>>,
}

impl<F, T> IdempotenceProperty<F, T>
where
    F: Fn(&T) -> T,
    T: Clone + std::fmt::Debug + 'static,
{
    pub fn new(name: String, function: F, strategy: impl Strategy<Value = T> + 'static) -> Self {
        Self {
            name,
            function,
            strategy: Box::new(strategy),
        }
    }
}

impl<F, T> TestableProperty for IdempotenceProperty<F, T>
where
    F: Fn(&T) -> T,
    T: Clone + std::fmt::Debug + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&self, config: &PropertyTestConfig) -> PropertyTestResult {
        let start = std::time::Instant::now();
        let function = &self.function;
        let strategy = self.strategy.clone();
        
        let result = proptest::TestRunner::new(proptest::Config {
            cases: config.max_cases,
            max_shrink_iters: config.max_shrink_iters,
            timeout: Some(std::time::Duration::from_secs(config.timeout_seconds)),
            verbose: config.verbose,
            ..Default::default()
        }).run(&strategy, |input| {
            let first = function(&input);
            let second = function(&first);
            prop_assert_eq!(first, second);
        });

        let duration = start.elapsed().as_millis() as u64;
        
        match result {
            Ok(_) => PropertyTestResult {
                property_name: self.name.clone(),
                passed: true,
                cases_tested: config.max_cases,
                counterexample: None,
                minimal_counterexample: None,
                shrink_steps: 0,
                duration_ms: duration,
            },
            Err(e) => {
                let counterexample = format!("{:?}", e.failure);
                PropertyTestResult {
                    property_name: self.name.clone(),
                    passed: false,
                    cases_tested: config.max_cases,
                    counterexample: Some(counterexample.clone()),
                    minimal_counterexample: Some(counterexample),
                    shrink_steps: 0, // proptest doesn't expose this easily
                    duration_ms: duration,
                }
            }
        }
    }
}

/// Property test para commutativity: f(a, b) == f(b, a)
pub struct CommutativityProperty<F, T>
where
    F: Fn(&T, &T) -> T,
    T: Clone + std::fmt::Debug + 'static,
{
    name: String,
    function: F,
    strategy: Box<dyn Strategy<Value = T>>,
}

impl<F, T> CommutativityProperty<F, T>
where
    F: Fn(&T, &T) -> T,
    T: Clone + std::fmt::Debug + 'static,
{
    pub fn new(name: String, function: F, strategy: impl Strategy<Value = T> + 'static) -> Self {
        Self {
            name,
            function,
            strategy: Box::new(strategy),
        }
    }
}

impl<F, T> TestableProperty for CommutativityProperty<F, T>
where
    F: Fn(&T, &T) -> T,
    T: Clone + std::fmt::Debug + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&self, config: &PropertyTestConfig) -> PropertyTestResult {
        let start = std::time::Instant::now();
        let function = &self.function;
        let strategy = self.strategy.clone();
        
        let result = proptest::TestRunner::new(proptest::Config {
            cases: config.max_cases,
            max_shrink_iters: config.max_shrink_iters,
            timeout: Some(std::time::Duration::from_secs(config.timeout_seconds)),
            verbose: config.verbose,
            ..Default::default()
        }).run(&(strategy.clone(), strategy), |(a, b)| {
            let result1 = function(&a, &b);
            let result2 = function(&b, &a);
            prop_assert_eq!(result1, result2);
        });

        let duration = start.elapsed().as_millis() as u64;
        
        match result {
            Ok(_) => PropertyTestResult {
                property_name: self.name.clone(),
                passed: true,
                cases_tested: config.max_cases,
                counterexample: None,
                minimal_counterexample: None,
                shrink_steps: 0,
                duration_ms: duration,
            },
            Err(e) => PropertyTestResult {
                property_name: self.name.clone(),
                passed: false,
                cases_tested: config.max_cases,
                counterexample: Some(format!("{:?}", e.failure)),
                minimal_counterexample: Some(format!("{:?}", e.failure)),
                shrink_steps: 0,
                duration_ms: duration,
            }
        }
    }
}

/// Property test para associativity: f(f(a, b), c) == f(a, f(b, c))
pub struct AssociativityProperty<F, T>
where
    F: Fn(&T, &T) -> T,
    T: Clone + std::fmt::Debug + 'static,
{
    name: String,
    function: F,
    strategy: Box<dyn Strategy<Value = T>>,
}

impl<F, T> AssociativityProperty<F, T>
where
    F: Fn(&T, &T) -> T,
    T: Clone + std::fmt::Debug + 'static,
{
    pub fn new(name: String, function: F, strategy: impl Strategy<Value = T> + 'static) -> Self {
        Self {
            name,
            function,
            strategy: Box::new(strategy),
        }
    }
}

impl<F, T> TestableProperty for AssociativityProperty<F, T>
where
    F: Fn(&T, &T) -> T,
    T: Clone + std::fmt::Debug + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&self, config: &PropertyTestConfig) -> PropertyTestResult {
        let start = std::time::Instant::now();
        let function = &self.function;
        let strategy = self.strategy.clone();
        
        let result = proptest::TestRunner::new(proptest::Config {
            cases: config.max_cases,
            max_shrink_iters: config.max_shrink_iters,
            timeout: Some(std::time::Duration::from_secs(config.timeout_seconds)),
            verbose: config.verbose,
            ..Default::default()
        }).run(&(strategy.clone(), strategy.clone(), strategy), |(a, b, c)| {
            let left = function(&function(&a, &b), &c);
            let right = function(&a, &function(&b, &c));
            prop_assert_eq!(left, right);
        });

        let duration = start.elapsed().as_millis() as u64;
        
        match result {
            Ok(_) => PropertyTestResult {
                property_name: self.name.clone(),
                passed: true,
                cases_tested: config.max_cases,
                counterexample: None,
                minimal_counterexample: None,
                shrink_steps: 0,
                duration_ms: duration,
            },
            Err(e) => PropertyTestResult {
                property_name: self.name.clone(),
                passed: false,
                cases_tested: config.max_cases,
                counterexample: Some(format!("{:?}", e.failure)),
                minimal_counterexample: Some(format!("{:?}", e.failure)),
                shrink_steps: 0,
                duration_ms: duration,
            }
        }
    }
}

/// Property test para límites/boundaries
pub struct BoundaryProperty<F, T>
where
    F: Fn(&T) -> bool,
    T: Clone + std::fmt::Debug + 'static,
{
    name: String,
    predicate: F,
    strategy: Box<dyn Strategy<Value = T>>,
    should_pass: bool, // true = predicate should hold, false = should fail
}

impl<F, T> BoundaryProperty<F, T>
where
    F: Fn(&T) -> bool,
    T: Clone + std::fmt::Debug + 'static,
{
    pub fn new(name: String, predicate: F, strategy: impl Strategy<Value = T> + 'static, should_pass: bool) -> Self {
        Self {
            name,
            predicate,
            strategy: Box::new(strategy),
            should_pass,
        }
    }
}

impl<F, T> TestableProperty for BoundaryProperty<F, T>
where
    F: Fn(&T) -> bool,
    T: Clone + std::fmt::Debug + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&self, config: &PropertyTestConfig) -> PropertyTestResult {
        let start = std::time::Instant::now();
        let predicate = &self.predicate;
        let strategy = self.strategy.clone();
        let should_pass = self.should_pass;
        
        let result = proptest::TestRunner::new(proptest::Config {
            cases: config.max_cases,
            max_shrink_iters: config.max_shrink_iters,
            timeout: Some(std::time::Duration::from_secs(config.timeout_seconds)),
            verbose: config.verbose,
            ..Default::default()
        }).run(&strategy, move |input| {
            let result = predicate(&input);
            if should_pass {
                prop_assert!(result);
            } else {
                prop_assert!(!result);
            }
        });

        let duration = start.elapsed().as_millis() as u64;
        
        match result {
            Ok(_) => PropertyTestResult {
                property_name: self.name.clone(),
                passed: true,
                cases_tested: config.max_cases,
                counterexample: None,
                minimal_counterexample: None,
                shrink_steps: 0,
                duration_ms: duration,
            },
            Err(e) => PropertyTestResult {
                property_name: self.name.clone(),
                passed: false,
                cases_tested: config.max_cases,
                counterexample: Some(format!("{:?}", e.failure)),
                minimal_counterexample: Some(format!("{:?}", e.failure)),
                shrink_steps: 0,
                duration_ms: duration,
            }
        }
    }
}

/// Runner para ejecutar múltiples property tests
pub struct PropertyTestRunner {
    config: PropertyTestConfig,
    tests: Vec<Box<dyn TestableProperty>>,
}

impl PropertyTestRunner {
    pub fn new(config: PropertyTestConfig) -> Self {
        Self {
            config,
            tests: Vec::new(),
        }
    }

    pub fn add_test(&mut self, test: Box<dyn TestableProperty>) {
        self.tests.push(test);
    }

    pub fn run_all(&self) -> Vec<PropertyTestResult> {
        self.tests.iter().map(|t| t.run(&self.config)).collect()
    }

    pub fn summary(&self, results: &[PropertyTestResult]) -> String {
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = total - passed;
        
        let mut out = String::new();
        out.push_str(&format!("Property Tests: {} total, {} passed, {} failed\n", total, passed, failed));
        
        for result in results {
            let status = if result.passed { "✓" } else { "✗" };
            out.push_str(&format!("  {} {} ({} cases, {}ms)", status, result.property_name, result.cases_tested, result.duration_ms));
            if let Some(ce) = &result.counterexample {
                out.push_str(&format!("\n    Counterexample: {}", ce));
            }
            out.push_str("\n");
        }
        
        out
    }
}

/// Estrategias comunes para testing
pub mod strategies {
    use proptest::prelude::*;
    use proptest::strategy::Strategy;

    /// Strings válidos para paths
    pub fn valid_path() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_.\\-/]+".prop_map(|s| s.to_string())
    }

    /// Identificadores válidos (nombres de funciones, variables)
    pub fn valid_identifier() -> impl Strategy<Value = String> {
        "[a-zA-Z_][a-zA-Z0-9_]*".prop_map(|s| s.to_string())
    }

    /// Números enteros positivos
    pub fn positive_int() -> impl Strategy<Value = i64> {
        1..10000i64
    }

    /// Números en rango
    pub fn int_range(min: i64, max: i64) -> impl Strategy<Value = i64> {
        min..max
    }

    /// Vectores de enteros
    pub fn int_vec(min_len: usize, max_len: usize) -> impl Strategy<Value = Vec<i64>> {
        proptest::collection::vec(1..100i64, min_len..max_len)
    }

    /// Strings con longitud limitada
    pub fn bounded_string(min_len: usize, max_len: usize) -> impl Strategy<Value = String> {
        "[\\x20-\\x7E]{0,1000}".prop_map(|s: String| s.chars().take(max_len).collect::<String>())
    }

    /// JSON values
    pub fn json_value() -> impl Strategy<Value = serde_json::Value> {
        prop::collection::vec(any::<u8>(), 0..100)
            .prop_map(|bytes| serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_idempotence_property() {
        // Test: absolute value is idempotent
        let prop = IdempotenceProperty::new(
            "abs_idempotent".to_string(),
            |x: &i32| x.abs(),
            any::<i32>(),
        );
        
        let config = PropertyTestConfig::default();
        let result = prop.run(&config);
        assert!(result.passed);
    }

    #[test]
    fn test_commutativity_property() {
        // Test: addition is commutative
        let prop = CommutativityProperty::new(
            "add_commutative".to_string(),
            |a: &i32, b: &i32| a + b,
            any::<i32>(),
        );
        
        let config = PropertyTestConfig::default();
        let result = prop.run(&config);
        assert!(result.passed);
    }

    #[test]
    fn test_associativity_property() {
        // Test: addition is associative
        let prop = AssociativityProperty::new(
            "add_associative".to_string(),
            |a: &i32, b: &i32| a + b,
            any::<i32>(),
        );
        
        let config = PropertyTestConfig::default();
        let result = prop.run(&config);
        assert!(result.passed);
    }

    #[test]
    fn test_boundary_property() {
        // Test: positive numbers are > 0
        let prop = BoundaryProperty::new(
            "positive_gt_zero".to_string(),
            |x: &i32| *x > 0,
            1..1000i32,
            true,
        );
        
        let config = PropertyTestConfig::default();
        let result = prop.run(&config);
        assert!(result.passed);
    }

    #[test]
    fn test_property_runner() {
        let mut runner = PropertyTestRunner::new(PropertyTestConfig::default());
        
        runner.add_test(Box::new(IdempotenceProperty::new(
            "abs_idempotent".to_string(),
            |x: &i32| x.abs(),
            any::<i32>(),
        )));
        
        runner.add_test(Box::new(CommutativityProperty::new(
            "add_commutative".to_string(),
            |a: &i32, b: &i32| a + b,
            any::<i32>(),
        )));
        
        let results = runner.run_all();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.passed));
        
        let summary = runner.summary(&results);
        assert!(summary.contains("2 total"));
        assert!(summary.contains("2 passed"));
    }
}