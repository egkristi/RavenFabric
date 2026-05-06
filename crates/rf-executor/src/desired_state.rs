//! Desired-state convergence engine.
//!
//! Implements declarative resource management with drift detection and remediation.
//! Resources are defined in YAML and the engine continuously reconciles actual state
//! against desired state.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use tracing::{info, warn};

/// API version for desired-state documents.
pub const API_VERSION: &str = "ravenfabric.io/v1alpha1";

/// A complete desired-state specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesiredStateSpec {
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: StateSpec,
}

/// Document metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Metadata {
    pub name: String,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub annotations: HashMap<String, String>,
}

/// The main spec body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateSpec {
    pub targets: TargetSelector,
    pub state: DesiredResources,
    #[serde(default)]
    pub convergence: ConvergenceConfig,
}

/// Target agent selection criteria.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TargetSelector {
    pub selector: LabelSelector,
}

/// Label-based selector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LabelSelector {
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

/// The desired resources to converge towards.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DesiredResources {
    #[serde(default)]
    pub packages: Vec<PackageState>,
    #[serde(default)]
    pub files: Vec<FileState>,
    #[serde(default)]
    pub services: Vec<ServiceState>,
    #[serde(default)]
    pub sysctl: Vec<SysctlState>,
}

/// Desired state for a package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackageState {
    pub name: String,
    pub state: ResourcePresence,
    #[serde(default)]
    pub version: Option<String>,
}

/// Desired state for a file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileState {
    pub path: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub state: ResourcePresence,
}

/// Desired state for a service.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceState {
    pub name: String,
    pub state: ServiceRunState,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Desired state for a sysctl key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SysctlState {
    pub key: String,
    pub value: String,
}

/// Whether a resource should be present or absent.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResourcePresence {
    Installed,
    Present,
    Absent,
}

impl Default for ResourcePresence {
    fn default() -> Self {
        Self::Present
    }
}

/// Running state of a service.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceRunState {
    Running,
    Stopped,
}

/// Convergence mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConvergenceMode {
    /// Only report drift, don't fix.
    Report,
    /// Actively remediate drift.
    Remediate,
}

impl Default for ConvergenceMode {
    fn default() -> Self {
        Self::Report
    }
}

/// Convergence configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConvergenceConfig {
    #[serde(default)]
    pub mode: ConvergenceMode,
    /// Interval between convergence checks in seconds. 0 = one-shot.
    #[serde(default = "default_interval")]
    pub interval_seconds: u64,
}

fn default_interval() -> u64 {
    300
}

impl Default for ConvergenceConfig {
    fn default() -> Self {
        Self {
            mode: ConvergenceMode::Report,
            interval_seconds: default_interval(),
        }
    }
}

/// Result of checking one resource against desired state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DriftItem {
    pub resource_type: ResourceType,
    pub resource_name: String,
    pub status: DriftStatus,
    pub detail: String,
}

/// Type of resource being checked.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    Package,
    File,
    Service,
    Sysctl,
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Package => write!(f, "package"),
            Self::File => write!(f, "file"),
            Self::Service => write!(f, "service"),
            Self::Sysctl => write!(f, "sysctl"),
        }
    }
}

/// Drift status for a single resource.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DriftStatus {
    /// Resource matches desired state.
    Converged,
    /// Resource has drifted from desired state.
    Drifted,
    /// Remediation was attempted and succeeded.
    Remediated,
    /// Remediation was attempted and failed.
    Failed,
}

/// Overall convergence report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConvergenceReport {
    pub spec_name: String,
    pub timestamp: String,
    pub mode: ConvergenceMode,
    pub items: Vec<DriftItem>,
}

impl ConvergenceReport {
    /// Returns true if all resources are converged (or successfully remediated).
    pub fn is_converged(&self) -> bool {
        self.items.iter().all(|i| {
            matches!(i.status, DriftStatus::Converged | DriftStatus::Remediated)
        })
    }

    /// Count of drifted or failed items.
    pub fn drift_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i.status, DriftStatus::Drifted | DriftStatus::Failed))
            .count()
    }
}

/// The convergence engine that evaluates desired state against actual state.
pub struct ConvergenceEngine {
    spec: DesiredStateSpec,
}

