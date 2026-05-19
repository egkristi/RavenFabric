use std::net::IpAddr;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;

use crate::decision::Decision;
use crate::error::PolicyError;

/// A CIDR network prefix for matching IP addresses.
#[derive(Debug, Clone)]
struct CidrNetwork {
    addr: IpAddr,
    prefix_len: u8,
}

impl CidrNetwork {
    fn parse(s: &str) -> Result<Self, PolicyError> {
        let (addr_str, prefix_str) = s.split_once('/').ok_or_else(|| PolicyError::InvalidCidr {
            cidr: s.to_string(),
            reason: "missing /prefix_len".into(),
        })?;
        let addr: IpAddr = addr_str.parse().map_err(|_| PolicyError::InvalidCidr {
            cidr: s.to_string(),
            reason: format!("invalid IP address: {addr_str}"),
        })?;
        let prefix_len: u8 = prefix_str.parse().map_err(|_| PolicyError::InvalidCidr {
            cidr: s.to_string(),
            reason: format!("invalid prefix length: {prefix_str}"),
        })?;
        let max_prefix = match addr {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        if prefix_len > max_prefix {
            return Err(PolicyError::InvalidCidr {
                cidr: s.to_string(),
                reason: format!("prefix length {prefix_len} exceeds maximum {max_prefix}"),
            });
        }
        Ok(Self { addr, prefix_len })
    }

    fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip) {
            (IpAddr::V4(net), IpAddr::V4(target)) => {
                if self.prefix_len == 0 {
                    return true;
                }
                let net_bits = u32::from(net);
                let target_bits = u32::from(target);
                let mask = u32::MAX
                    .checked_shl(32 - u32::from(self.prefix_len))
                    .unwrap_or(0);
                (net_bits & mask) == (target_bits & mask)
            }
            (IpAddr::V6(net), IpAddr::V6(target)) => {
                if self.prefix_len == 0 {
                    return true;
                }
                let net_bits = u128::from(net);
                let target_bits = u128::from(target);
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.prefix_len))
                    .unwrap_or(0);
                (net_bits & mask) == (target_bits & mask)
            }
            _ => false, // v4 vs v6 mismatch
        }
    }
}

/// A parsed port range (inclusive).
#[derive(Debug, Clone)]
struct PortRange {
    start: u16,
    end: u16,
}

impl PortRange {
    fn contains(&self, port: u16) -> bool {
        port >= self.start && port <= self.end
    }
}

/// A compiled network policy rule.
#[derive(Debug, Clone)]
struct NetworkRule {
    cidr: Option<CidrNetwork>,
    hostname: Option<String>,
    ports: Vec<PortRange>,
}

impl NetworkRule {
    fn matches(&self, host: &str, ip: Option<IpAddr>, port: u16) -> bool {
        // Check port constraint (empty means all ports)
        if !self.ports.is_empty() && !self.ports.iter().any(|r| r.contains(port)) {
            return false;
        }

        // Check CIDR match
        if let Some(cidr) = &self.cidr {
            if let Some(ip) = ip {
                return cidr.contains(ip);
            }
            // Try parsing host as IP directly
            if let Ok(parsed_ip) = host.parse::<IpAddr>() {
                return cidr.contains(parsed_ip);
            }
            return false;
        }

        // Check hostname match (glob-style with leading wildcard)
        if let Some(pattern) = &self.hostname {
            return hostname_matches(pattern, host);
        }

        false
    }
}

/// Simple hostname glob matching (supports leading `*.` prefix).
fn hostname_matches(pattern: &str, host: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        // Match exact suffix or the suffix itself
        host == suffix || host.ends_with(&format!(".{suffix}"))
    } else {
        host == pattern
    }
}

/// Parse port specifications like "80", "8080-8090", or "*".
fn parse_ports(ports: &[String]) -> Result<Vec<PortRange>, PolicyError> {
    let mut result = Vec::new();
    for spec in ports {
        if spec == "*" {
            result.push(PortRange {
                start: 1,
                end: 65535,
            });
            continue;
        }
        if let Some((start_s, end_s)) = spec.split_once('-') {
            let start: u16 = start_s
                .parse()
                .map_err(|_| PolicyError::InvalidPortSpec { spec: spec.clone() })?;
            let end: u16 = end_s
                .parse()
                .map_err(|_| PolicyError::InvalidPortSpec { spec: spec.clone() })?;
            if start > end {
                return Err(PolicyError::InvalidPortSpec { spec: spec.clone() });
            }
            result.push(PortRange { start, end });
        } else {
            let port: u16 = spec
                .parse()
                .map_err(|_| PolicyError::InvalidPortSpec { spec: spec.clone() })?;
            result.push(PortRange {
                start: port,
                end: port,
            });
        }
    }
    Ok(result)
}

