//! Role-Based Access Control (RBAC) types for multi-tenant environments.
//!
//! Defines roles, permissions, and tenant isolation for the policy engine.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// A role that can be assigned to users/agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Full control over all resources and policies.
    Admin,
    /// Can execute commands and manage deployments.
    Operator,
    /// Read-only access to outputs, logs, and metrics.
    Viewer,
    /// Access to audit logs and security events only.
    Auditor,
    /// Custom role with explicit permission set.
    Custom(String),
}

/// A permission that can be granted to a role.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Can execute commands on agents.
    Execute,
    /// Can read file contents from agents.
    FileRead,
    /// Can write files to agents.
    FileWrite,
    /// Can view agent status and metrics.
    ViewStatus,
    /// Can view audit logs.
    ViewAudit,
    /// Can manage policies.
    ManagePolicy,
    /// Can manage agent enrollment.
    ManageAgents,
    /// Can manage port forwards.
    PortForward,
    /// Can access secrets.
    AccessSecrets,
}

/// Maps roles to their default permissions.
pub fn default_permissions(role: &Role) -> HashSet<Permission> {
    match role {
        Role::Admin => {
            let mut perms = HashSet::new();
            perms.insert(Permission::Execute);
            perms.insert(Permission::FileRead);
            perms.insert(Permission::FileWrite);
            perms.insert(Permission::ViewStatus);
            perms.insert(Permission::ViewAudit);
            perms.insert(Permission::ManagePolicy);
            perms.insert(Permission::ManageAgents);
            perms.insert(Permission::PortForward);
            perms.insert(Permission::AccessSecrets);
            perms
        }
        Role::Operator => {
            let mut perms = HashSet::new();
            perms.insert(Permission::Execute);
            perms.insert(Permission::FileRead);
            perms.insert(Permission::FileWrite);
            perms.insert(Permission::ViewStatus);
            perms.insert(Permission::PortForward);
            perms
        }
        Role::Viewer => {
            let mut perms = HashSet::new();
            perms.insert(Permission::ViewStatus);
            perms
        }
        Role::Auditor => {
            let mut perms = HashSet::new();
            perms.insert(Permission::ViewStatus);
            perms.insert(Permission::ViewAudit);
            perms
        }
        Role::Custom(_) => HashSet::new(),
    }
}

/// An identity (user or agent) with assigned roles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identity {
    /// Unique identifier (agent ID or user principal).
    pub id: String,
    /// Tenant this identity belongs to.
    pub tenant: Option<String>,
    /// Assigned roles.
    pub roles: Vec<Role>,
}

impl Identity {
    /// Check if this identity has a specific permission.
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.roles
            .iter()
            .any(|role| default_permissions(role).contains(permission))
    }

    /// Check if this identity has a specific role.
    pub fn has_role(&self, role: &Role) -> bool {
        self.roles.contains(role)
    }

    /// Get all effective permissions for this identity.
    pub fn effective_permissions(&self) -> HashSet<Permission> {
        self.roles.iter().flat_map(default_permissions).collect()
    }
}

/// Tenant-scoped access control.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TenantPolicy {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Allowed agent IDs for this tenant.
    pub allowed_agents: Vec<String>,
    /// Maximum concurrent executions.
    pub max_concurrent: Option<u32>,
}

impl TenantPolicy {
    /// Check if an agent belongs to this tenant.
    pub fn allows_agent(&self, agent_id: &str) -> bool {
        self.allowed_agents.iter().any(|a| a == agent_id)
    }
}

/// Tenant isolation enforcer — blocks cross-tenant access.
pub struct TenantIsolation {
    tenants: Vec<TenantPolicy>,
}

impl TenantIsolation {
    /// Create a new tenant isolation enforcer.
    pub fn new(tenants: Vec<TenantPolicy>) -> Self {
        Self { tenants }
    }

