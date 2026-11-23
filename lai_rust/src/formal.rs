//! Métodos formales ligeros (v5.0, herramienta 8: Dafny / TLA+).
//!
//! No sustituye a Dafny ni a TLA+: implementa el patrón *generate → check →
//! repair* sobre contratos ejecutables declarados en el `SPEC.md`:
//!
//! ```text
//! contracts:
//!   - requires: "x > 0"
//!     ensures:  "result == x * 2"
//! ```
//!
//! Las pre/postcondiciones se evalúan como expresiones Python sobre un
//! estado JSON (pre y post). Reglas de veredicto (v5.0):
//!
//! - Todas las cláusulas `requires`/`ensures` evaluadas y verdaderas →
//!   contrato cerrado (`Safe`).
//! - El alcance exigía el contrato y no cerró (violación, timeout de
//!   reparación o cláusula no evaluable) → `BLOCKED`.
//! - Contrato opcional no evaluable → gap declarado.

use crate::contracts::ExternalVerdict;
use serde::{Deserialize, Serialize};

/// Cláusula de contrato (pre o post).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractClause {
    pub kind: String, // "requires" | "ensures"
    pub expression: String,
}

/// Contrato completo de un símbolo o módulo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSpec {
    pub target: String,
    pub clauses: Vec<ContractClause>,
    /// Si true, el `verification_scope` lo exige: no cerrar → BLOCKED.
    #[serde(default)]
    pub required: bool,
}

impl ContractSpec {
    pub fn requires(&self) -> Vec<&ContractClause> {
        self.clauses.iter().filter(|c| c.kind == "requires").collect()
    }
    pub fn ensures(&self) -> Vec<&ContractClause> {
        self.clauses.iter().filter(|c| c.kind == "ensures").collect()
    }
}

/// Resultado de evaluar una cláusula.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClauseResult {
    pub expression: String,
    pub satisfied: bool,
    pub error: Option<String>,
}

/// Resultado de verificar un contrato.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractResult {
    pub target: String,
    pub clauses: Vec<ClauseResult>,
    pub closed: bool,
    /// Intentos de reparación realizados (patrón generate→check→repair).
    pub repair_attempts: u32,
}

impl ContractResult {
    pub fn to_external_verdict(&self) -> ExternalVerdict {
        if self.closed {
            ExternalVerdict::Safe
        } else if self
            .clauses
            .iter()
            .any(|c| c.error.as_deref().map(|e| e.contains("no evaluable")).unwrap_or(false))
        {
            ExternalVerdict::Unknown
        } else {
            ExternalVerdict::Unsafe
        }
    }
}

/// Evalúa una expresión sobre el estado (pre o post) usando `python3`.
/// Devuelve Ok(true/false) o Err con el motivo (incluye "no evaluable").
pub fn evaluate_expression(expression: &str, state: &serde_json::Value) -> Result<bool, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let state_json = serde_json::to_string(state).unwrap_or_else(|_| "{}".to_string());
    let script = format!(
        "import json, sys\nstate = json.loads('''{state_json}''')\n_scope = dict(state)\ntry:\n    _result = bool(eval({expr_json}, _scope))\nexcept Exception as _e:\n    print('NO_EVALUABLE: ' + str(_e))\n    sys.exit(0)\nprint('OK' if _result else 'NO')\n",
        state_json = state_json.replace("\\", "\\\\").replace("'''", "\\'\\'\\'"),
        expr_json = serde_json::to_string(expression).unwrap_or_else(|_| "\"\"".to_string()),
    );
    let mut child = Command::new("python3")
        .args(["-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("no evaluable: python3 no disponible: {}", e))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("no evaluable: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "no evaluable: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains("NO_EVALUABLE") {
        return Err(format!("no evaluable: {}", stdout.trim()));
    }
    Ok(stdout.trim() == "OK")
}

/// Verifica un contrato: evalúa requires sobre el estado pre y ensures sobre
/// el post. Sin LLM (determinista); la reparación la orquesta el harness.
pub fn check_contract(spec: &ContractSpec, pre: &serde_json::Value, post: &serde_json::Value) -> ContractResult {
    let mut clauses = Vec::new();
    let mut all_ok = true;
    for c in &spec.clauses {
        let state = if c.kind == "requires" { pre } else { post };
        let (satisfied, error) = match evaluate_expression(&c.expression, state) {
            Ok(v) => (v, None),
            Err(e) => (false, Some(e)),
        };
        if !satisfied {
            all_ok = false;
        }
        clauses.push(ClauseResult {
            expression: c.expression.clone(),
            satisfied,
            error,
        });
    }
    ContractResult {
        target: spec.target.clone(),
        closed: all_ok,
        clauses,
        repair_attempts: 0,
    }
}

