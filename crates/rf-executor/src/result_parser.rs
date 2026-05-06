//! Result parsing and assertion engine.
//!
//! Parses command output in various formats (JSON, YAML, CSV, regex, key-value)
//! and evaluates assertions against the parsed data.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::warn;

/// Output format to parse.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Json,
    Yaml,
    Csv,
    Regex,
    KeyValue,
    Lines,
    Raw,
}

/// An assertion to evaluate against parsed output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Assertion {
    /// JSON pointer or key path to extract (e.g., "/status" or "cpu.usage").
    pub path: String,
    /// The comparison operator.
    pub op: AssertOp,
    /// Expected value (string representation).
    pub value: String,
}

/// Assertion operator.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AssertOp {
    /// Equals (string comparison).
    Eq,
    /// Not equals.
    Ne,
    /// Contains (substring).
    Contains,
    /// Does not contain.
    NotContains,
    /// Matches regex pattern.
    Matches,
    /// Greater than (numeric).
    Gt,
    /// Less than (numeric).
    Lt,
    /// Greater than or equal (numeric).
    Gte,
    /// Less than or equal (numeric).
    Lte,
    /// Value is present (non-null, non-empty).
    Exists,
}

/// Result of evaluating one assertion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssertionResult {
    pub assertion: Assertion,
    pub passed: bool,
    pub actual_value: Option<String>,
    pub message: String,
}

/// Overall result of parsing and asserting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParseResult {
    pub format: OutputFormat,
    pub parsed_fields: HashMap<String, String>,
    pub assertions: Vec<AssertionResult>,
}

impl ParseResult {
    /// Returns true if all assertions passed.
    pub fn all_passed(&self) -> bool {
        self.assertions.iter().all(|a| a.passed)
    }

    /// Count of failed assertions.
    pub fn failure_count(&self) -> usize {
        self.assertions.iter().filter(|a| !a.passed).count()
    }
}

/// Parse command output and evaluate assertions.
pub fn parse_and_assert(
    output: &str,
    format: OutputFormat,
    assertions: &[Assertion],
) -> ParseResult {
    let parsed_fields = parse_output(output, format);

    let assertion_results: Vec<AssertionResult> = assertions
        .iter()
        .map(|a| evaluate_assertion(a, &parsed_fields))
        .collect();

    ParseResult {
        format,
        parsed_fields,
        assertions: assertion_results,
    }
}

/// Parse output into key-value fields based on format.
pub fn parse_output(output: &str, format: OutputFormat) -> HashMap<String, String> {
    match format {
        OutputFormat::Json => parse_json(output),
        OutputFormat::Yaml => parse_yaml(output),
        OutputFormat::Csv => parse_csv(output),
        OutputFormat::Regex => HashMap::new(), // Regex extraction uses assertions directly
        OutputFormat::KeyValue => parse_key_value(output),
        OutputFormat::Lines => parse_lines(output),
        OutputFormat::Raw => {
            let mut m = HashMap::new();
            m.insert("raw".to_string(), output.to_string());
            m
        }
    }
}

/// Parse JSON output, flattening nested objects with dot notation.
fn parse_json(output: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    match serde_json::from_str::<serde_json::Value>(output) {
        Ok(value) => {
            flatten_json("", &value, &mut fields);
        }
        Err(e) => {
            warn!(error = %e, "failed to parse JSON output");
            fields.insert("_error".to_string(), e.to_string());
        }
    }
    fields
}

/// Recursively flatten a JSON value into dot-separated keys.
fn flatten_json(prefix: &str, value: &serde_json::Value, out: &mut HashMap<String, String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_json(&key, v, out);
            }
        }
        serde_json::Value::Array(arr) => {
            out.insert(
                if prefix.is_empty() {
                    "_length".to_string()
                } else {
                    format!("{prefix}._length")
                },
                arr.len().to_string(),
            );
            for (i, v) in arr.iter().enumerate() {
                let key = if prefix.is_empty() {
                    format!("{i}")
                } else {
                    format!("{prefix}.{i}")
                };
                flatten_json(&key, v, out);
            }
        }
        serde_json::Value::String(s) => {
            out.insert(prefix.to_string(), s.clone());
        }
        serde_json::Value::Number(n) => {
            out.insert(prefix.to_string(), n.to_string());
        }
        serde_json::Value::Bool(b) => {
            out.insert(prefix.to_string(), b.to_string());
        }
        serde_json::Value::Null => {
            out.insert(prefix.to_string(), "null".to_string());
        }
    }
}

