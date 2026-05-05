//! WASM plugin runtime types.
//!
//! Defines the plugin system for extending RavenFabric with custom
//! resource types, transport drivers, and policy hooks via WebAssembly.

use serde::{Deserialize, Serialize};

/// WASM plugin manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin name (unique identifier).
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Plugin author.
    pub author: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Plugin type (determines host interface).
    pub plugin_type: PluginType,
    /// Required host capabilities.
    pub capabilities: Vec<PluginCapability>,
    /// WASM module hash (SHA-256) for integrity verification.
    pub module_hash: String,
    /// Maximum memory pages (64KB each).
    pub max_memory_pages: u32,
    /// Maximum fuel/instructions per invocation.
    pub max_fuel: u64,
}

/// Plugin type determines the host interface exposed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    /// Custom resource type (extends RPC with new commands).
    ResourceType,
    /// Custom transport driver (implements Driver trait).
    TransportDriver,
    /// Policy hook (pre/post execution filter).
    PolicyHook,
    /// Metrics collector (custom metric sources).
    MetricsCollector,
    /// Audit formatter (custom audit output format).
    AuditFormatter,
}

/// Host capabilities a plugin may request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    /// Read filesystem (scoped to allowed paths).
    FsRead,
    /// Write filesystem (scoped to allowed paths).
    FsWrite,
    /// Network outbound (scoped to allowed hosts).
    NetOutbound,
    /// Access to environment variables.
    EnvRead,
    /// Spawn subprocesses (policy-checked).
    ProcessSpawn,
    /// Access system clock.
    Clock,
    /// Random number generation.
    Random,
}

/// Plugin instance state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginState {
    /// Loaded but not initialized.
    Loaded,
    /// Initialized and ready.
    Ready,
    /// Currently executing.
    Running,
    /// Execution failed, can be retried.
    Failed { error: String },
    /// Permanently disabled (e.g., hash mismatch).
    Disabled { reason: String },
}

/// Plugin sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Maximum execution time per call (ms).
    pub timeout_ms: u32,
    /// Maximum memory (bytes).
    pub max_memory_bytes: u64,
    /// Allowed filesystem paths.
    pub allowed_paths: Vec<String>,
    /// Allowed network hosts.
    pub allowed_hosts: Vec<String>,
    /// Whether the plugin can access other plugins.
    pub inter_plugin: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 5000,
            max_memory_bytes: 64 * 1024 * 1024, // 64 MB
            allowed_paths: Vec::new(),
            allowed_hosts: Vec::new(),
            inter_plugin: false,
        }
    }
}

/// Plugin registry entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    /// Manifest.
    pub manifest: PluginManifest,
    /// Current state.
    pub state: PluginState,
    /// Sandbox configuration.
    pub sandbox: SandboxConfig,
    /// Number of invocations.
    pub invocation_count: u64,
    /// Total fuel consumed.
    pub total_fuel_consumed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manifest() {
        let manifest = PluginManifest {
            name: "custom-metrics".into(),
            version: "0.1.0".into(),
            author: Some("ravenfabric".into()),
            description: Some("Custom metrics collector".into()),
            plugin_type: PluginType::MetricsCollector,
            capabilities: vec![PluginCapability::Clock, PluginCapability::NetOutbound],
            module_hash: "a".repeat(64),
            max_memory_pages: 256,
            max_fuel: 1_000_000,
        };
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("metrics_collector"));
    }

    #[test]
    fn test_plugin_types_serde() {
        let types = [
            PluginType::ResourceType,
            PluginType::TransportDriver,
            PluginType::PolicyHook,
            PluginType::MetricsCollector,
            PluginType::AuditFormatter,
        ];
        for t in &types {
            let json = serde_json::to_string(t).unwrap();
            let parsed: PluginType = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, t);
        }
    }

    #[test]
    fn test_sandbox_defaults() {
        let sandbox = SandboxConfig::default();
        assert_eq!(sandbox.timeout_ms, 5000);
        assert_eq!(sandbox.max_memory_bytes, 64 * 1024 * 1024);
        assert!(sandbox.allowed_paths.is_empty());
    }

    #[test]
    fn test_plugin_state_transitions() {
        let states = [
            PluginState::Loaded,
            PluginState::Ready,
            PluginState::Running,
            PluginState::Failed {
                error: "timeout".into(),
            },
            PluginState::Disabled {
                reason: "hash mismatch".into(),
            },
        ];
        for s in &states {
            let json = serde_json::to_string(s).unwrap();
            let parsed: PluginState = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, s);
        }
    }

    #[test]
    fn test_capabilities_deny_default() {
        // Plugin with no capabilities should have no access
        let manifest = PluginManifest {
            name: "minimal".into(),
            version: "0.1.0".into(),
            author: None,
            description: None,
            plugin_type: PluginType::PolicyHook,
            capabilities: vec![], // No capabilities requested
            module_hash: "b".repeat(64),
            max_memory_pages: 16,
            max_fuel: 10_000,
        };
        assert!(manifest.capabilities.is_empty());
    }

    #[test]
    fn test_plugin_entry() {
        let entry = PluginEntry {
            manifest: PluginManifest {
                name: "test-plugin".into(),
                version: "1.0.0".into(),
                author: None,
                description: None,
                plugin_type: PluginType::ResourceType,
                capabilities: vec![PluginCapability::FsRead],
                module_hash: "c".repeat(64),
                max_memory_pages: 64,
                max_fuel: 500_000,
            },
            state: PluginState::Ready,
            sandbox: SandboxConfig::default(),
            invocation_count: 42,
            total_fuel_consumed: 12345,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("test-plugin"));
        assert!(json.contains("42"));
    }
}