impl ConvergenceEngine {
    /// Create a new engine from a desired-state spec.
    pub fn new(spec: DesiredStateSpec) -> Self {
        Self { spec }
    }

    /// Parse a desired-state YAML document.
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        let spec: DesiredStateSpec = serde_yaml::from_str(yaml)?;
        Ok(Self::new(spec))
    }

    /// Get the spec name.
    pub fn name(&self) -> &str {
        &self.spec.metadata.name
    }

    /// Get the convergence mode.
    pub fn mode(&self) -> ConvergenceMode {
        self.spec.spec.convergence.mode
    }

    /// Get the check interval in seconds.
    pub fn interval_seconds(&self) -> u64 {
        self.spec.spec.convergence.interval_seconds
    }

    /// Check current state against desired state.
    ///
    /// Takes a `SystemProbe` that provides the actual state of the system.
    pub fn check(&self, probe: &dyn SystemProbe) -> ConvergenceReport {
        let mut items = Vec::new();

        // Check packages
        for pkg in &self.spec.spec.state.packages {
            let item = self.check_package(pkg, probe);
            items.push(item);
        }

        // Check files
        for file in &self.spec.spec.state.files {
            let item = self.check_file(file, probe);
            items.push(item);
        }

        // Check services
        for svc in &self.spec.spec.state.services {
            let item = self.check_service(svc, probe);
            items.push(item);
        }

        // Check sysctl
        for ctl in &self.spec.spec.state.sysctl {
            let item = self.check_sysctl(ctl, probe);
            items.push(item);
        }

        let report = ConvergenceReport {
            spec_name: self.spec.metadata.name.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            mode: self.spec.spec.convergence.mode,
            items,
        };

        if report.is_converged() {
            info!(spec = %self.spec.metadata.name, "all resources converged");
        } else {
            warn!(
                spec = %self.spec.metadata.name,
                drifted = report.drift_count(),
                "drift detected"
            );
        }

        report
    }

    /// Perform convergence: check + remediate if mode is Remediate.
    pub fn converge(&self, probe: &dyn SystemProbe, remediator: &dyn Remediator) -> ConvergenceReport {
        let mut report = self.check(probe);

        if self.spec.spec.convergence.mode == ConvergenceMode::Report {
            return report;
        }

        // Remediate drifted items
        for item in &mut report.items {
            if item.status == DriftStatus::Drifted {
                let result = remediator.remediate(item);
                if result {
                    item.status = DriftStatus::Remediated;
                    info!(
                        resource = %item.resource_name,
                        kind = %item.resource_type,
                        "remediated successfully"
                    );
                } else {
                    item.status = DriftStatus::Failed;
                    warn!(
                        resource = %item.resource_name,
                        kind = %item.resource_type,
                        "remediation failed"
                    );
                }
            }
        }

        report
    }

    fn check_package(&self, pkg: &PackageState, probe: &dyn SystemProbe) -> DriftItem {
        let installed = probe.is_package_installed(&pkg.name);
        let should_exist = matches!(pkg.state, ResourcePresence::Installed | ResourcePresence::Present);

        let (status, detail) = if should_exist && !installed {
            (DriftStatus::Drifted, format!("package '{}' should be installed but is not", pkg.name))
        } else if !should_exist && installed {
            (DriftStatus::Drifted, format!("package '{}' should be absent but is installed", pkg.name))
        } else if should_exist && installed {
            // Check version constraint if specified
            if let Some(ref version_constraint) = pkg.version {
                if let Some(actual_version) = probe.package_version(&pkg.name) {
                    if !version_matches(&actual_version, version_constraint) {
                        (
                            DriftStatus::Drifted,
                            format!(
                                "package '{}' version {actual_version} does not satisfy {version_constraint}",
                                pkg.name
                            ),
                        )
                    } else {
                        (DriftStatus::Converged, format!("package '{}' version {actual_version} OK", pkg.name))
                    }
                } else {
                    (DriftStatus::Converged, format!("package '{}' installed (version unknown)", pkg.name))
                }
            } else {
                (DriftStatus::Converged, format!("package '{}' installed", pkg.name))
            }
        } else {
            (DriftStatus::Converged, format!("package '{}' absent as desired", pkg.name))
        };

        DriftItem {
            resource_type: ResourceType::Package,
            resource_name: pkg.name.clone(),
            status,
            detail,
        }
    }

    fn check_file(&self, file: &FileState, probe: &dyn SystemProbe) -> DriftItem {
        let exists = probe.file_exists(&file.path);
        let should_exist = matches!(file.state, ResourcePresence::Present | ResourcePresence::Installed);

        if should_exist && !exists {
            return DriftItem {
                resource_type: ResourceType::File,
                resource_name: file.path.clone(),
                status: DriftStatus::Drifted,
                detail: format!("file '{}' should exist but does not", file.path),
            };
        }

        if !should_exist && exists {
            return DriftItem {
                resource_type: ResourceType::File,
                resource_name: file.path.clone(),
                status: DriftStatus::Drifted,
                detail: format!("file '{}' should be absent but exists", file.path),
            };
        }

        if !should_exist && !exists {
            return DriftItem {
                resource_type: ResourceType::File,
                resource_name: file.path.clone(),
                status: DriftStatus::Converged,
                detail: format!("file '{}' absent as desired", file.path),
            };
        }

        // File exists and should exist — check content/mode/owner
        if let Some(ref expected_content) = file.content {
            if let Some(actual_content) = probe.file_content(&file.path) {
                if actual_content.trim() != expected_content.trim() {
                    return DriftItem {
                        resource_type: ResourceType::File,
                        resource_name: file.path.clone(),
                        status: DriftStatus::Drifted,
                        detail: format!("file '{}' content has drifted", file.path),
                    };
                }
            }
        }

        if let Some(ref expected_mode) = file.mode {
            if let Some(actual_mode) = probe.file_mode(&file.path) {
                if actual_mode != *expected_mode {
                    return DriftItem {
                        resource_type: ResourceType::File,
                        resource_name: file.path.clone(),
                        status: DriftStatus::Drifted,
                        detail: format!("file '{}' mode is {actual_mode}, expected {expected_mode}", file.path),
                    };
                }
            }
        }

        DriftItem {
            resource_type: ResourceType::File,
            resource_name: file.path.clone(),
            status: DriftStatus::Converged,
            detail: format!("file '{}' matches desired state", file.path),
        }
    }

    fn check_service(&self, svc: &ServiceState, probe: &dyn SystemProbe) -> DriftItem {
        let is_running = probe.is_service_running(&svc.name);
        let should_run = matches!(svc.state, ServiceRunState::Running);

        if should_run && !is_running {
            return DriftItem {
                resource_type: ResourceType::Service,
                resource_name: svc.name.clone(),
                status: DriftStatus::Drifted,
                detail: format!("service '{}' should be running but is stopped", svc.name),
            };
        }

        if !should_run && is_running {
            return DriftItem {
                resource_type: ResourceType::Service,
                resource_name: svc.name.clone(),
                status: DriftStatus::Drifted,
                detail: format!("service '{}' should be stopped but is running", svc.name),
            };
        }

        // Check enabled state if specified
        if let Some(should_enable) = svc.enabled {
            let is_enabled = probe.is_service_enabled(&svc.name);
            if should_enable && !is_enabled {
                return DriftItem {
                    resource_type: ResourceType::Service,
                    resource_name: svc.name.clone(),
                    status: DriftStatus::Drifted,
                    detail: format!("service '{}' should be enabled but is not", svc.name),
                };
            }
            if !should_enable && is_enabled {
                return DriftItem {
                    resource_type: ResourceType::Service,
                    resource_name: svc.name.clone(),
                    status: DriftStatus::Drifted,
                    detail: format!("service '{}' should be disabled but is enabled", svc.name),
                };
            }
        }

        DriftItem {
            resource_type: ResourceType::Service,
            resource_name: svc.name.clone(),
            status: DriftStatus::Converged,
            detail: format!("service '{}' matches desired state", svc.name),
        }
    }

    fn check_sysctl(&self, ctl: &SysctlState, probe: &dyn SystemProbe) -> DriftItem {
        if let Some(actual) = probe.sysctl_value(&ctl.key) {
            if actual.trim() == ctl.value.trim() {
                DriftItem {
                    resource_type: ResourceType::Sysctl,
                    resource_name: ctl.key.clone(),
                    status: DriftStatus::Converged,
                    detail: format!("sysctl '{}' = '{}'", ctl.key, ctl.value),
                }
            } else {
                DriftItem {
                    resource_type: ResourceType::Sysctl,
                    resource_name: ctl.key.clone(),
                    status: DriftStatus::Drifted,
                    detail: format!(
                        "sysctl '{}' is '{actual}', expected '{}'",
                        ctl.key, ctl.value
                    ),
                }
            }
        } else {
            DriftItem {
                resource_type: ResourceType::Sysctl,
                resource_name: ctl.key.clone(),
                status: DriftStatus::Drifted,
                detail: format!("sysctl '{}' not found", ctl.key),
            }
        }
    }
}

