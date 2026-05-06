use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;

use crate::decision::Decision;
use crate::error::PolicyError;

/// RPCPolicy — commands, filesystem, resources.
pub struct RpcPolicy {
    allowed_commands: Vec<Regex>,
    denied_commands: Vec<Regex>,
    allowed_paths: Vec<PathBuf>,
    denied_paths: Vec<PathBuf>,
    pub max_output_bytes: u64,
    pub timeout_seconds: u32,
    /// Immutable deny patterns — cannot be overridden by policy configuration.
    /// These prevent catastrophic commands regardless of YAML allow rules.
    immutable_deny: Vec<String>,
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
    pub fn load(path: &Path) -> Result<Self, PolicyError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    /// Parse policy from YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self, PolicyError> {
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
            max_output_bytes: resources
                .and_then(|r| r.max_output_bytes)
                .unwrap_or(10_485_760),
            timeout_seconds: resources.and_then(|r| r.timeout_seconds).unwrap_or(300),
            immutable_deny: Self::default_immutable_deny(),
        })
    }

    /// Check if a command is allowed by policy.
    /// Immutable deny checked first (cannot be overridden).
    /// Then deny rules. Then allow rules. Default: deny.
    pub fn check_command(&self, cmd: &str) -> Decision {
        // Immutable deny — these can never be overridden by policy configuration
        if let Some(pattern) = self.is_immutably_denied(cmd) {
            return Decision::deny(
                format!("immutable deny: command contains '{pattern}'"),
                format!("immutable_deny:{pattern}"),
            );
        }

        // Deny rules always win over allow rules
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

    /// Returns the default set of immutable deny patterns.
    /// These cannot be removed or overridden by any policy file.
    fn default_immutable_deny() -> Vec<String> {
        vec![
            "rm -rf /".into(),
            "rm -rf --no-preserve-root".into(),
            "mkfs".into(),
            "dd if=/dev/zero".into(),
            ":(){ :|:& };:".into(),
            "> /dev/sda".into(),
            "chmod -R 777 /".into(),
        ]
    }

    /// Check if a command matches any immutable deny pattern.
    fn is_immutably_denied(&self, command: &str) -> Option<&str> {
        self.immutable_deny
            .iter()
            .find(|pattern| command.contains(pattern.as_str()))
            .map(|s| s.as_str())
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
fn compile_anchored(pattern: &str) -> Result<Regex, PolicyError> {
    let anchored = if pattern.starts_with('^') && pattern.ends_with('$') {
        pattern.to_string()
    } else if pattern.starts_with('^') {
        format!("{pattern}$")
    } else if pattern.ends_with('$') {
        format!("^{pattern}")
    } else {
        format!("^{pattern}$")
    };
    Regex::new(&anchored).map_err(|source| PolicyError::InvalidRegex {
        pattern: pattern.to_string(),
        source,
    })
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

    #[test]
    fn test_immutable_deny_rm_rf() {
        // Even if a policy explicitly allows "rm", immutable deny blocks "rm -rf /"
        let yaml = r#"
spec:
  commands:
    allow:
      - pattern: "^rm.*"
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        let decision = policy.check_command("rm -rf /");
        assert!(!decision.allowed);
        assert!(decision.matched_rule.contains("immutable_deny"));
    }

    #[test]
    fn test_immutable_deny_mkfs() {
        let yaml = r#"
spec:
  commands:
    allow:
      - pattern: ".*"
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        let decision = policy.check_command("mkfs.ext4 /dev/sda1");
        assert!(!decision.allowed);
        assert!(decision.matched_rule.contains("immutable_deny"));
    }

    #[test]
    fn test_immutable_deny_fork_bomb() {
        let yaml = r#"
spec:
  commands:
    allow:
      - pattern: ".*"
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        let decision = policy.check_command(":(){ :|:& };:");
        assert!(!decision.allowed);
        assert!(decision.matched_rule.contains("immutable_deny"));
    }

    #[test]
    fn test_immutable_deny_dd_zero() {
        let yaml = r#"
spec:
  commands:
    allow:
      - pattern: ".*"
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        let decision = policy.check_command("dd if=/dev/zero of=/dev/sda bs=1M");
        assert!(!decision.allowed);
        assert!(decision.matched_rule.contains("immutable_deny"));
    }

    #[test]
    fn test_immutable_deny_cannot_be_overridden() {
        // A policy that explicitly allows everything still can't override immutable deny
        let yaml = r#"
spec:
  commands:
    allow:
      - pattern: ".*"
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        // Normal commands still work
        assert!(policy.check_command("echo hello").allowed);
        assert!(policy.check_command("ls -la").allowed);
        // But immutable deny patterns are always blocked
        assert!(!policy.check_command("rm -rf /").allowed);
        assert!(!policy.check_command("chmod -R 777 /").allowed);
        assert!(!policy.check_command("> /dev/sda").allowed);
    }
}