/// Parse YAML output into flat key-value pairs.
fn parse_yaml(output: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    match serde_yaml::from_str::<serde_yaml::Value>(output) {
        Ok(value) => {
            flatten_yaml("", &value, &mut fields);
        }
        Err(e) => {
            warn!(error = %e, "failed to parse YAML output");
            fields.insert("_error".to_string(), e.to_string());
        }
    }
    fields
}

/// Recursively flatten a YAML value.
fn flatten_yaml(prefix: &str, value: &serde_yaml::Value, out: &mut HashMap<String, String>) {
    match value {
        serde_yaml::Value::Mapping(map) => {
            for (k, v) in map {
                let key_str = match k {
                    serde_yaml::Value::String(s) => s.clone(),
                    other => format!("{other:?}"),
                };
                let key = if prefix.is_empty() {
                    key_str
                } else {
                    format!("{prefix}.{key_str}")
                };
                flatten_yaml(&key, v, out);
            }
        }
        serde_yaml::Value::Sequence(seq) => {
            out.insert(
                if prefix.is_empty() {
                    "_length".to_string()
                } else {
                    format!("{prefix}._length")
                },
                seq.len().to_string(),
            );
            for (i, v) in seq.iter().enumerate() {
                let key = if prefix.is_empty() {
                    format!("{i}")
                } else {
                    format!("{prefix}.{i}")
                };
                flatten_yaml(&key, v, out);
            }
        }
        serde_yaml::Value::String(s) => {
            out.insert(prefix.to_string(), s.clone());
        }
        serde_yaml::Value::Number(n) => {
            out.insert(prefix.to_string(), format!("{n}"));
        }
        serde_yaml::Value::Bool(b) => {
            out.insert(prefix.to_string(), b.to_string());
        }
        serde_yaml::Value::Null => {
            out.insert(prefix.to_string(), "null".to_string());
        }
        serde_yaml::Value::Tagged(tagged) => {
            flatten_yaml(prefix, &tagged.value, out);
        }
    }
}

/// Parse CSV output (first line = headers, subsequent lines = values).
fn parse_csv(output: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return fields;
    }

    let headers: Vec<&str> = lines[0].split(',').map(str::trim).collect();
    fields.insert("_rows".to_string(), (lines.len() - 1).to_string());
    fields.insert("_columns".to_string(), headers.len().to_string());

    for (row_idx, line) in lines[1..].iter().enumerate() {
        let values: Vec<&str> = line.split(',').map(str::trim).collect();
        for (col_idx, value) in values.iter().enumerate() {
            if col_idx < headers.len() {
                let key = format!("{}.{}", row_idx, headers[col_idx]);
                fields.insert(key, (*value).to_string());
            }
        }
    }

    fields
}

/// Parse key=value format (one per line).
fn parse_key_value(output: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    fields
}

/// Parse output as numbered lines.
fn parse_lines(output: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let lines: Vec<&str> = output.lines().collect();
    fields.insert("_count".to_string(), lines.len().to_string());
    for (i, line) in lines.iter().enumerate() {
        fields.insert(i.to_string(), (*line).to_string());
    }
    fields
}

/// Evaluate a single assertion against parsed fields.
fn evaluate_assertion(assertion: &Assertion, fields: &HashMap<String, String>) -> AssertionResult {
    let actual = fields.get(&assertion.path);

    match assertion.op {
        AssertOp::Exists => {
            let exists = actual.is_some() && actual != Some(&"null".to_string());
            AssertionResult {
                assertion: assertion.clone(),
                passed: exists,
                actual_value: actual.cloned(),
                message: if exists {
                    format!("'{}' exists", assertion.path)
                } else {
                    format!("'{}' does not exist", assertion.path)
                },
            }
        }
        _ => {
            let Some(actual_val) = actual else {
                return AssertionResult {
                    assertion: assertion.clone(),
                    passed: false,
                    actual_value: None,
                    message: format!("path '{}' not found in output", assertion.path),
                };
            };

            let passed = match assertion.op {
                AssertOp::Eq => actual_val == &assertion.value,
                AssertOp::Ne => actual_val != &assertion.value,
                AssertOp::Contains => actual_val.contains(&assertion.value),
                AssertOp::NotContains => !actual_val.contains(&assertion.value),
                AssertOp::Matches => {
                    regex::Regex::new(&assertion.value)
                        .map(|re| re.is_match(actual_val))
                        .unwrap_or(false)
                }
                AssertOp::Gt => numeric_cmp(actual_val, &assertion.value, |a, b| a > b),
                AssertOp::Lt => numeric_cmp(actual_val, &assertion.value, |a, b| a < b),
                AssertOp::Gte => numeric_cmp(actual_val, &assertion.value, |a, b| a >= b),
                AssertOp::Lte => numeric_cmp(actual_val, &assertion.value, |a, b| a <= b),
                AssertOp::Exists => unreachable!(),
            };

            AssertionResult {
                assertion: assertion.clone(),
                passed,
                actual_value: Some(actual_val.clone()),
                message: if passed {
                    format!(
                        "'{}' {} '{}' (actual: '{}')",
                        assertion.path,
                        op_symbol(assertion.op),
                        assertion.value,
                        actual_val
                    )
                } else {
                    format!(
                        "FAILED: '{}' {} '{}' (actual: '{}')",
                        assertion.path,
                        op_symbol(assertion.op),
                        assertion.value,
                        actual_val
                    )
                },
            }
        }
    }
}