/// Trait for probing actual system state.
pub trait SystemProbe: Send + Sync {
    fn is_package_installed(&self, name: &str) -> bool;
    fn package_version(&self, name: &str) -> Option<String>;
    fn file_exists(&self, path: &str) -> bool;
    fn file_content(&self, path: &str) -> Option<String>;
    fn file_mode(&self, path: &str) -> Option<String>;
    fn is_service_running(&self, name: &str) -> bool;
    fn is_service_enabled(&self, name: &str) -> bool;
    fn sysctl_value(&self, key: &str) -> Option<String>;
}

/// Trait for remediating drifted resources.
pub trait Remediator: Send + Sync {
    /// Attempt to remediate a drifted resource. Returns true on success.
    fn remediate(&self, item: &DriftItem) -> bool;
}

/// Simple version constraint matching.
/// Supports: exact "1.24.0", prefix ">=" / ">" / "<" / "<=".
fn version_matches(actual: &str, constraint: &str) -> bool {
    let constraint = constraint.trim();

    if let Some(min) = constraint.strip_prefix(">=") {
        version_cmp(actual, min.trim()) != std::cmp::Ordering::Less
    } else if let Some(min) = constraint.strip_prefix('>') {
        version_cmp(actual, min.trim()) == std::cmp::Ordering::Greater
    } else if let Some(max) = constraint.strip_prefix("<=") {
        version_cmp(actual, max.trim()) != std::cmp::Ordering::Greater
    } else if let Some(max) = constraint.strip_prefix('<') {
        version_cmp(actual, max.trim()) == std::cmp::Ordering::Less
    } else {
        // Exact match
        actual == constraint
    }
}

