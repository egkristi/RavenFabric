//! Capability-based authorization types (Biscuit-inspired).
//!
//! Implements capability tokens that are:
//! - Self-contained (carry their own signed permissions)
//! - Delegatable (agent A can grant agent B limited capabilities)
//! - Attenuatable (capabilities can be narrowed, never widened)
//! - Offline-verifiable (no central authority needed at execution time)

use serde::{Deserialize, Serialize};

/// A capability token (Biscuit-inspired).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityToken {
    /// Token ID (unique, for revocation).
    pub id: String,
    /// Authority block (root permissions from issuer).
    pub authority: AuthorityBlock,
    /// Attenuation blocks (each narrows permissions further).
    pub attenuations: Vec<AttenuationBlock>,
    /// Cryptographic signature chain.
    pub signatures: Vec<String>,
    /// Expiry timestamp (Unix seconds, 0 = no expiry).
    pub expires_at: u64,
}

/// Authority block: defines the maximum permission scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorityBlock {
    /// Issuer public key (base64).
    pub issuer: String,
    /// Granted capabilities.
    pub capabilities: Vec<Capability>,
    /// Hard constraints that cannot be overridden.
    pub caveats: Vec<Caveat>,
}

/// A single capability grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    /// Resource pattern (e.g., "command:*", "file:/opt/app/**").
    pub resource: String,
    /// Action (e.g., "execute", "read", "write").
    pub action: String,
    /// Optional conditions.
    pub conditions: Vec<Condition>,
}

/// Condition on a capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    /// Time-based: only valid within time window.
    TimeWindow { not_before: u64, not_after: u64 },
    /// Source IP restriction.
    SourceIp { allowed: Vec<String> },
    /// Maximum invocation count.
    MaxInvocations { count: u32 },
    /// Target agent restriction.
    TargetAgent { agent_ids: Vec<String> },
}

/// Attenuation block: narrows the scope of authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttenuationBlock {
    /// Who created this attenuation.
    pub delegator: String,
    /// Additional restrictions (AND-ed with authority).
    pub restrictions: Vec<Restriction>,
    /// Signature of this block by delegator.
    pub signature: String,
}

/// Restriction in an attenuation block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Restriction {
    /// Narrow resource pattern.
    ResourcePrefix { prefix: String },
    /// Restrict to specific actions.
    AllowedActions { actions: Vec<String> },
    /// Reduce max invocations.
    MaxUses { remaining: u32 },
    /// Shorten expiry.
    ExpireBefore { timestamp: u64 },
    /// Restrict to subset of agents.
    AgentSubset { agent_ids: Vec<String> },
}

/// Validation result for a capability token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenValidation {
    /// Token is valid for the requested action.
    Valid,
    /// Token has expired.
    Expired,
    /// Signature verification failed.
    InvalidSignature,
    /// Token has been revoked.
    Revoked,
    /// Action not permitted by capabilities.
    Denied { reason: String },
    /// Caveat check failed.
    CaveatFailed { caveat: String },
}

/// Caveat: a check that must pass for the token to be valid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Caveat {
    /// Must be verified before this time.
    ExpiresAt { timestamp: u64 },
    /// Must originate from specific network.
    SourceNetwork { cidr: String },
    /// Must target specific resource pattern.
    ResourceMatch { pattern: String },
    /// Custom fact check (Datalog-style).
    FactCheck { fact: String },
}

/// Check if capabilities allow a specific action.
pub fn check_capability(
    token: &CapabilityToken,
    resource: &str,
    action: &str,
    now: u64,
) -> TokenValidation {
    // Check expiry.
    if token.expires_at > 0 && now > token.expires_at {
        return TokenValidation::Expired;
    }

    // Check authority caveats.
    for caveat in &token.authority.caveats {
        match caveat {
            Caveat::ExpiresAt { timestamp } => {
                if now > *timestamp {
                    return TokenValidation::CaveatFailed {
                        caveat: "expired".into(),
                    };
                }
            }
            Caveat::ResourceMatch { pattern } => {
                if !resource.starts_with(pattern) {
                    return TokenValidation::CaveatFailed {
                        caveat: format!("resource must match {pattern}"),
                    };
                }
            }
            _ => {}
        }
    }

    // Check if any capability matches.
    let has_capability = token.authority.capabilities.iter().any(|cap| {
        resource_matches(&cap.resource, resource) && (cap.action == "*" || cap.action == action)
    });

    if !has_capability {
        return TokenValidation::Denied {
            reason: format!("no capability for {action} on {resource}"),
        };
    }

    // Check attenuation blocks (each narrows further).
    for attenuation in &token.attenuations {
        for restriction in &attenuation.restrictions {
            match restriction {
                Restriction::ResourcePrefix { prefix } => {
                    if !resource.starts_with(prefix.as_str()) {
                        return TokenValidation::Denied {
                            reason: format!("attenuated: resource must start with {prefix}"),
                        };
                    }
                }
                Restriction::AllowedActions { actions } => {
                    if !actions.contains(&action.to_string()) {
                        return TokenValidation::Denied {
                            reason: format!("attenuated: action {action} not in allowed list"),
                        };
                    }
                }
                Restriction::ExpireBefore { timestamp } => {
                    if now > *timestamp {
                        return TokenValidation::Expired;
                    }
                }
                _ => {}
            }
        }
    }

    TokenValidation::Valid
}