/// Extrae los contratos de un bloque `contracts:` en Markdown/JSON dentro
/// del SPEC.md. Formato: bloque JSON entre fences ```json contracts ```.
pub fn parse_contracts_from_spec(spec_md: &str) -> Vec<ContractSpec> {
    let mut specs = Vec::new();
    for block in spec_md.split("```") {
        let trimmed = block.trim();
        let Some(rest) = trimmed.strip_prefix("json") else { continue };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(rest.trim()) else { continue };
        let Some(arr) = value.get("contracts").and_then(|v| v.as_array()) else { continue };
        for item in arr {
            let target = item
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let required = item.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
            let mut clauses = Vec::new();
            if let Some(requires) = item.get("requires").and_then(|v| v.as_array()) {
                for r in requires {
                    if let Some(e) = r.as_str() {
                        clauses.push(ContractClause { kind: "requires".to_string(), expression: e.to_string() });
                    }
                }
            }
            if let Some(ensures) = item.get("ensures").and_then(|v| v.as_array()) {
                for e in ensures {
                    if let Some(e) = e.as_str() {
                        clauses.push(ContractClause { kind: "ensures".to_string(), expression: e.to_string() });
                    }
                }
            }
            if !clauses.is_empty() {
                specs.push(ContractSpec { target, clauses, required });
            }
        }
    }
    specs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_expression_true() {
        let state = serde_json::json!({"x": 5, "result": 10});
        assert_eq!(evaluate_expression("result == x * 2", &state).unwrap(), true);
    }

    #[test]
    fn test_evaluate_expression_false() {
        let state = serde_json::json!({"x": 5, "result": 11});
        assert_eq!(evaluate_expression("result == x * 2", &state).unwrap(), false);
    }

    #[test]
    fn test_evaluate_expression_syntax_error_is_no_evaluable() {
        let state = serde_json::json!({});
        let err = evaluate_expression("result ===", &state).unwrap_err();
        assert!(err.contains("no evaluable"));
    }

    #[test]
    fn test_check_contract_closed() {
        let spec = ContractSpec {
            target: "add".to_string(),
            clauses: vec![
                ContractClause { kind: "requires".to_string(), expression: "x > 0".to_string() },
                ContractClause { kind: "ensures".to_string(), expression: "result == x * 2".to_string() },
            ],
            required: true,
        };
        let pre = serde_json::json!({"x": 5});
        let post = serde_json::json!({"x": 5, "result": 10});
        let r = check_contract(&spec, &pre, &post);
        assert!(r.closed);
        assert_eq!(r.to_external_verdict(), ExternalVerdict::Safe);
    }

    #[test]
    fn test_check_contract_violated() {
        let spec = ContractSpec {
            target: "add".to_string(),
            clauses: vec![ContractClause { kind: "ensures".to_string(), expression: "result == x * 2".to_string() }],
            required: true,
        };
        let pre = serde_json::json!({"x": 5});
        let post = serde_json::json!({"x": 5, "result": 11});
        let r = check_contract(&spec, &pre, &post);
        assert!(!r.closed);
        assert_eq!(r.to_external_verdict(), ExternalVerdict::Unsafe);
    }

    #[test]
    fn test_parse_contracts_from_spec() {
        let md = r#"# SPEC

```json
{"contracts": [{"target": "add", "required": true, "requires": ["x > 0"], "ensures": ["result == x * 2"]}]}
```
"#;
        let specs = parse_contracts_from_spec(md);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].target, "add");
        assert!(specs[0].required);
        assert_eq!(specs[0].clauses.len(), 2);
    }

    #[test]
    fn test_required_not_closed_maps_blocked_via_contracts() {
        // Integración con contracts.rs: un contrato requerido que no cierra
        // (Unsafe) debe producir BLOCKED, no un gap.
        let spec = ContractSpec {
            target: "t".to_string(),
            clauses: vec![ContractClause { kind: "ensures".to_string(), expression: "result == 1".to_string() }],
            required: true,
        };
        let r = check_contract(&spec, &serde_json::json!({}), &serde_json::json!({"result": 2}));
        let verdict = r.to_external_verdict().to_verdict(true);
        assert!(matches!(verdict, crate::verifier::Verdict::Blocked(_)));
    }
}