/// Simple version comparison (splits on '.' and compares numerically).
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    };
    let va = parse(a);
    let vb = parse(b);
    va.cmp(&vb)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProbe {
        packages: HashMap<String, String>,
        files: HashMap<String, (String, String)>, // path -> (content, mode)
        services: HashMap<String, (bool, bool)>,  // name -> (running, enabled)
        sysctls: HashMap<String, String>,
    }

    impl MockProbe {
        fn new() -> Self {
            Self {
                packages: HashMap::new(),
                files: HashMap::new(),
                services: HashMap::new(),
                sysctls: HashMap::new(),
            }
        }
    }

    impl SystemProbe for MockProbe {
        fn is_package_installed(&self, name: &str) -> bool {
            self.packages.contains_key(name)
        }
        fn package_version(&self, name: &str) -> Option<String> {
            self.packages.get(name).cloned()
        }
        fn file_exists(&self, path: &str) -> bool {
            self.files.contains_key(path)
        }
        fn file_content(&self, path: &str) -> Option<String> {
            self.files.get(path).map(|(c, _)| c.clone())
        }
        fn file_mode(&self, path: &str) -> Option<String> {
            self.files.get(path).map(|(_, m)| m.clone())
        }
        fn is_service_running(&self, name: &str) -> bool {
            self.services.get(name).map(|(r, _)| *r).unwrap_or(false)
        }
        fn is_service_enabled(&self, name: &str) -> bool {
            self.services.get(name).map(|(_, e)| *e).unwrap_or(false)
        }
        fn sysctl_value(&self, key: &str) -> Option<String> {
            self.sysctls.get(key).cloned()
        }
    }

    struct MockRemediator {
        succeed: bool,
    }

    impl Remediator for MockRemediator {
        fn remediate(&self, _item: &DriftItem) -> bool {
            self.succeed
        }
    }

    #[test]
    fn test_parse_desired_state_yaml() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: web-server-baseline
