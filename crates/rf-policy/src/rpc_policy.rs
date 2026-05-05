use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;

use crate::decision::Decision;

/// RPCPolicy — commands, filesystem, resources.
pub struct RpcPolicy {
    allowed_commands: Vec<Regex>,
    denied_commands: Vec<Regex>,
    allowed_paths: Vec<PathBuf>,
    denied_paths: Vec<PathBuf>,
    pub max_output_bytes: u64,
    pub timeout_seconds: u32,
}

/// YAML config format for policy files.
#[derive(Debug, Deserialize)]
struct PolicyConfig {
    spec: PolicySpec,
}

#[derive(Debug, Deserialize)]
struct PolicySpec {
    commands: Option<CommandSpec>,
    filesystem: Option<FilesystemSpec>,
    resources: Option<ResourceSpec>,
}

#[derive(Debug, Deserialize)]
struct CommandSpec {
    allow: Option<Vec<PatternEntry>>,
    deny: Option<Vec<PatternEntry>>,
}

#[derive(Debug, Deserialize)]
struct PatternEntry {
    pattern: String,
}

#[derive(Debug, Deserialize)]
struct FilesystemSpec {
    allow: Option<Vec<PathEntry>>,
    deny: Option<Vec<PathEntry>>,
}

#[derive(Debug, Deserialize)]
struct PathEntry {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceSpec {
    max_output_bytes: Option<u64>,
    timeout_seconds: Option<u32>,
}

impl RpcPolicy {
    /// Load policy from a YAML file.
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    /// Parse policy from YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config: PolicyConfig = serde_yaml::from_str(yaml)?;
        let spec = config.spec;

        let allowed_commands = spec
            .commands
            .as_ref()
            .and_then(|c| c.allow.as_ref())
            .map(|patterns| {
                patterns
                    .iter()
                    .map(|p| compile_anchored(&p.pattern))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();

        let denied_commands = spec
            .commands
            .as_ref()
            .and_then(|c| c.deny.as_ref())
            .map(|patterns| {
                patterns
                    .iter()
                    .map(|p| compile_anchored(&p.pattern))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();

        let allowed_paths = spec
            .filesystem
            .as_ref()
            .and_then(|f| f.allow.as_ref())
            .map(|paths| paths.iter().map(|p| PathBuf::from(&p.path)).collect())
            .unwrap_or_default();

        let denied_paths = spec
            .filesystem
            .as_ref()
            .and_then(|f| f.deny.as_ref())
            .map(|paths| paths.iter().map(|p| PathBuf::from(&p.path)).collect())
            .unwrap_or_default();

        let resources = spec.resources.as_ref();

        Ok(Self {
            allowed_commands,
            denied_commands,
            allowed_paths,
            denied_paths,
            max_output_bytes: resources.and_then(|r| r.max_output_bytes).unwrap_or(10_485_760),
            timeout_seconds: resources.and_then(|r| r.timeout_seconds).unwrap_or(300),
        })
    }

    /// Check if a command is allowed by policy.
    /// Deny rules checked first (always win). Then allow rules. Default: deny.
    pub fn check_command(&self, cmd: &str) -> Decision {
        // Deny rules always win
        for re in &self.denied_commands {
            if re.is_match(cmd) {
                return Decision::deny(
                    format!("matches deny rule: {}", re.as_str()),
                    re.as_str().to_string(),
                );
            }
        }

        // Check allow rules
        for re in &self.allowed_commands {
            if re.is_match(cmd) {
                return Decision::allow(re.as_str().to_string());
            }
        }

        // Default: deny
        Decision::deny_default()
    }

    /// Check if a filesystem path is allowed by policy.
    pub fn check_path(&self, path: &Path) -> Decision {
        // Resolve symlinks to prevent traversal
        let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        // Deny rules first
        for denied in &self.denied_paths {
            if resolved.starts_with(denied) {
                return Decision::deny(
                    format!("path under denied prefix: {}", denied.display()),
                    denied.display().to_string(),
                );
            }
        }

        // Check allow rules
        for allowed in &self.allowed_paths {
            if resolved.starts_with(allowed) {
                return Decision::allow(allowed.display().to_string());
            }
        }

        Decision::deny_default()
    }
}

/// Compile a regex pattern, ensuring it is anchored (^...$).
fn compile_anchored(pattern: &str) -> Result<Regex, regex::Error> {
    let anchored = if pattern.starts_with('^') && pattern.ends_with('$') {
        pattern.to_string()
    } else if pattern.starts_with('^') {
        format!("{}$", pattern)
    } else if pattern.ends_with('$') {
        format!("^{}", pattern)
    } else {
        format!("^{}$", pattern)
    };
    Regex::new(&anchored)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_policy() -> RpcPolicy {
        let yaml = r#"
spec:
  commands:
    allow:
      - pattern: "^echo .*"
      - pattern: "^ls( .*)?$"
    deny:
      - pattern: ".*secret.*"
      - pattern: "^rm.*-rf"
  filesystem:
    allow:
      - path: /workspace
      - path: /tmp
    deny:
      - path: /etc/shadow
      - path: /root
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 60
"#;
        RpcPolicy::from_yaml(yaml).unwrap()
    }

    #[test]
    fn test_allowed_command() {
        let policy = test_policy();
        assert!(policy.check_command("echo hello").allowed);
        assert!(policy.check_command("ls -la").allowed);
        assert!(policy.check_command("ls").allowed);
    }

    #[test]
    fn test_denied_command() {
        let policy = test_policy();
        assert!(!policy.check_command("cat /etc/secret").allowed);
        assert!(!policy.check_command("rm -rf /").allowed);
    }

    #[test]
    fn test_default_deny() {
        let policy = test_policy();
        assert!(!policy.check_command("wget http://evil.com").allowed);
        assert!(!policy.check_command("curl http://evil.com").allowed);
    }

    #[test]
    fn test_deny_wins_over_allow() {
        let policy = test_policy();
        // "echo" is allowed, but "secret" in command triggers deny
        assert!(!policy.check_command("echo secret").allowed);
    }
}