/// Compare two values numerically.
fn numeric_cmp(actual: &str, expected: &str, cmp: fn(f64, f64) -> bool) -> bool {
    match (actual.parse::<f64>(), expected.parse::<f64>()) {
        (Ok(a), Ok(b)) => cmp(a, b),
        _ => false,
    }
}

/// Human-readable operator symbol.
fn op_symbol(op: AssertOp) -> &'static str {
    match op {
        AssertOp::Eq => "==",
        AssertOp::Ne => "!=",
        AssertOp::Contains => "contains",
        AssertOp::NotContains => "!contains",
        AssertOp::Matches => "=~",
        AssertOp::Gt => ">",
        AssertOp::Lt => "<",
        AssertOp::Gte => ">=",
        AssertOp::Lte => "<=",
        AssertOp::Exists => "exists",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_flat() {
        let output = r#"{"status": "ok", "code": 200, "message": "healthy"}"#;
        let fields = parse_output(output, OutputFormat::Json);
        assert_eq!(fields.get("status").unwrap(), "ok");
        assert_eq!(fields.get("code").unwrap(), "200");
        assert_eq!(fields.get("message").unwrap(), "healthy");
    }

    #[test]
    fn test_parse_json_nested() {
        let output = r#"{"server": {"cpu": 45.2, "memory": 78.1}, "uptime": 3600}"#;
        let fields = parse_output(output, OutputFormat::Json);
        assert_eq!(fields.get("server.cpu").unwrap(), "45.2");
        assert_eq!(fields.get("server.memory").unwrap(), "78.1");
        assert_eq!(fields.get("uptime").unwrap(), "3600");
    }

    #[test]
    fn test_parse_json_array() {
        let output = r#"{"items": ["a", "b", "c"]}"#;
        let fields = parse_output(output, OutputFormat::Json);
        assert_eq!(fields.get("items._length").unwrap(), "3");
        assert_eq!(fields.get("items.0").unwrap(), "a");
        assert_eq!(fields.get("items.2").unwrap(), "c");
    }

    #[test]
    fn test_parse_yaml() {
        let output = "status: running\ncpu: 23.5\nmemory: 512\n";
        let fields = parse_output(output, OutputFormat::Yaml);
        assert_eq!(fields.get("status").unwrap(), "running");
        assert_eq!(fields.get("cpu").unwrap(), "23.5");
        assert_eq!(fields.get("memory").unwrap(), "512");
    }

    #[test]
    fn test_parse_csv() {
        let output = "name,status,pid\nnginx,running,1234\npostgres,running,5678\n";
        let fields = parse_output(output, OutputFormat::Csv);
        assert_eq!(fields.get("_rows").unwrap(), "2");
        assert_eq!(fields.get("_columns").unwrap(), "3");
        assert_eq!(fields.get("0.name").unwrap(), "nginx");
        assert_eq!(fields.get("0.status").unwrap(), "running");
        assert_eq!(fields.get("1.name").unwrap(), "postgres");
        assert_eq!(fields.get("1.pid").unwrap(), "5678");
    }

    #[test]
    fn test_parse_key_value() {
        let output = "VERSION=1.24.0\nSTATUS=active\n# comment\nUPTIME=3600\n";
        let fields = parse_output(output, OutputFormat::KeyValue);
        assert_eq!(fields.get("VERSION").unwrap(), "1.24.0");
        assert_eq!(fields.get("STATUS").unwrap(), "active");
        assert_eq!(fields.get("UPTIME").unwrap(), "3600");
        assert!(!fields.contains_key("# comment"));
    }

    #[test]
    fn test_parse_lines() {
        let output = "line one\nline two\nline three\n";
        let fields = parse_output(output, OutputFormat::Lines);
        assert_eq!(fields.get("_count").unwrap(), "3");
        assert_eq!(fields.get("0").unwrap(), "line one");
        assert_eq!(fields.get("2").unwrap(), "line three");
    }

    #[test]
    fn test_assertion_eq() {
        let output = r#"{"status": "ok"}"#;
        let result = parse_and_assert(
            output,
            OutputFormat::Json,
            &[Assertion {
                path: "status".into(),
                op: AssertOp::Eq,
                value: "ok".into(),
            }],
        );
        assert!(result.all_passed());
    }

    #[test]
    fn test_assertion_ne() {
        let output = r#"{"status": "error"}"#;
        let result = parse_and_assert(
            output,
            OutputFormat::Json,
            &[Assertion {
                path: "status".into(),
                op: AssertOp::Ne,
                value: "ok".into(),
            }],
        );
        assert!(result.all_passed());
    }

    #[test]
    fn test_assertion_numeric_gt() {
        let output = r#"{"cpu": 85.5}"#;
        let result = parse_and_assert(
            output,
            OutputFormat::Json,
            &[Assertion {
                path: "cpu".into(),
                op: AssertOp::Gt,
                value: "80".into(),
            }],
        );
        assert!(result.all_passed());
    }

    #[test]
    fn test_assertion_numeric_lt_fails() {
        let output = r#"{"cpu": 85.5}"#;
        let result = parse_and_assert(
            output,
            OutputFormat::Json,
            &[Assertion {
                path: "cpu".into(),
                op: AssertOp::Lt,
                value: "80".into(),
            }],
        );
        assert!(!result.all_passed());
        assert_eq!(result.failure_count(), 1);
    }

    #[test]
    fn test_assertion_contains() {
        let output = r#"{"message": "server is healthy and running"}"#;
        let result = parse_and_assert(
            output,
            OutputFormat::Json,
            &[Assertion {
                path: "message".into(),
                op: AssertOp::Contains,
                value: "healthy".into(),
            }],
        );
        assert!(result.all_passed());
    }

    #[test]
    fn test_assertion_matches_regex() {
        let output = r#"{"version": "v1.24.3"}"#;
        let result = parse_and_assert(
            output,
            OutputFormat::Json,
            &[Assertion {
                path: "version".into(),
                op: AssertOp::Matches,
                value: r"^v\d+\.\d+\.\d+$".into(),
            }],
        );
        assert!(result.all_passed());
    }

    #[test]
    fn test_assertion_exists() {
        let output = r#"{"name": "web-01", "tags": null}"#;
        let result = parse_and_assert(
            output,
            OutputFormat::Json,
            &[
                Assertion {
                    path: "name".into(),
                    op: AssertOp::Exists,
                    value: String::new(),
                },
                Assertion {
                    path: "tags".into(),
                    op: AssertOp::Exists,
                    value: String::new(),
                },
            ],
        );
        // "name" exists, "tags" is null → does not "exist"
        assert!(!result.all_passed());
        assert!(result.assertions[0].passed);
        assert!(!result.assertions[1].passed);
    }

    #[test]
    fn test_assertion_path_not_found() {
        let output = r#"{"status": "ok"}"#;
        let result = parse_and_assert(
            output,
            OutputFormat::Json,
            &[Assertion {
                path: "nonexistent".into(),
                op: AssertOp::Eq,
                value: "whatever".into(),
            }],
        );
        assert!(!result.all_passed());
        assert!(result.assertions[0].message.contains("not found"));
    }

    #[test]
    fn test_multiple_assertions_mixed() {
        let output = r#"{"cpu": 45.2, "status": "running", "uptime": 7200}"#;
        let result = parse_and_assert(
            output,
            OutputFormat::Json,
            &[
                Assertion {
                    path: "cpu".into(),
                    op: AssertOp::Lt,
                    value: "90".into(),
                },
                Assertion {
                    path: "status".into(),
                    op: AssertOp::Eq,
                    value: "running".into(),
                },
                Assertion {
                    path: "uptime".into(),
                    op: AssertOp::Gte,
                    value: "3600".into(),
                },
            ],
        );
        assert!(result.all_passed());
        assert_eq!(result.failure_count(), 0);
    }

    #[test]
    fn test_invalid_json_produces_error() {
        let output = "not json at all";
        let fields = parse_output(output, OutputFormat::Json);
        assert!(fields.contains_key("_error"));
    }

    #[test]
    fn test_parse_result_serialization() {
        let result = ParseResult {
            format: OutputFormat::Json,
            parsed_fields: HashMap::from([("status".into(), "ok".into())]),
            assertions: vec![AssertionResult {
                assertion: Assertion {
                    path: "status".into(),
                    op: AssertOp::Eq,
                    value: "ok".into(),
                },
                passed: true,
                actual_value: Some("ok".into()),
                message: "passed".into(),
            }],
        };

        let json = serde_json::to_string(&result).unwrap();
        let deser: ParseResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, deser);
    }
}