spec:
  targets:
    selector:
      labels:
        role: web-server
  state:
    packages:
      - name: nginx
        state: installed
        version: ">=1.24.0"
      - name: telnet
        state: absent
    files:
      - path: /etc/nginx/nginx.conf
        content: "worker_processes auto;"
        mode: "0644"
        owner: root
    services:
      - name: nginx
        state: running
        enabled: true
    sysctl:
      - key: net.ipv4.ip_forward
        value: "0"
  convergence:
    mode: remediate
    intervalSeconds: 300
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();
        assert_eq!(engine.name(), "web-server-baseline");
        assert_eq!(engine.mode(), ConvergenceMode::Remediate);
        assert_eq!(engine.interval_seconds(), 300);
    }

    #[test]
    fn test_all_converged() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: test
spec:
  targets:
    selector:
      labels: {}
  state:
    packages:
      - name: nginx
        state: installed
    services:
      - name: nginx
        state: running
        enabled: true
  convergence:
    mode: report
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();
        let mut probe = MockProbe::new();
        probe.packages.insert("nginx".into(), "1.25.0".into());
        probe.services.insert("nginx".into(), (true, true));

        let report = engine.check(&probe);
        assert!(report.is_converged());
        assert_eq!(report.drift_count(), 0);
    }

    #[test]
    fn test_package_missing_detected() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: test
spec:
  targets:
    selector:
      labels: {}
  state:
    packages:
      - name: nginx
        state: installed
  convergence:
    mode: report
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();
        let probe = MockProbe::new(); // empty — nginx not installed

        let report = engine.check(&probe);
        assert!(!report.is_converged());
        assert_eq!(report.drift_count(), 1);
        assert_eq!(report.items[0].status, DriftStatus::Drifted);
        assert!(report.items[0].detail.contains("should be installed"));
    }

    #[test]
    fn test_package_should_be_absent() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: test
spec:
  targets:
    selector:
      labels: {}
  state:
    packages:
      - name: telnet
        state: absent
  convergence:
    mode: report
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();
        let mut probe = MockProbe::new();
        probe.packages.insert("telnet".into(), "1.0".into()); // installed but should be absent

        let report = engine.check(&probe);
        assert!(!report.is_converged());
        assert!(report.items[0].detail.contains("should be absent"));
    }

    #[test]
    fn test_version_constraint() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: test
spec:
  targets:
    selector:
      labels: {}
  state:
    packages:
      - name: nginx
        state: installed
        version: ">=1.24.0"
  convergence:
    mode: report
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();

        // Version too old
        let mut probe = MockProbe::new();
        probe.packages.insert("nginx".into(), "1.20.0".into());
        let report = engine.check(&probe);
        assert!(!report.is_converged());
        assert!(report.items[0].detail.contains("does not satisfy"));

        // Version OK
        let mut probe = MockProbe::new();
        probe.packages.insert("nginx".into(), "1.25.0".into());
        let report = engine.check(&probe);
        assert!(report.is_converged());
    }

    #[test]
    fn test_file_missing() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: test
spec:
  targets:
    selector:
      labels: {}
  state:
    files:
      - path: /etc/nginx/nginx.conf
        content: "worker_processes auto;"
  convergence:
    mode: report
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();
        let probe = MockProbe::new();

        let report = engine.check(&probe);
        assert!(!report.is_converged());
        assert!(report.items[0].detail.contains("should exist but does not"));
    }

    #[test]
    fn test_file_content_drift() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: test
spec:
  targets:
    selector:
      labels: {}
  state:
    files:
      - path: /etc/nginx/nginx.conf
        content: "worker_processes auto;"
        mode: "0644"
  convergence:
    mode: report
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();
        let mut probe = MockProbe::new();
        probe.files.insert(
            "/etc/nginx/nginx.conf".into(),
            ("worker_processes 4;".into(), "0644".into()),
        );

        let report = engine.check(&probe);
        assert!(!report.is_converged());
        assert!(report.items[0].detail.contains("content has drifted"));
    }

    #[test]
    fn test_file_mode_drift() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: test
