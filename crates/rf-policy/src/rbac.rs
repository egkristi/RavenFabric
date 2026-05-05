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
        self.roles
            .iter()
            .flat_map(|role| default_permissions(role))
            .collect()
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
}
