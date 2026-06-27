//! Built-in policy templates for common AI agent roles.
//!
//! These templates provide vetted, secure-by-default policy configurations
//! for common use cases. They can be used directly or composed together
//! (deny-wins conflict resolution).

use crate::error::PolicyError;

/// A named policy template with metadata.
#[derive(Debug, Clone)]
pub struct PolicyTemplate {
    /// Template identifier (e.g., "coding-assistant")
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// The YAML policy content
    pub yaml: String,
    /// Template category
    pub category: TemplateCategory,
}

/// Categories of policy templates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateCategory {
    /// AI coding assistants (Claude Code, Cursor, Aider)
    CodingAssistant,
    /// Production read-only operations
    ProductionReadOnly,
    /// Security investigation and auditing
    SecurityInvestigator,
    /// CI/CD pipeline agents
    CiCdAgent,
    /// Database query agents
    DatabaseQuery,
    /// Productized AI guardrails (drop-in security for AI agents)
    ProductizeAI,
}

impl std::fmt::Display for TemplateCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CodingAssistant => write!(f, "coding-assistant"),
            Self::ProductionReadOnly => write!(f, "production-read-only"),
            Self::SecurityInvestigator => write!(f, "security-investigator"),
            Self::CiCdAgent => write!(f, "ci-cd-agent"),
            Self::DatabaseQuery => write!(f, "database-query"),
            Self::ProductizeAI => write!(f, "productize-ai"),
        }
    }
}

/// Registry of all built-in policy templates.
pub struct TemplateRegistry {
    templates: Vec<PolicyTemplate>,
}

impl TemplateRegistry {
    /// Create a registry with all built-in templates.
    pub fn new() -> Self {
        Self {
            templates: vec![
                coding_assistant_template(),
                production_read_only_template(),
                security_investigator_template(),
                ci_cd_agent_template(),
                database_query_template(),
                safe_dev_mode_template(),
                production_ai_guardrails_template(),
                read_only_infrastructure_ai_template(),
            ],
        }
    }

    /// Get a template by name.
    pub fn get(&self, name: &str) -> Option<&PolicyTemplate> {
        self.templates.iter().find(|t| t.name == name)
    }

    /// List all available templates.
    pub fn list(&self) -> &[PolicyTemplate] {
        &self.templates
    }

    /// Get templates by category.
    pub fn by_category(&self, category: &TemplateCategory) -> Vec<&PolicyTemplate> {
        self.templates
            .iter()
            .filter(|t| &t.category == category)
            .collect()
    }

    /// Validate that a YAML string is a valid policy template.
    pub fn validate_yaml(yaml: &str) -> Result<(), PolicyError> {
        // Parse as YAML to check structure
        let _: serde_yaml::Value =
            serde_yaml::from_str(yaml).map_err(|e| PolicyError::Validation(e.to_string()))?;
        Ok(())
    }