spec:
  targets:
    selector:
      labels: {}
  state:
    files:
      - path: /etc/config
        mode: "0644"
  convergence:
    mode: report
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();
        let mut probe = MockProbe::new();
        probe.files.insert("/etc/config".into(), ("data".into(), "0777".into()));

        let report = engine.check(&probe);
        assert!(!report.is_converged());
        assert!(report.items[0].detail.contains("mode is 0777"));
    }

    #[test]
    fn test_service_stopped_when_should_run() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: test
spec:
  targets:
    selector:
      labels: {}
  state:
    services:
      - name: nginx
        state: running
        enabled: true
  convergence:
    mode: report
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();
        let mut probe = MockProbe::new();
        probe.services.insert("nginx".into(), (false, false)); // stopped and disabled

        let report = engine.check(&probe);
        assert!(!report.is_converged());
        assert!(report.items[0].detail.contains("should be running but is stopped"));
    }

    #[test]
    fn test_sysctl_drift() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: test
spec:
  targets:
    selector:
      labels: {}
  state:
    sysctl:
      - key: net.ipv4.ip_forward
        value: "0"
  convergence:
    mode: report
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();
        let mut probe = MockProbe::new();
        probe.sysctls.insert("net.ipv4.ip_forward".into(), "1".into());

        let report = engine.check(&probe);
        assert!(!report.is_converged());
        assert!(report.items[0].detail.contains("is '1', expected '0'"));
    }

    #[test]
    fn test_remediate_mode_fixes_drift() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: test
spec:
  targets:
    selector:
      labels: {}
  state:
    packages:
      - name: nginx
        state: installed
  convergence:
    mode: remediate
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();
        let probe = MockProbe::new(); // nginx not installed
        let remediator = MockRemediator { succeed: true };

        let report = engine.converge(&probe, &remediator);
        assert!(report.is_converged());
        assert_eq!(report.items[0].status, DriftStatus::Remediated);
    }

    #[test]
    fn test_remediate_failure() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: test
spec:
  targets:
    selector:
      labels: {}
  state:
    packages:
      - name: nginx
        state: installed
  convergence:
    mode: remediate
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();
        let probe = MockProbe::new();
        let remediator = MockRemediator { succeed: false };

        let report = engine.converge(&probe, &remediator);
        assert!(!report.is_converged());
        assert_eq!(report.items[0].status, DriftStatus::Failed);
    }

    #[test]
    fn test_report_mode_does_not_remediate() {
        let yaml = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: test
spec:
  targets:
    selector:
      labels: {}
  state:
    packages:
      - name: nginx
        state: installed
  convergence:
    mode: report
"#;
        let engine = ConvergenceEngine::from_yaml(yaml).unwrap();
        let probe = MockProbe::new();
        let remediator = MockRemediator { succeed: true };

        let report = engine.converge(&probe, &remediator);
        // Should remain drifted — report mode doesn't fix
        assert!(!report.is_converged());
        assert_eq!(report.items[0].status, DriftStatus::Drifted);
    }

    #[test]
    fn test_version_matches_exact() {
        assert!(version_matches("1.24.0", "1.24.0"));
        assert!(!version_matches("1.24.0", "1.25.0"));
    }

    #[test]
    fn test_version_matches_gte() {
        assert!(version_matches("1.25.0", ">=1.24.0"));
        assert!(version_matches("1.24.0", ">=1.24.0"));
        assert!(!version_matches("1.23.0", ">=1.24.0"));
    }

    #[test]
    fn test_version_matches_gt() {
        assert!(version_matches("1.25.0", ">1.24.0"));
        assert!(!version_matches("1.24.0", ">1.24.0"));
    }

    #[test]
    fn test_version_matches_lt() {
        assert!(version_matches("1.23.0", "<1.24.0"));
        assert!(!version_matches("1.24.0", "<1.24.0"));
    }

    #[test]
    fn test_convergence_report_serialization() {
        let report = ConvergenceReport {
            spec_name: "test".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            mode: ConvergenceMode::Report,
            items: vec![DriftItem {
                resource_type: ResourceType::Package,
                resource_name: "nginx".into(),
                status: DriftStatus::Converged,
                detail: "installed".into(),
            }],
        };
        let json = serde_json::to_string(&report).unwrap();
        let deser: ConvergenceReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, deser);
    }
}
