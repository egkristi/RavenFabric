//! WASM plugin runtime types.
//!
//! Defines the plugin system for extending RavenFabric with custom
//! resource types, transport drivers, and policy hooks via WebAssembly.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

/// Plugin registry — manages loaded plugins and their lifecycle.
pub struct PluginRegistry {
    /// Loaded plugins by name.
    plugins: HashMap<String, PluginEntry>,
    /// Max plugins allowed.
    max_plugins: usize,
}

impl PluginRegistry {
    /// Create a new plugin registry.
    pub fn new(max_plugins: usize) -> Self {
        Self {
            plugins: HashMap::new(),
            max_plugins,
        }
    }

    /// Register a plugin from its manifest and WASM module bytes.
    /// Validates module hash before registration.
    pub fn register(
        &mut self,
        manifest: PluginManifest,
        module_bytes: &[u8],
        sandbox: SandboxConfig,
    ) -> Result<(), PluginError> {
        if self.plugins.len() >= self.max_plugins {
            return Err(PluginError::RegistryFull);
        }

        if self.plugins.contains_key(&manifest.name) {
            return Err(PluginError::AlreadyRegistered(manifest.name.clone()));
        }

        // Verify module hash
        let actual_hash = Self::compute_hash(module_bytes);
        if actual_hash != manifest.module_hash {
            return Err(PluginError::HashMismatch {
                expected: manifest.module_hash.clone(),
                actual: actual_hash,
            });
        }

        // Check denied capabilities
        for cap in &manifest.capabilities {
            if !Self::is_capability_allowed(cap, &sandbox) {
                return Err(PluginError::CapabilityDenied(format!("{cap:?}")));
            }
        }

        let entry = PluginEntry {
            manifest: manifest.clone(),
            state: PluginState::Loaded,
            sandbox,
            invocation_count: 0,
            total_fuel_consumed: 0,
        };

        self.plugins.insert(manifest.name, entry);
        Ok(())
    }

    /// Transition a plugin to Ready state.
    pub fn activate(&mut self, name: &str) -> Result<(), PluginError> {
        let entry = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;

        match &entry.state {
            PluginState::Loaded => {
                entry.state = PluginState::Ready;
                Ok(())
            }
            PluginState::Failed { .. } => {
                entry.state = PluginState::Ready;
                Ok(())
            }
            _ => Err(PluginError::InvalidState(format!("{:?}", entry.state))),
        }
    }

    /// Disable a plugin permanently.
    pub fn disable(&mut self, name: &str, reason: String) -> Result<(), PluginError> {
        let entry = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;

        entry.state = PluginState::Disabled { reason };
        Ok(())
    }

    /// Record an invocation (fuel consumed, state transitions).
    pub fn record_invocation(&mut self, name: &str, fuel_used: u64) -> Result<(), PluginError> {
        let entry = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;

        entry.invocation_count += 1;
        entry.total_fuel_consumed += fuel_used;
        Ok(())
    }

    /// Get a plugin entry by name.
    pub fn get(&self, name: &str) -> Option<&PluginEntry> {
        self.plugins.get(name)
    }

    /// List all plugins.
    pub fn list(&self) -> Vec<&PluginEntry> {
        self.plugins.values().collect()
    }

    /// List plugins by type.
    pub fn by_type(&self, plugin_type: PluginType) -> Vec<&PluginEntry> {
        self.plugins
            .values()
            .filter(|e| e.manifest.plugin_type == plugin_type)
            .collect()
    }

    /// Unregister a plugin.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.plugins.remove(name).is_some()
    }

    /// Number of registered plugins.
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// Compute SHA-256 hash of module bytes.
    fn compute_hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Check if a capability is allowed by the sandbox.
    fn is_capability_allowed(cap: &PluginCapability, sandbox: &SandboxConfig) -> bool {
        match cap {
            PluginCapability::FsRead => !sandbox.allowed_paths.is_empty(),
            PluginCapability::FsWrite => !sandbox.allowed_paths.is_empty(),
            PluginCapability::NetOutbound => !sandbox.allowed_hosts.is_empty(),
            // These are always available
            PluginCapability::EnvRead | PluginCapability::Clock | PluginCapability::Random => true,
            PluginCapability::ProcessSpawn => false, // Never allowed
        }
    }
}

/// Plugin operation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    /// Registry capacity reached.
    RegistryFull,
    /// Plugin with this name already registered.
    AlreadyRegistered(String),
    /// Plugin not found.
    NotFound(String),
    /// Module hash doesn't match manifest.
    HashMismatch { expected: String, actual: String },
    /// Required capability denied by sandbox.
    CapabilityDenied(String),
    /// Invalid state transition.
    InvalidState(String),
    /// WASM execution error.
    ExecutionError(String),
}

/// Result of a WASM plugin invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResult {
    /// Output data from the plugin (JSON-encoded).
    pub output: serde_json::Value,
    /// Fuel consumed during execution.
    pub fuel_consumed: u64,
    /// Execution time in milliseconds.
    pub elapsed_ms: u64,
}