/// RPCPolicy — commands, filesystem, network, resources.
pub struct RpcPolicy {
    allowed_commands: Vec<Regex>,
    denied_commands: Vec<Regex>,
    allowed_paths: Vec<PathBuf>,
    denied_paths: Vec<PathBuf>,
    allowed_networks: Vec<NetworkRule>,
    denied_networks: Vec<NetworkRule>,
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
    network: Option<NetworkSpec>,
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
struct NetworkSpec {
    allow: Option<Vec<NetworkEntry>>,
    deny: Option<Vec<NetworkEntry>>,
}

#[derive(Debug, Deserialize)]
struct NetworkEntry {
    cidr: Option<String>,
    hostname: Option<String>,
    ports: Option<Vec<String>>,
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

        let allowed_networks = spec
            .network
            .as_ref()
            .and_then(|n| n.allow.as_ref())
            .map(|entries| {
                entries
                    .iter()
                    .map(compile_network_rule)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();

        let denied_networks = spec
            .network
            .as_ref()
            .and_then(|n| n.deny.as_ref())
            .map(|entries| {
                entries
                    .iter()
                    .map(compile_network_rule)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();

        let resources = spec.resources.as_ref();

        Ok(Self {
            allowed_commands,
            denied_commands,
            allowed_paths,
            denied_paths,
            allowed_networks,
            denied_networks,
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

    /// Check if a network target (host:port) is allowed by policy.
    /// Immutable deny is enforced first (link-local, metadata endpoints).
    /// Then explicit deny rules. Then allow rules. Default: deny.
    pub fn check_network_target(&self, target: &str) -> Decision {
        // Parse target as host:port
        let (host, port) = match parse_host_port(target) {
            Some(hp) => hp,
            None => {
                return Decision::deny(
                    format!("invalid target format (expected host:port): {target}"),
                    "invalid_target",
                );
            }
        };

        // Try to parse host as IP address
        let ip: Option<IpAddr> = host.parse().ok();

        // Immutable deny: link-local (169.254.0.0/16), cloud metadata (169.254.169.254)
        if let Some(ip_val) = ip {
            if is_link_local(ip_val) {
                return Decision::deny(
                    "immutable deny: link-local/metadata address",
                    format!("immutable_deny:link_local:{ip_val}"),
                );
            }
        }

        // Explicit deny rules
        for (i, rule) in self.denied_networks.iter().enumerate() {
            if rule.matches(&host, ip, port) {
                return Decision::deny(
                    format!("network target denied by rule: {target}"),
                    format!("network:deny[{i}]"),
                );
            }
        }

        // Allow rules
        for (i, rule) in self.allowed_networks.iter().enumerate() {
            if rule.matches(&host, ip, port) {
                return Decision::allow(format!("network:allow[{i}]"));
            }
        }

        // Default: deny
        Decision::deny_default()
    }
}

/// Check if an IP is in the link-local range (169.254.0.0/16 for IPv4, fe80::/10 for IPv6).
fn is_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            octets[0] == 169 && octets[1] == 254
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            (segments[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Parse a target string as "host:port". Handles IPv6 bracket notation [::1]:port.
fn parse_host_port(target: &str) -> Option<(String, u16)> {
    if target.starts_with('[') {
        // IPv6 bracket notation: [::1]:8080
        let end_bracket = target.find(']')?;
        let host = &target[1..end_bracket];
        let rest = &target[end_bracket + 1..];
        let port_str = rest.strip_prefix(':')?;
        let port: u16 = port_str.parse().ok()?;
        Some((host.to_string(), port))
    } else {
        // host:port or IPv4:port
        let last_colon = target.rfind(':')?;
        let host = &target[..last_colon];
        let port_str = &target[last_colon + 1..];
        let port: u16 = port_str.parse().ok()?;
        if host.is_empty() {
            return None;
        }
        Some((host.to_string(), port))
    }
}

/// Compile a `NetworkEntry` from YAML into a `NetworkRule`.
fn compile_network_rule(entry: &NetworkEntry) -> Result<NetworkRule, PolicyError> {
    let cidr = entry.cidr.as_deref().map(CidrNetwork::parse).transpose()?;
    let hostname = entry.hostname.clone();
    let ports = entry
        .ports
        .as_ref()
        .map(|p| parse_ports(p))
        .transpose()?
        .unwrap_or_default();

    if cidr.is_none() && hostname.is_none() {
        return Err(PolicyError::InvalidNetworkRule {
            reason: "network rule must have either 'cidr' or 'hostname'".into(),
        });
    }

    Ok(NetworkRule {
        cidr,
        hostname,
        ports,
    })
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

    #[test]
    fn test_network_allow_cidr() {
        let yaml = r#"
spec:
  network:
    allow:
      - cidr: "10.0.0.0/8"
        ports: ["80", "443"]
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        assert!(policy.check_network_target("10.1.2.3:80").allowed);
        assert!(policy.check_network_target("10.255.0.1:443").allowed);
        // Wrong port
        assert!(!policy.check_network_target("10.1.2.3:22").allowed);
        // Wrong CIDR
        assert!(!policy.check_network_target("192.168.1.1:80").allowed);
    }

    #[test]
    fn test_network_allow_port_range() {
        let yaml = r#"
spec:
  network:
    allow:
      - cidr: "10.0.0.0/8"
        ports: ["8080-8090"]
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        assert!(policy.check_network_target("10.1.2.3:8080").allowed);
        assert!(policy.check_network_target("10.1.2.3:8085").allowed);
        assert!(policy.check_network_target("10.1.2.3:8090").allowed);
        assert!(!policy.check_network_target("10.1.2.3:8091").allowed);
        assert!(!policy.check_network_target("10.1.2.3:8079").allowed);
    }

    #[test]
    fn test_network_deny_overrides_allow() {
        let yaml = r#"
spec:
  network:
    allow:
      - cidr: "10.0.0.0/8"
    deny:
      - cidr: "10.0.0.0/24"
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        // 10.0.0.x is denied
        assert!(!policy.check_network_target("10.0.0.5:80").allowed);
        // Other 10.x is allowed
        assert!(policy.check_network_target("10.0.1.5:80").allowed);
    }

    #[test]
    fn test_network_immutable_deny_link_local() {
        let yaml = r#"
spec:
  network:
    allow:
      - cidr: "0.0.0.0/0"
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        // Link-local (cloud metadata) is always denied
        assert!(!policy.check_network_target("169.254.169.254:80").allowed);
        assert!(!policy.check_network_target("169.254.0.1:443").allowed);
        // Normal IPs are fine
        assert!(policy.check_network_target("8.8.8.8:53").allowed);
    }

    #[test]
    fn test_network_hostname_match() {
        let yaml = r#"
spec:
  network:
    allow:
      - hostname: "*.internal.com"
        ports: ["443"]
      - hostname: "api.example.com"
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        assert!(policy.check_network_target("web.internal.com:443").allowed);
        assert!(policy.check_network_target("db.internal.com:443").allowed);
        // Wrong port for hostname rule
        assert!(!policy.check_network_target("web.internal.com:80").allowed);
        // Exact hostname match (any port since no port constraint)
        assert!(policy.check_network_target("api.example.com:8080").allowed);
        // No match
        assert!(!policy.check_network_target("evil.com:443").allowed);
    }

    #[test]
    fn test_network_default_deny() {
        let yaml = r#"
spec:
  commands:
    allow:
      - pattern: ".*"
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        // No network rules → deny by default
        assert!(!policy.check_network_target("10.0.0.1:80").allowed);
    }

    #[test]
    fn test_network_invalid_target() {
        let yaml = r#"
spec:
  network:
    allow:
      - cidr: "0.0.0.0/0"
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        // Missing port
        assert!(!policy.check_network_target("10.0.0.1").allowed);
        // Empty
        assert!(!policy.check_network_target("").allowed);
        // Bad port
        assert!(!policy.check_network_target("10.0.0.1:abc").allowed);
    }

    #[test]
    fn test_network_ipv6() {
        let yaml = r#"
spec:
  network:
    allow:
      - cidr: "fd00::/8"
        ports: ["443"]
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        assert!(policy.check_network_target("[fd00::1]:443").allowed);
        assert!(!policy.check_network_target("[fd00::1]:80").allowed);
        assert!(!policy.check_network_target("[2001:db8::1]:443").allowed);
    }

    #[test]
    fn test_network_ipv6_link_local_denied() {
        let yaml = r#"
spec:
  network:
    allow:
      - cidr: "::/0"
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        assert!(!policy.check_network_target("[fe80::1]:80").allowed);
    }

    #[test]
    fn test_network_wildcard_hostname() {
        let yaml = r#"
spec:
  network:
    allow:
      - hostname: "*"
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        assert!(policy.check_network_target("anything.com:443").allowed);
        assert!(policy.check_network_target("x.y.z:80").allowed);
    }

    #[test]
    fn test_network_invalid_cidr_rejected() {
        let yaml = r#"
spec:
  network:
    allow:
      - cidr: "not-an-ip/8"
"#;
        assert!(RpcPolicy::from_yaml(yaml).is_err());
    }

    #[test]
    fn test_network_invalid_port_spec_rejected() {
        let yaml = r#"
spec:
  network:
    allow:
      - cidr: "10.0.0.0/8"
        ports: ["abc"]
"#;
        assert!(RpcPolicy::from_yaml(yaml).is_err());
    }

    #[test]
    fn test_network_rule_requires_cidr_or_hostname() {
        let yaml = r#"
spec:
  network:
    allow:
      - ports: ["80"]
"#;
        assert!(RpcPolicy::from_yaml(yaml).is_err());
    }
}