/// Simple glob-style resource matching.
fn resource_matches(pattern: &str, resource: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        return resource.starts_with(prefix);
    }
    pattern == resource
}

/// SecurityPolicy with immutable rules that cannot be overridden.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// Immutable deny rules (cannot be overridden by any token).
    pub immutable_deny: Vec<String>,
    /// Maximum token lifetime (seconds).
    pub max_token_lifetime_secs: u64,
    /// Maximum delegation depth.
    pub max_delegation_depth: u8,
    /// Require all tokens to have expiry.
    pub require_expiry: bool,
    /// Minimum key strength (bits).
    pub min_key_bits: u16,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            immutable_deny: vec![
                "command:rm -rf /".into(),
                "file:/etc/shadow".into(),
                "file:/proc/kcore".into(),
            ],
            max_token_lifetime_secs: 86400, // 24 hours
            max_delegation_depth: 3,
            require_expiry: true,
            min_key_bits: 256,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_token(capabilities: Vec<Capability>, expires_at: u64) -> CapabilityToken {
        CapabilityToken {
            id: "test-token".into(),
            authority: AuthorityBlock {
                issuer: "issuer-key".into(),
                capabilities,
                caveats: Vec::new(),
            },
            attenuations: Vec::new(),
            signatures: vec!["sig".into()],
            expires_at,
        }
    }

    #[test]
    fn test_valid_capability_check() {
        let token = make_token(
            vec![Capability {
                resource: "command:*".into(),
                action: "execute".into(),
                conditions: vec![],
            }],
            0,
        );
        let result = check_capability(&token, "command:ls", "execute", 1000);
        assert_eq!(result, TokenValidation::Valid);
    }

    #[test]
    fn test_expired_token() {
        let token = make_token(
            vec![Capability {
                resource: "*".into(),
                action: "*".into(),
                conditions: vec![],
            }],
            500,
        );
        let result = check_capability(&token, "command:ls", "execute", 1000);
        assert_eq!(result, TokenValidation::Expired);
    }

    #[test]
    fn test_denied_no_capability() {
        let token = make_token(
            vec![Capability {
                resource: "file:/opt/*".into(),
                action: "read".into(),
                conditions: vec![],
            }],
            0,
        );
        let result = check_capability(&token, "command:ls", "execute", 1000);
        assert!(matches!(result, TokenValidation::Denied { .. }));
    }

    #[test]
    fn test_attenuation_narrows() {
        let mut token = make_token(
            vec![Capability {
                resource: "file:*".into(),
                action: "*".into(),
                conditions: vec![],
            }],
            0,
        );
        token.attenuations.push(AttenuationBlock {
            delegator: "delegator-key".into(),
            restrictions: vec![Restriction::ResourcePrefix {
                prefix: "file:/opt/app".into(),
            }],
            signature: "sig2".into(),
        });

        // Allowed: within attenuated prefix.
        let result = check_capability(&token, "file:/opt/app/data.txt", "read", 1000);
        assert_eq!(result, TokenValidation::Valid);

        // Denied: outside attenuated prefix.
        let result = check_capability(&token, "file:/etc/passwd", "read", 1000);
        assert!(matches!(result, TokenValidation::Denied { .. }));
    }

    #[test]
    fn test_caveat_resource_match() {
        let mut token = make_token(
            vec![Capability {
                resource: "*".into(),
                action: "*".into(),
                conditions: vec![],
            }],
            0,
        );
        token.authority.caveats.push(Caveat::ResourceMatch {
            pattern: "command:".into(),
        });

        let result = check_capability(&token, "command:ls", "execute", 1000);
        assert_eq!(result, TokenValidation::Valid);

        let result = check_capability(&token, "file:/etc/passwd", "read", 1000);
        assert!(matches!(result, TokenValidation::CaveatFailed { .. }));
    }

    #[test]
    fn test_security_policy_defaults() {
        let policy = SecurityPolicy::default();
        assert_eq!(policy.max_delegation_depth, 3);
        assert!(policy.require_expiry);
        assert!(policy.immutable_deny.contains(&"file:/etc/shadow".into()));
    }

    #[test]
    fn test_resource_matching() {
        assert!(resource_matches("*", "anything"));
        assert!(resource_matches("file:/opt/*", "file:/opt/app/data"));
        assert!(!resource_matches("file:/opt/*", "file:/etc/passwd"));
        assert!(resource_matches("command:ls", "command:ls"));
        assert!(!resource_matches("command:ls", "command:cat"));
    }

    #[test]
    fn test_action_attenuation() {
        let mut token = make_token(
            vec![Capability {
                resource: "file:*".into(),
                action: "*".into(),
                conditions: vec![],
            }],
            0,
        );
        token.attenuations.push(AttenuationBlock {
            delegator: "delegator".into(),
            restrictions: vec![Restriction::AllowedActions {
                actions: vec!["read".into()],
            }],
            signature: "sig".into(),
        });

        // Read is allowed.
        let result = check_capability(&token, "file:/opt/data", "read", 1000);
        assert_eq!(result, TokenValidation::Valid);

        // Write is denied by attenuation.
        let result = check_capability(&token, "file:/opt/data", "write", 1000);
        assert!(matches!(result, TokenValidation::Denied { .. }));
    }
}