    /// Check if an identity can access a target agent.
    /// Returns the tenant policy if access is allowed.
    pub fn check_access<'a>(
        &'a self,
        identity: &Identity,
        target_agent: &str,
    ) -> Option<&'a TenantPolicy> {
        let tenant_id = identity.tenant.as_deref()?;
        let tenant = self.tenants.iter().find(|t| t.tenant_id == tenant_id)?;
        if tenant.allows_agent(target_agent) {
            Some(tenant)
        } else {
            None
        }
    }

    /// Check if two identities are in the same tenant.
    pub fn same_tenant(a: &Identity, b: &Identity) -> bool {
        match (&a.tenant, &b.tenant) {
            (Some(ta), Some(tb)) => ta == tb,
            _ => false,
        }
    }

    /// Get all agents accessible to a tenant.
    pub fn agents_for_tenant(&self, tenant_id: &str) -> Vec<&str> {
        self.tenants
            .iter()
            .filter(|t| t.tenant_id == tenant_id)
            .flat_map(|t| t.allowed_agents.iter().map(|a| a.as_str()))
            .collect()
    }

    /// Number of tenants.
    pub fn tenant_count(&self) -> usize {
        self.tenants.len()
    }
}

/// Security policy with immutable deny rules.
///
/// Immutable rules cannot be overridden by any RBAC role,
/// capability token, or policy merge. They are the security floor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// Rules that can NEVER be overridden (immutable deny-list).
    pub immutable_deny: Vec<String>,
    /// Maximum delegation depth for capability tokens.
    pub max_delegation_depth: u8,
    /// Maximum capability token lifetime in seconds.
    pub max_token_lifetime_secs: u64,
    /// Whether to require PQ crypto for new connections.
    pub require_pq: bool,
    /// Minimum required roles for policy changes.
    pub policy_change_roles: Vec<Role>,
}

impl SecurityPolicy {
    /// Check if a command is immutably denied.
    pub fn is_immutably_denied(&self, command: &str) -> bool {
        self.immutable_deny
            .iter()
            .any(|pattern| command.contains(pattern))
    }

    /// Check if an identity can modify policies.
    pub fn can_modify_policy(&self, identity: &Identity) -> bool {
        self.policy_change_roles
            .iter()
            .any(|required| identity.has_role(required))
    }

    /// Check if a delegation depth is within limits.
    pub fn is_delegation_allowed(&self, depth: u8) -> bool {
        depth <= self.max_delegation_depth
    }

    /// Check if a token lifetime is within limits.
    pub fn is_lifetime_allowed(&self, lifetime_secs: u64) -> bool {
        lifetime_secs <= self.max_token_lifetime_secs
    }
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            immutable_deny: vec![
                "rm -rf /".into(),
                "mkfs".into(),
                "dd if=/dev/zero".into(),
                ":(){ :|:& };:".into(), // Fork bomb
            ],
            max_delegation_depth: 3,
            max_token_lifetime_secs: 86400, // 24 hours
            require_pq: false,
            policy_change_roles: vec![Role::Admin],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admin_has_all_permissions() {
        let identity = Identity {
            id: "admin-user".into(),
            tenant: None,
            roles: vec![Role::Admin],
        };

        assert!(identity.has_permission(&Permission::Execute));
        assert!(identity.has_permission(&Permission::ManagePolicy));
        assert!(identity.has_permission(&Permission::AccessSecrets));
    }

    #[test]
    fn test_viewer_limited_permissions() {
        let identity = Identity {
            id: "viewer-user".into(),
            tenant: Some("acme".into()),
            roles: vec![Role::Viewer],
        };

        assert!(identity.has_permission(&Permission::ViewStatus));
        assert!(!identity.has_permission(&Permission::Execute));
        assert!(!identity.has_permission(&Permission::FileWrite));
    }

    #[test]
    fn test_operator_permissions() {
        let identity = Identity {
            id: "ops-user".into(),
            tenant: None,
            roles: vec![Role::Operator],
        };

        assert!(identity.has_permission(&Permission::Execute));
        assert!(identity.has_permission(&Permission::FileRead));
        assert!(identity.has_permission(&Permission::PortForward));
        assert!(!identity.has_permission(&Permission::ManagePolicy));
        assert!(!identity.has_permission(&Permission::AccessSecrets));
    }

    #[test]
    fn test_auditor_permissions() {
        let identity = Identity {
            id: "auditor".into(),
            tenant: None,
            roles: vec![Role::Auditor],
        };

        assert!(identity.has_permission(&Permission::ViewAudit));
        assert!(identity.has_permission(&Permission::ViewStatus));
        assert!(!identity.has_permission(&Permission::Execute));
    }