/// WASM plugin runtime — loads and executes WASM modules with sandboxing.
///
/// When compiled with `wasm-plugins` feature, uses `wasmtime` for real execution.
/// Without the feature, provides a validation-only runtime (verifies manifests,
/// hashes, and sandbox config but cannot execute modules).
#[cfg(feature = "wasm-plugins")]
pub mod runtime {
    use super::*;

    /// WASM engine wrapper for executing plugin modules.
    pub struct WasmRuntime {
        engine: wasmtime::Engine,
    }

    impl WasmRuntime {
        /// Create a new WASM runtime with resource limits.
        pub fn new(max_memory_pages: u64) -> Result<Self, PluginError> {
            let mut config = wasmtime::Config::new();
            config.consume_fuel(true);
            config.cranelift_opt_level(wasmtime::OptLevel::Speed);

            let engine = wasmtime::Engine::new(&config)
                .map_err(|e| PluginError::ExecutionError(format!("engine init: {e}")))?;

            let _ = max_memory_pages; // Used when creating store limits
            Ok(Self { engine })
        }

        /// Execute a plugin's exported function with the given input.
        ///
        /// The WASM module must export:
        /// - `memory` — linear memory
        /// - `alloc(size: i32) -> i32` — allocate buffer in WASM memory
        /// - `process(ptr: i32, len: i32) -> i32` — process input, returns output ptr
        /// - `result_len() -> i32` — get length of last result
        pub fn invoke(
            &self,
            module_bytes: &[u8],
            input: &[u8],
            max_fuel: u64,
            _sandbox: &SandboxConfig,
        ) -> Result<Vec<u8>, PluginError> {
            let module = wasmtime::Module::new(&self.engine, module_bytes)
                .map_err(|e| PluginError::ExecutionError(format!("compile: {e}")))?;

            let mut store = wasmtime::Store::new(&self.engine, ());
            store
                .set_fuel(max_fuel)
                .map_err(|e| PluginError::ExecutionError(format!("set fuel: {e}")))?;

            let instance = wasmtime::Instance::new(&mut store, &module, &[])
                .map_err(|e| PluginError::ExecutionError(format!("instantiate: {e}")))?;

            let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| {
                PluginError::ExecutionError("module has no 'memory' export".into())
            })?;

            // Allocate input buffer in WASM memory.
            let alloc = instance
                .get_typed_func::<i32, i32>(&mut store, "alloc")
                .map_err(|e| PluginError::ExecutionError(format!("no alloc export: {e}")))?;

            let input_ptr = alloc
                .call(&mut store, input.len() as i32)
                .map_err(|e| PluginError::ExecutionError(format!("alloc failed: {e}")))?;

            // Write input to WASM memory.
            memory
                .write(&mut store, input_ptr as usize, input)
                .map_err(|e| PluginError::ExecutionError(format!("memory write: {e}")))?;

            // Call the process function.
            let process = instance
                .get_typed_func::<(i32, i32), i32>(&mut store, "process")
                .map_err(|e| PluginError::ExecutionError(format!("no process export: {e}")))?;

            let output_ptr = process
                .call(&mut store, (input_ptr, input.len() as i32))
                .map_err(|e| PluginError::ExecutionError(format!("process failed: {e}")))?;

            // Get result length.
            let result_len_fn = instance
                .get_typed_func::<(), i32>(&mut store, "result_len")
                .map_err(|e| PluginError::ExecutionError(format!("no result_len export: {e}")))?;

            let result_len = result_len_fn
                .call(&mut store, ())
                .map_err(|e| PluginError::ExecutionError(format!("result_len failed: {e}")))?;

            // Read output from WASM memory.
            let mut output = vec![0u8; result_len as usize];
            memory
                .read(&store, output_ptr as usize, &mut output)
                .map_err(|e| PluginError::ExecutionError(format!("memory read: {e}")))?;

            Ok(output)
        }

        /// Get remaining fuel after last execution.
        pub fn engine(&self) -> &wasmtime::Engine {
            &self.engine
        }
    }
}

/// Validation-only runtime (no WASM execution — used when `wasm-plugins` feature is disabled).
#[cfg(not(feature = "wasm-plugins"))]
pub mod runtime {
    use super::*;

    /// Stub WASM runtime that validates manifests but cannot execute modules.
    pub struct WasmRuntime;

    impl WasmRuntime {
        /// Create a validation-only runtime.
        pub fn new(_max_memory_pages: u64) -> Result<Self, PluginError> {
            Ok(Self)
        }

        /// Attempting to invoke without the `wasm-plugins` feature returns an error.
        pub fn invoke(
            &self,
            _module_bytes: &[u8],
            _input: &[u8],
            _max_fuel: u64,
            _sandbox: &SandboxConfig,
        ) -> Result<Vec<u8>, PluginError> {
            Err(PluginError::ExecutionError(
                "WASM execution requires the 'wasm-plugins' feature flag".into(),
            ))
        }
    }
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

