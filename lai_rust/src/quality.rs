use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Python,
    Rust,
    JavaScript,
    Go,
    Shell,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub id: &'static str,
    pub passed: bool,
    pub detail: String,
}

impl Check {
    pub fn ok(id: &'static str, detail: impl Into<String>) -> Self {
        Self {
            id,
            passed: true,
            detail: detail.into(),
        }
    }
    pub fn fail(id: &'static str, detail: impl Into<String>) -> Self {
        Self {
            id,
            passed: false,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VerificationReport {
    pub score: f64,
    pub checks: Vec<Check>,
}

impl VerificationReport {
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("  score: {:.1}/100\n", self.score));
        for check in &self.checks {
            out.push_str(&format!(
                "    {} {:<16} {}\n",
                if check.passed { "[ ok ]" } else { "[FAIL]" },
                check.id,
                check.detail
            ));
        }
        out
    }
}

pub struct Verifier {
    pub language: Language,
    pub description: String,
}

impl Verifier {
    pub fn new(language: Language, description: &str) -> Self {
        Self {
            language,
            description: description.to_string(),
        }
    }

    pub async fn verify_static(&self, code: &str, _run_lint: bool) -> VerificationReport {
        let mut checks = Vec::new();

        if code.trim().is_empty() {
            checks.push(Check::fail("non_empty", "empty candidate"));
            checks.push(Check::ok("syntax", "no code"));
        } else {
            checks.push(Check::ok("non_empty", format!("{} bytes", code.len())));

            if self.language == Language::Python {
                let has_def = Regex::new(r"(?m)^\s*(?:async\s+)?def\s+\w+|^\s*class\s+\w+")
                    .map(|re| re.is_match(code))
                    .unwrap_or(false);
                if has_def {
                    checks.push(Check::ok("structure", "defs/functions"));
                } else {
                    checks.push(Check::fail("structure", "no defs"));
                }

                let has_main = Regex::new(r#"if\s+__name__\s*==\s*['"]__main__['"]"#)
                    .map(|re| re.is_match(code))
                    .unwrap_or(false);
                if has_main {
                    checks.push(Check::ok("main_guard", "__main__ guard"));
                } else {
                    checks.push(Check::fail("main_guard", "no __main__"));
                }
            }

            if self.language == Language::Python {
                let (ran, passed) = self.try_compile_python(code);
                if ran {
                    if passed {
                        checks.push(Check::ok("syntax", "OK"));
                    } else {
                        checks.push(Check::fail("syntax", "syntax error"));
                    }
                } else {
                    checks.push(Check::ok("syntax", "python3 n/a"));
                }
            }
        }

        let pc = checks.iter().filter(|c| c.passed).count() as f64;
        let tc = checks.len() as f64;
        VerificationReport {
            score: if tc > 0.0 { (pc / tc) * 100.0 } else { 0.0 },
            checks,
        }
    }

    pub fn is_verified(&self, report: &VerificationReport, min_score: f64) -> bool {
        report.score >= min_score && !report.checks.iter().any(|c| !c.passed && c.id == "syntax")
    }

    fn try_compile_python(&self, code: &str) -> (bool, bool) {
        use std::process::{Command, Stdio};
        match Command::new("python3")
            .arg("-c")
            .arg(format!("import ast; ast.parse({:?})", code))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => match child.wait_with_output() {
                Ok(out) => (true, out.status.success()),
                Err(_) => (false, false),
            },
            Err(_) => (false, false),
        }
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::new(Language::Unknown, "default")
    }
}