    /// Compose multiple templates together with deny-wins semantics.
    ///
    /// When templates conflict:
    /// - Deny rules from ALL templates are merged (union)
    /// - Allow rules are intersected (only commands allowed by ALL templates pass)
    /// - Resource limits use the most restrictive value
    pub fn compose(templates: &[&PolicyTemplate]) -> Result<String, PolicyError> {
        if templates.is_empty() {
            return Err(PolicyError::Validation(
                "cannot compose empty template list".into(),
            ));
        }

        let mut all_deny_commands: Vec<String> = Vec::new();
        let mut all_allow_commands: Vec<String> = Vec::new();
        let mut all_deny_paths: Vec<String> = Vec::new();
        let mut all_allow_paths: Vec<String> = Vec::new();
        let mut min_output_bytes: u64 = u64::MAX;
        let mut min_timeout: u32 = u32::MAX;

        for template in templates {
            let value: serde_yaml::Value = serde_yaml::from_str(&template.yaml)
                .map_err(|e| PolicyError::Validation(e.to_string()))?;

            if let Some(spec) = value.get("spec") {
                // Collect deny commands (union)
                if let Some(commands) = spec.get("commands") {
                    if let Some(deny) = commands.get("deny") {
                        if let Some(arr) = deny.as_sequence() {
                            for item in arr {
                                if let Some(pattern) = item.get("pattern").and_then(|p| p.as_str())
                                {
                                    if !all_deny_commands.contains(&pattern.to_string()) {
                                        all_deny_commands.push(pattern.to_string());
                                    }
                                }
                            }
                        }
                    }
                    if let Some(allow) = commands.get("allow") {
                        if let Some(arr) = allow.as_sequence() {
                            for item in arr {
                                if let Some(pattern) = item.get("pattern").and_then(|p| p.as_str())
                                {
                                    all_allow_commands.push(pattern.to_string());
                                }
                            }
                        }
                    }
                }

                // Collect deny paths (union)
                if let Some(filesystem) = spec.get("filesystem") {
                    if let Some(deny) = filesystem.get("deny") {
                        if let Some(arr) = deny.as_sequence() {
                            for item in arr {
                                if let Some(path) = item.get("path").and_then(|p| p.as_str()) {
                                    if !all_deny_paths.contains(&path.to_string()) {
                                        all_deny_paths.push(path.to_string());
                                    }
                                }
                            }
                        }
                    }
                    if let Some(allow) = filesystem.get("allow") {
                        if let Some(arr) = allow.as_sequence() {
                            for item in arr {
                                if let Some(path) = item.get("path").and_then(|p| p.as_str()) {
                                    all_allow_paths.push(path.to_string());
                                }
                            }
                        }
                    }
                }

                // Resources: most restrictive wins
                if let Some(resources) = spec.get("resources") {
                    if let Some(bytes) = resources.get("maxOutputBytes").and_then(|v| v.as_u64()) {
                        min_output_bytes = min_output_bytes.min(bytes);
                    }
                    if let Some(timeout) = resources.get("timeoutSeconds").and_then(|v| v.as_u64())
                    {
                        min_timeout = min_timeout.min(timeout as u32);
                    }
                }
            }
        }

        // Build composed YAML
        let mut result = String::from("spec:\n");

        if !all_allow_commands.is_empty() || !all_deny_commands.is_empty() {
            result.push_str("  commands:\n");
            if !all_allow_commands.is_empty() {
                result.push_str("    allow:\n");
                for pattern in &all_allow_commands {
                    result.push_str(&format!("      - pattern: \"{pattern}\"\n"));
                }
            }
            if !all_deny_commands.is_empty() {
                result.push_str("    deny:\n");
                for pattern in &all_deny_commands {
                    result.push_str(&format!("      - pattern: \"{pattern}\"\n"));
                }
            }
        }

        if !all_allow_paths.is_empty() || !all_deny_paths.is_empty() {
            result.push_str("  filesystem:\n");
            if !all_allow_paths.is_empty() {
                result.push_str("    allow:\n");
                for path in &all_allow_paths {
                    result.push_str(&format!("      - path: {path}\n"));
                }
            }
            if !all_deny_paths.is_empty() {
                result.push_str("    deny:\n");
                for path in &all_deny_paths {
                    result.push_str(&format!("      - path: {path}\n"));
                }
            }
        }

        if min_output_bytes < u64::MAX || min_timeout < u32::MAX {
            result.push_str("  resources:\n");
            if min_output_bytes < u64::MAX {
                result.push_str(&format!("    maxOutputBytes: {min_output_bytes}\n"));
            }
            if min_timeout < u32::MAX {
                result.push_str(&format!("    timeoutSeconds: {min_timeout}\n"));
            }
        }

        Ok(result)
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Coding assistant template: filesystem read/write in project dir, git, package managers, test runners.
fn coding_assistant_template() -> PolicyTemplate {
    PolicyTemplate {
        name: "coding-assistant".into(),
        description: "AI coding assistants (Claude Code, Cursor, Aider). Allows file operations in project directory, git, package managers, test runners. Denies network mutation, credential access, system modification.".into(),
        category: TemplateCategory::CodingAssistant,
        yaml: r#"spec:
  commands:
    allow:
      - pattern: "^git (status|log|diff|add|commit|branch|checkout|stash|show|blame|rev-parse).*"
      - pattern: "^git push( .*)?$"
      - pattern: "^(npm|yarn|pnpm) (install|ci|test|run|exec|build|lint).*"
      - pattern: "^cargo (build|test|clippy|fmt|run|check|doc).*"
      - pattern: "^(python|python3|pip|pip3|uv) .*"
      - pattern: "^(cat|head|tail|less|wc|grep|find|ls|tree|file|stat) .*"
      - pattern: "^(mkdir|cp|mv|rm|touch|chmod) .*"
      - pattern: "^(make|cmake|go|rustc|gcc|clang) .*"
      - pattern: "^(docker|podman) (build|run|ps|logs|exec|compose).*"
    deny:
      - pattern: ".*rm.*-rf.*/.*"
      - pattern: "^(curl|wget|nc|ncat|socat) .*"
      - pattern: "^sudo .*"
      - pattern: ".*(ssh|scp|rsync|sftp) .*"
      - pattern: ".*(\\.ssh|credentials|secret|token|password|api.key).*"
      - pattern: "^(iptables|ufw|firewall-cmd|netfilter) .*"
      - pattern: "^(systemctl|service) (start|stop|restart|enable|disable) .*"
      - pattern: "^(useradd|userdel|passwd|chown) .*"
  filesystem:
    allow:
      - path: .
    deny:
      - path: /etc
      - path: /var
      - path: /usr
      - path: /root
      - path: ~/.ssh
      - path: ~/.aws
      - path: ~/.config/gh
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300"#
            .into(),
    }
}

/// Production read-only template: allow query/status commands, deny all writes.
fn production_read_only_template() -> PolicyTemplate {
    PolicyTemplate {
        name: "production-read-only".into(),
        description: "Production read-only operations. Allows status checks, log reading, metric queries. Denies all writes, destructive operations, and configuration changes.".into(),
        category: TemplateCategory::ProductionReadOnly,
        yaml: r#"spec:
  commands:
    allow:
      - pattern: "^systemctl status .*"
      - pattern: "^journalctl.*"
      - pattern: "^cat /var/log/.*"
      - pattern: "^tail (-[0-9]+f?|--follow|-f) /var/log/.*"
      - pattern: "^kubectl get .*"
      - pattern: "^kubectl describe .*"
      - pattern: "^kubectl logs .*"
      - pattern: "^docker (ps|logs|inspect|stats).*"
      - pattern: "^(df|du|free|top|uptime|w|who|last|ps aux).*"
      - pattern: "^(ip addr|ip route|ss -tlnp|netstat -tlnp).*"
      - pattern: "^curl -s (localhost|127\\.0\\.0\\.1).*"
    deny:
      - pattern: "^(rm|mv|cp|mkdir|touch|chmod|chown) .*"
      - pattern: "^(systemctl|service) (start|stop|restart|enable|disable) .*"
      - pattern: "^kubectl (delete|apply|patch|edit|scale|rollout) .*"
      - pattern: "^docker (rm|rmi|stop|kill|exec) .*"
      - pattern: "^(apt|yum|dnf|pacman|pip|npm|cargo) (install|remove|update|upgrade) .*"
      - pattern: "^sudo .*"
      - pattern: "^(dd|mkfs|fdisk|parted) .*"
      - pattern: ".*> .*"
      - pattern: ".*>> .*"
      - pattern: ".*tee .*"
  filesystem:
    allow:
      - path: /var/log
      - path: /tmp
    deny:
      - path: /etc
      - path: /root
      - path: ~/.ssh
  resources:
    maxOutputBytes: 5242880
    timeoutSeconds: 60"#
            .into(),
    }
}

/// Security investigator template: broad read access, deny writes and exfiltration.
fn security_investigator_template() -> PolicyTemplate {
    PolicyTemplate {
        name: "security-investigator".into(),
        description: "Security investigation and auditing. Broad read access for forensics, log analysis, process inspection. Denies writes, exfiltration, and credential access requires approval.".into(),
        category: TemplateCategory::SecurityInvestigator,
        yaml: r#"spec:
  commands:
    allow:
      - pattern: "^(cat|head|tail|less|strings|hexdump|xxd|file) .*"
      - pattern: "^(find|locate|grep|awk|sed|sort|uniq|wc) .*"
      - pattern: "^(ps|top|lsof|strace|ltrace|ss|netstat) .*"
      - pattern: "^(last|lastlog|who|w|id|groups) .*"
      - pattern: "^journalctl.*"
      - pattern: "^(ausearch|aureport|auditctl -l).*"
      - pattern: "^(ip addr|ip route|ip neigh|arp|nmap -sn).*"
      - pattern: "^(sha256sum|md5sum|stat|lsattr|getfacl) .*"
      - pattern: "^(docker inspect|docker logs|docker history) .*"
      - pattern: "^kubectl (get|describe|logs) .*"
      - pattern: "^(systemctl status|systemctl list-units) .*"
      - pattern: "^(dpkg -l|rpm -qa|pip list|npm list) .*"
      # Non-exec action allow rules (synthetic commands checked by policy engine)
      - pattern: "^port-forward [0-9.]+:[0-9]+ [0-9a-zA-Z._-]+:[0-9]+$"
      - pattern: "^remote-forward [0-9.]+:[0-9]+ [0-9a-zA-Z._-]+:[0-9]+$"
      - pattern: "^socks5-forward (127\\.0\\.0\\.1|localhost):[0-9]+$"
      - pattern: "^proxy [0-9a-zA-Z._-]+:[0-9]+$"
    deny:
      - pattern: "^(rm|mv|cp|mkdir|touch|chmod|chown|dd|mkfs) .*"
      - pattern: "^(curl|wget|nc|ncat|socat) .*[^localhost].*"
      - pattern: "^(scp|rsync|sftp|ftp) .*"
      - pattern: "^(systemctl|service) (start|stop|restart|enable|disable) .*"
      - pattern: "^(kill|killall|pkill) .*"
      - pattern: "^sudo .*"
      - pattern: "^(python|perl|ruby|bash|sh)( -c .*)?$"
      - pattern: "^/bin/(bash|sh)( .*)?$"
      - pattern: "^/usr/bin/(bash|sh)( .*)?$"
  filesystem:
    allow:
      - path: /var/log
      - path: /tmp
      - path: /proc
      - path: /sys
    deny:
      - path: ~/.ssh
      - path: ~/.aws
      - path: ~/.gnupg
      - path: /etc/shadow
      - path: /etc/sudoers
  resources:
    maxOutputBytes: 52428800
    timeoutSeconds: 120"#
            .into(),
    }
}

/// CI/CD agent template: build/test/deploy commands scoped to repo workdir.
fn ci_cd_agent_template() -> PolicyTemplate {
    PolicyTemplate {
        name: "ci-cd-agent".into(),
        description: "CI/CD pipeline agent. Allows build, test, and deployment commands scoped to repository working directory. Production push requires approval.".into(),
        category: TemplateCategory::CiCdAgent,
        yaml: r#"spec:
  commands:
    allow:
      - pattern: "^git (clone|pull|fetch|checkout|status|log|diff|tag|push).*"
      - pattern: "^(npm|yarn|pnpm) (install|ci|test|run|build|publish).*"
      - pattern: "^cargo (build|test|clippy|fmt|publish).*"
      - pattern: "^(docker|podman) (build|push|tag|login).*"
      - pattern: "^(make|cmake|go build|go test).*"
      - pattern: "^(python|pip|uv) .*"
      - pattern: "^(kubectl apply|kubectl rollout|helm upgrade|helm install).*"
      - pattern: "^(terraform|tofu) (plan|apply|init|validate).*"
      - pattern: "^(aws|gcloud|az) .*"
      - pattern: "^(cat|ls|find|grep|wc|head|tail) .*"
    deny:
      - pattern: "^rm -rf /.*"
      - pattern: "^sudo .*"
      - pattern: "^(curl|wget) .*(pastebin|gist|transfer\\.sh).*"
      - pattern: "^(ssh|scp|rsync) .*"
      - pattern: "^(iptables|ufw|firewall-cmd) .*"
      - pattern: "^(useradd|userdel|passwd) .*"
      - pattern: ".*(\\.env|secret|credential|password).*cat.*"
  filesystem:
    allow:
      - path: .
      - path: /tmp
    deny:
      - path: /etc
      - path: /var
      - path: /root
      - path: ~/.ssh
      - path: ~/.aws/credentials
  resources:
    maxOutputBytes: 52428800
    timeoutSeconds: 600"#
            .into(),
    }
}

/// Database query agent template: SELECT allowed, DML denied by default.
fn database_query_template() -> PolicyTemplate {
    PolicyTemplate {
        name: "database-query".into(),
        description: "Database query agent. Allows SELECT queries and read-only database operations. DML (INSERT, UPDATE, DELETE) denied by default. Schema changes require approval.".into(),
        category: TemplateCategory::DatabaseQuery,
        yaml: r#"spec:
  commands:
    allow:
      - pattern: "^psql .* -c \"SELECT .*\"$"
      - pattern: "^psql .* -c \"\\\\d.*\"$"
      - pattern: "^psql .* -c \"EXPLAIN .*\"$"
      - pattern: "^mysql .* -e \"SELECT .*\"$"
      - pattern: "^mysql .* -e \"SHOW .*\"$"
      - pattern: "^mysql .* -e \"DESCRIBE .*\"$"
      - pattern: "^sqlite3 .* \"SELECT .*\"$"
      - pattern: "^redis-cli (GET|MGET|HGET|HGETALL|KEYS|SCAN|INFO|DBSIZE|TYPE) .*"
      - pattern: "^mongosh .* --eval \"db\\..*\\.find\\(.*\\)\"$"
      - pattern: "^(pg_dump|mysqldump) --schema-only .*"
    deny:
      - pattern: "^psql .* -c \"(INSERT|UPDATE|DELETE|DROP|ALTER|TRUNCATE|CREATE) .*"
      - pattern: "^mysql .* -e \"(INSERT|UPDATE|DELETE|DROP|ALTER|TRUNCATE|CREATE) .*"
      - pattern: "^redis-cli (SET|DEL|FLUSHDB|FLUSHALL|SHUTDOWN) .*"
      - pattern: "^(rm|mv|cp|chmod|chown) .*"
      - pattern: "^sudo .*"
      - pattern: ".*(password|credential|secret|token).*"
      - pattern: "^(curl|wget|nc) .*"
  filesystem:
    allow:
      - path: /tmp
    deny:
      - path: /etc
      - path: /var
      - path: /root
      - path: ~/.ssh
      - path: ~/.pgpass
      - path: ~/.my.cnf
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 30"#
            .into(),
    }
}

/// Safe Dev Mode: AI can read/write project files, run tests, use git.
/// Cannot touch system, credentials, or network. Designed for Claude Code, Cursor, Aider.
fn safe_dev_mode_template() -> PolicyTemplate {
    PolicyTemplate {
        name: "safe-dev-mode".into(),
        description: "Drop-in safe mode for AI coding agents. Read/write project files, run tests, use git. Blocks system access, credential files, network tools, and destructive operations. Works with Claude Code, Cursor, Aider out of the box.".into(),
        category: TemplateCategory::ProductizeAI,
        yaml: r#"spec:
  commands:
    allow:
      - pattern: "^git (status|log|diff|add|commit|branch|checkout|stash|show|blame|rev-parse|push|pull|fetch|rebase|merge|cherry-pick).*"
      - pattern: "^(cargo|npm|yarn|pnpm|pip|uv|go|make|cmake|gradle|mvn) (build|test|check|clippy|fmt|lint|run|install|ci).*"
      - pattern: "^(cat|head|tail|less|wc|grep|rg|find|fd|ls|tree|file|stat|diff|sort|uniq|awk|sed) .*"
      - pattern: "^(mkdir|cp|mv|touch|chmod) .*"
      - pattern: "^(python|python3|node|ruby|perl) .*"
      - pattern: "^(docker|podman) (build|run|ps|logs|compose).*"
      - pattern: "^(echo|printf|cat|tee) .*"
    deny:
      - pattern: "^rm -rf /.*"
      - pattern: "^rm -rf ~.*"
      - pattern: "^(curl|wget|nc|ncat|socat|telnet|ftp) .*"
      - pattern: "^sudo .*"
      - pattern: "^(ssh|scp|rsync|sftp) .*"
      - pattern: "^(iptables|ufw|firewall-cmd|pfctl) .*"
      - pattern: "^(systemctl|service|launchctl) (start|stop|restart|enable|disable) .*"
      - pattern: "^(useradd|userdel|passwd|chown|chgrp) .*"
      - pattern: "^(mount|umount|mkfs|fdisk|dd) .*"
      - pattern: "^(kill|killall|pkill) .*"
      - pattern: ".*(password|secret|token|credential|api.key|\\.env).*"
      - pattern: "^(base64|openssl|gpg) .*(decode|decrypt|enc).*"
  filesystem:
    allow:
      - path: .
      - path: /tmp
    deny:
      - path: /etc
      - path: /var
      - path: /usr
      - path: /root
      - path: /home
      - path: ~/.ssh
      - path: ~/.aws
      - path: ~/.config/gh
      - path: ~/.gnupg
      - path: ~/.netrc
      - path: ~/.npmrc
      - path: ~/.pypirc
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300"#
            .into(),
    }
}

/// Production AI Guardrails: read-only production access with mandatory human approval for any mutation.
fn production_ai_guardrails_template() -> PolicyTemplate {
    PolicyTemplate {
        name: "production-ai-guardrails".into(),
        description: "Production guardrails for AI agents. Read-only access to production systems. All mutations require explicit human approval. Full audit trail with reasoning capture. Blocks exfiltration, destructive ops, and credential access.".into(),
        category: TemplateCategory::ProductizeAI,
        yaml: r#"spec:
  commands:
    allow:
      - pattern: "^systemctl status .*"
      - pattern: "^journalctl (--no-pager|--since|--until|-u) .*"
      - pattern: "^kubectl get .*"
      - pattern: "^kubectl describe .*"
      - pattern: "^kubectl logs .*"
      - pattern: "^kubectl top (nodes|pods).*"
      - pattern: "^docker (ps|logs|inspect|stats|images).*"
      - pattern: "^(df|du|free|top|uptime|w|who|last|ps aux|lsof).*"
      - pattern: "^(ip addr|ip route|ss -tlnp|netstat -tlnp).*"
      - pattern: "^(cat|head|tail|grep|awk) /var/log/.*"
      - pattern: "^curl -s (localhost|127\\.0\\.0\\.1|\\$).*"
      - pattern: "^(terraform|tofu) (plan|show|state list|state show).*"
      - pattern: "^(aws|gcloud|az) .*(describe|get|list|show).*"
      - pattern: "^helm (list|status|get).*"
    deny:
      - pattern: "^(rm|mv|cp|mkdir|touch|chmod|chown|dd|mkfs) .*"
      - pattern: "^(systemctl|service) (start|stop|restart|enable|disable) .*"
      - pattern: "^kubectl (delete|apply|patch|edit|scale|rollout|exec) .*"
      - pattern: "^docker (rm|rmi|stop|kill|exec|run) .*"
      - pattern: "^(apt|yum|dnf|pacman|pip|npm|cargo) (install|remove|update|upgrade|uninstall) .*"
      - pattern: "^sudo .*"
      - pattern: "^(terraform|tofu) (apply|destroy|import).*"
      - pattern: "^(aws|gcloud|az) .*(create|delete|update|put|modify|terminate).*"
      - pattern: "^helm (install|upgrade|delete|rollback).*"
      - pattern: "^(curl|wget) .*(-X POST|-X PUT|-X DELETE|-d |--data).*"
      - pattern: "^(curl|wget|nc|ncat|socat) .*[^(localhost|127\\.0\\.0\\.1)].*--upload.*"
      - pattern: "^(scp|rsync|sftp|ftp) .*"
      - pattern: ".*(password|secret|token|credential|api.key).*cat.*"
      - pattern: ".*> /dev/.*"
      - pattern: ".*>> /etc/.*"
  filesystem:
    allow:
      - path: /var/log
      - path: /tmp
    deny:
      - path: /etc
      - path: /root
      - path: ~/.ssh
      - path: ~/.aws/credentials
      - path: ~/.kube/config
  resources:
    maxOutputBytes: 5242880
    timeoutSeconds: 60"#
            .into(),
    }
}

/// Read-only Infrastructure AI: query logs, metrics, status. Block all writes and exfiltration.
fn read_only_infrastructure_ai_template() -> PolicyTemplate {
    PolicyTemplate {
        name: "read-only-infrastructure-ai".into(),
        description: "Strict read-only infrastructure access for AI agents. Query logs, metrics, health status only. Blocks ALL writes, ALL network egress, ALL credential paths. Zero mutation surface.".into(),
        category: TemplateCategory::ProductizeAI,
        yaml: r#"spec:
  commands:
    allow:
      - pattern: "^cat /var/log/.*"
      - pattern: "^tail (-[0-9]+f?|-n [0-9]+|--follow|-f) /var/log/.*"
      - pattern: "^grep .* /var/log/.*"
      - pattern: "^journalctl (--no-pager|--since|--until|-u|-n) .*"
      - pattern: "^systemctl (status|is-active|is-enabled|list-units) .*"
      - pattern: "^(df|du|free|uptime|w|who|last|ps aux|top -bn1).*"
      - pattern: "^(ip addr|ip route|ss -tlnp|netstat -tlnp).*"
      - pattern: "^kubectl get .*"
      - pattern: "^kubectl describe .*"
      - pattern: "^kubectl logs .* --tail=[0-9]+.*"
      - pattern: "^kubectl top (nodes|pods).*"
      - pattern: "^docker (ps|stats|inspect) .*"
      - pattern: "^(prometheus|promtool) .*(query|check).*"
      - pattern: "^curl -s (localhost|127\\.0\\.0\\.1):(9090|3000|8080)/.*"
    deny:
      - pattern: ".*"
  filesystem:
    allow:
      - path: /var/log
      - path: /proc
      - path: /sys/class
    deny:
      - path: /
  resources:
    maxOutputBytes: 2097152
    timeoutSeconds: 30"#
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_all_templates() {
        let registry = TemplateRegistry::new();
        assert_eq!(registry.list().len(), 8);
    }

    #[test]
    fn test_get_template_by_name() {
        let registry = TemplateRegistry::new();
        let template = registry.get("coding-assistant").unwrap();
        assert_eq!(template.name, "coding-assistant");
        assert_eq!(template.category, TemplateCategory::CodingAssistant);
    }

    #[test]
    fn test_get_nonexistent_template() {
        let registry = TemplateRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_templates_by_category() {
        let registry = TemplateRegistry::new();
        let coding = registry.by_category(&TemplateCategory::CodingAssistant);
        assert_eq!(coding.len(), 1);
        assert_eq!(coding[0].name, "coding-assistant");
    }

    #[test]
    fn test_all_templates_are_valid_yaml() {
        let registry = TemplateRegistry::new();
        for template in registry.list() {
            let result = TemplateRegistry::validate_yaml(&template.yaml);
            assert!(
                result.is_ok(),
                "Template '{}' has invalid YAML: {:?}",
                template.name,
                result.err()
            );
        }
    }

    #[test]
    fn test_compose_two_templates() {
        let registry = TemplateRegistry::new();
        let coding = registry.get("coding-assistant").unwrap();
        let readonly = registry.get("production-read-only").unwrap();

        let composed = TemplateRegistry::compose(&[coding, readonly]).unwrap();

        // Composed policy should contain deny rules from both
        assert!(composed.contains("deny:"));
        assert!(composed.contains("allow:"));
        // Most restrictive timeout wins (60 from readonly vs 300 from coding)
        assert!(composed.contains("timeoutSeconds: 60"));
    }

    #[test]
    fn test_compose_empty_fails() {
        let result = TemplateRegistry::compose(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_valid_yaml() {
        let yaml = "spec:\n  commands:\n    allow:\n      - pattern: \"^ls\"";
        assert!(TemplateRegistry::validate_yaml(yaml).is_ok());
    }

    #[test]
    fn test_validate_invalid_yaml() {
        let yaml = "spec:\n  commands:\n    allow: [invalid yaml {{{}}}";
        assert!(TemplateRegistry::validate_yaml(yaml).is_err());
    }

    #[test]
    fn test_template_category_display() {
        assert_eq!(
            TemplateCategory::CodingAssistant.to_string(),
            "coding-assistant"
        );
        assert_eq!(
            TemplateCategory::ProductionReadOnly.to_string(),
            "production-read-only"
        );
        assert_eq!(
            TemplateCategory::SecurityInvestigator.to_string(),
            "security-investigator"
        );
        assert_eq!(TemplateCategory::CiCdAgent.to_string(), "ci-cd-agent");
        assert_eq!(
            TemplateCategory::DatabaseQuery.to_string(),
            "database-query"
        );
        assert_eq!(TemplateCategory::ProductizeAI.to_string(), "productize-ai");
    }

    #[test]
    fn test_productize_ai_templates() {
        let registry = TemplateRegistry::new();
        let productize = registry.by_category(&TemplateCategory::ProductizeAI);
        assert_eq!(productize.len(), 3);
        assert!(registry.get("safe-dev-mode").is_some());
        assert!(registry.get("production-ai-guardrails").is_some());
        assert!(registry.get("read-only-infrastructure-ai").is_some());
    }
}