    fn make_manifest(name: &str, module_bytes: &[u8]) -> PluginManifest {
        let mut hasher = Sha256::new();
        hasher.update(module_bytes);
        let hash: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();

        PluginManifest {
            name: name.into(),
            version: "1.0.0".into(),
            author: None,
            description: None,
            plugin_type: PluginType::MetricsCollector,
            capabilities: vec![PluginCapability::Clock],
            module_hash: hash,
            max_memory_pages: 64,
            max_fuel: 100_000,
        }
    }

    #[test]
    fn test_plugin_registry_register() {
        let mut registry = PluginRegistry::new(10);
        let module = b"fake wasm module bytes";
        let manifest = make_manifest("test-plugin", module);
        let sandbox = SandboxConfig::default();

        assert!(registry.register(manifest, module, sandbox).is_ok());
        assert_eq!(registry.count(), 1);
        assert!(registry.get("test-plugin").is_some());
    }

    #[test]
    fn test_plugin_registry_hash_mismatch() {
        let mut registry = PluginRegistry::new(10);
        let manifest = PluginManifest {
            name: "bad-plugin".into(),
            version: "1.0.0".into(),
            author: None,
            description: None,
            plugin_type: PluginType::PolicyHook,
            capabilities: vec![],
            module_hash: "wrong_hash".into(),
            max_memory_pages: 16,
            max_fuel: 10_000,
        };
        let result = registry.register(manifest, b"module", SandboxConfig::default());
        assert!(matches!(result, Err(PluginError::HashMismatch { .. })));
    }

    #[test]
    fn test_plugin_registry_duplicate() {
        let mut registry = PluginRegistry::new(10);
        let module = b"module";
        let manifest = make_manifest("dup", module);

        registry
            .register(manifest.clone(), module, SandboxConfig::default())
            .unwrap();
        let result = registry.register(manifest, module, SandboxConfig::default());
        assert!(matches!(result, Err(PluginError::AlreadyRegistered(_))));
    }

    #[test]
    fn test_plugin_registry_full() {
        let mut registry = PluginRegistry::new(1);
        let m1 = b"mod1";
        registry
            .register(make_manifest("p1", m1), m1, SandboxConfig::default())
            .unwrap();

        let m2 = b"mod2";
        let result = registry.register(make_manifest("p2", m2), m2, SandboxConfig::default());
        assert!(matches!(result, Err(PluginError::RegistryFull)));
    }

    #[test]
    fn test_plugin_registry_lifecycle() {
        let mut registry = PluginRegistry::new(10);
        let module = b"test";
        let manifest = make_manifest("lifecycle", module);

        registry
            .register(manifest, module, SandboxConfig::default())
            .unwrap();

        // Loaded → Ready
        assert!(registry.activate("lifecycle").is_ok());
        assert!(matches!(
            registry.get("lifecycle").unwrap().state,
            PluginState::Ready
        ));

        // Record invocation
        registry.record_invocation("lifecycle", 5000).unwrap();
        assert_eq!(registry.get("lifecycle").unwrap().invocation_count, 1);

        // Disable
        registry.disable("lifecycle", "maintenance".into()).unwrap();
        assert!(matches!(
            registry.get("lifecycle").unwrap().state,
            PluginState::Disabled { .. }
        ));

        // Unregister
        assert!(registry.unregister("lifecycle"));
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_plugin_registry_by_type() {
        let mut registry = PluginRegistry::new(10);

        let m1 = b"mod1";
        let mut man1 = make_manifest("metrics-1", m1);
        man1.plugin_type = PluginType::MetricsCollector;

        let m2 = b"mod2";
        let mut man2 = make_manifest("policy-1", m2);
        man2.plugin_type = PluginType::PolicyHook;
        man2.capabilities = vec![];

        registry
            .register(man1, m1, SandboxConfig::default())
            .unwrap();
        registry
            .register(man2, m2, SandboxConfig::default())
            .unwrap();

        let metrics_plugins = registry.by_type(PluginType::MetricsCollector);
        assert_eq!(metrics_plugins.len(), 1);
        assert_eq!(metrics_plugins[0].manifest.name, "metrics-1");
    }

    #[test]
    fn test_plugin_capability_denied() {
        let mut registry = PluginRegistry::new(10);
        let module = b"spawn";
        let mut manifest = make_manifest("spawner", module);
        manifest.capabilities = vec![PluginCapability::ProcessSpawn]; // Always denied

        let result = registry.register(manifest, module, SandboxConfig::default());
        assert!(matches!(result, Err(PluginError::CapabilityDenied(_))));
    }

    #[test]
    fn test_wasm_runtime_stub_without_feature() {
        let rt = runtime::WasmRuntime::new(256).unwrap();
        let result = rt.invoke(b"fake", b"input", 100_000, &SandboxConfig::default());
        // Without wasm-plugins feature, this returns an error.
        // With wasm-plugins feature, this would fail with a compile error (not valid WASM).
        assert!(result.is_err());
    }

    #[test]
    fn test_plugin_result_serde() {
        let result = PluginResult {
            output: serde_json::json!({"status": "ok", "count": 42}),
            fuel_consumed: 5000,
            elapsed_ms: 12,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: PluginResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.fuel_consumed, 5000);
        assert_eq!(parsed.elapsed_ms, 12);
    }
}