    #[test]
    fn test_multiple_roles() {
        let identity = Identity {
            id: "power-user".into(),
            tenant: None,
            roles: vec![Role::Viewer, Role::Operator],
        };

        let perms = identity.effective_permissions();
        assert!(perms.contains(&Permission::Execute));
        assert!(perms.contains(&Permission::ViewStatus));
    }

    #[test]
    fn test_tenant_policy() {
        let policy = TenantPolicy {
            tenant_id: "acme".into(),
            allowed_agents: vec!["web-01".into(), "web-02".into()],
            max_concurrent: Some(5),
        };

        assert!(policy.allows_agent("web-01"));
        assert!(!policy.allows_agent("db-01"));
    }

    #[test]
    fn test_has_role() {
        let identity = Identity {
            id: "test".into(),
            tenant: None,
            roles: vec![Role::Operator],
        };
        assert!(identity.has_role(&Role::Operator));
        assert!(!identity.has_role(&Role::Admin));
    }

    #[test]
    fn test_tenant_isolation_access() {
        let isolation = TenantIsolation::new(vec![
            TenantPolicy {
                tenant_id: "acme".into(),
                allowed_agents: vec!["web-01".into(), "web-02".into()],
                max_concurrent: Some(5),
            },
            TenantPolicy {
                tenant_id: "globex".into(),
                allowed_agents: vec!["db-01".into()],
                max_concurrent: None,
            },
        ]);

        let acme_user = Identity {
            id: "alice".into(),
            tenant: Some("acme".into()),
            roles: vec![Role::Operator],
        };

        // Can access own tenant's agents.
        assert!(isolation.check_access(&acme_user, "web-01").is_some());
        // Cannot access other tenant's agents.
        assert!(isolation.check_access(&acme_user, "db-01").is_none());
        // Cannot access unknown agents.
        assert!(isolation.check_access(&acme_user, "unknown").is_none());
    }

    #[test]
    fn test_tenant_isolation_no_tenant() {
        let isolation = TenantIsolation::new(vec![]);
        let no_tenant = Identity {
            id: "orphan".into(),
            tenant: None,
            roles: vec![Role::Admin],
        };
        assert!(isolation.check_access(&no_tenant, "anything").is_none());
    }

    #[test]
    fn test_same_tenant() {
        let a = Identity {
            id: "a".into(),
            tenant: Some("acme".into()),
            roles: vec![],
        };
        let b = Identity {
            id: "b".into(),
            tenant: Some("acme".into()),
            roles: vec![],
        };
        let c = Identity {
            id: "c".into(),
            tenant: Some("globex".into()),
            roles: vec![],
        };
        assert!(TenantIsolation::same_tenant(&a, &b));
        assert!(!TenantIsolation::same_tenant(&a, &c));
    }

    #[test]
    fn test_security_policy_immutable_deny() {
        let policy = SecurityPolicy::default();
        assert!(policy.is_immutably_denied("rm -rf /"));
        assert!(policy.is_immutably_denied("sudo rm -rf /"));
        assert!(policy.is_immutably_denied("mkfs.ext4 /dev/sda"));
        assert!(!policy.is_immutably_denied("ls -la"));
    }

    #[test]
    fn test_security_policy_modify_access() {
        let policy = SecurityPolicy::default();
        let admin = Identity {
            id: "admin".into(),
            tenant: None,
            roles: vec![Role::Admin],
        };
        let viewer = Identity {
            id: "viewer".into(),
            tenant: None,
            roles: vec![Role::Viewer],
        };
        assert!(policy.can_modify_policy(&admin));
        assert!(!policy.can_modify_policy(&viewer));
    }

    #[test]
    fn test_security_policy_delegation() {
        let policy = SecurityPolicy::default();
        assert!(policy.is_delegation_allowed(3));
        assert!(!policy.is_delegation_allowed(4));
    }

    #[test]
    fn test_security_policy_lifetime() {
        let policy = SecurityPolicy::default();
        assert!(policy.is_lifetime_allowed(3600));
        assert!(!policy.is_lifetime_allowed(100_000));
    }

    #[test]
    fn test_agents_for_tenant() {
        let isolation = TenantIsolation::new(vec![TenantPolicy {
            tenant_id: "acme".into(),
            allowed_agents: vec!["a1".into(), "a2".into()],
            max_concurrent: None,
        }]);
        let agents = isolation.agents_for_tenant("acme");
        assert_eq!(agents.len(), 2);
        assert!(agents.contains(&"a1"));
    }
}
