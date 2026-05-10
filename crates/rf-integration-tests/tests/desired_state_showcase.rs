//! Desired-state convergence showcase tests.
//!
//! Demonstrates the full desired-state lifecycle: YAML parsing, drift detection,
//! remediation, report-only mode, grains-based targeting, event triggers, and
//! version constraint matching — all using mock probes (no real system changes).

use rf_executor::desired_state::{
    ConvergenceEngine, ConvergenceMode, DriftItem, DriftStatus, Remediator, ResourceType,
    SystemProbe,
};
use rf_executor::events::{Action, EventBus, EventTrigger};
use rf_executor::grains::Grains;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// ---------------------------------------------------------------------------
// Mock SystemProbe — simulates real system state with HashMaps
// ---------------------------------------------------------------------------

struct MockProbe {
    packages: HashMap<String, Option<String>>, // name -> version (None = no version info)
    files: HashMap<String, (String, String)>,  // path -> (content, mode)
    services: HashMap<String, (bool, bool)>,   // name -> (running, enabled)
    sysctl: HashMap<String, String>,           // key -> value
}

impl MockProbe {
    fn new() -> Self {
        Self {
            packages: HashMap::new(),
            files: HashMap::new(),
            services: HashMap::new(),
            sysctl: HashMap::new(),
        }
    }

    fn with_package(mut self, name: &str, version: Option<&str>) -> Self {
        self.packages
            .insert(name.to_string(), version.map(String::from));
        self
    }

    fn with_file(mut self, path: &str, content: &str, mode: &str) -> Self {
        self.files
            .insert(path.to_string(), (content.to_string(), mode.to_string()));
        self
    }

    fn with_service(mut self, name: &str, running: bool, enabled: bool) -> Self {
        self.services.insert(name.to_string(), (running, enabled));
        self
    }

    fn with_sysctl(mut self, key: &str, value: &str) -> Self {
        self.sysctl.insert(key.to_string(), value.to_string());
        self
    }
}

impl SystemProbe for MockProbe {
    fn is_package_installed(&self, name: &str) -> bool {
        self.packages.contains_key(name)
    }
    fn package_version(&self, name: &str) -> Option<String> {
        self.packages.get(name)?.clone()
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
        self.sysctl.get(key).cloned()
    }
}

// ---------------------------------------------------------------------------
// Mock Remediator
// ---------------------------------------------------------------------------

struct MockRemediator {
    succeed: bool,
    call_count: Arc<AtomicUsize>,
}

impl MockRemediator {
    fn succeeding() -> Self {
        Self {
            succeed: true,
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn failing() -> Self {
        Self {
            succeed: false,
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

impl Remediator for MockRemediator {
    fn remediate(&self, _item: &DriftItem) -> bool {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        self.succeed
    }
}

// ---------------------------------------------------------------------------
// Shared YAML specs
// ---------------------------------------------------------------------------

const WEB_SERVER_SPEC: &str = r#"
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

const REPORT_ONLY_SPEC: &str = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: audit-baseline
spec:
  targets:
    selector:
      labels: {}
  state:
    packages:
      - name: curl
        state: installed
      - name: wget
        state: installed
    files:
      - path: /etc/motd
        content: "Managed by RavenFabric"
    services:
      - name: sshd
        state: running
        enabled: true
  convergence:
    mode: report
    intervalSeconds: 60
"#;

// ===================================================================
// Scenario 1: Drift Detection
// ===================================================================

/// Detect multiple types of drift across all resource categories.
#[test]
fn test_desired_state_drift_detection() {
    let engine = ConvergenceEngine::from_yaml(WEB_SERVER_SPEC).unwrap();

    // System has drift: nginx wrong version, telnet installed (should be absent),
    // config file has wrong content, nginx stopped, sysctl wrong value
    let probe = MockProbe::new()
        .with_package("nginx", Some("1.22.0")) // version drift: need >=1.24.0
        .with_package("telnet", None) // should be absent
        .with_file("/etc/nginx/nginx.conf", "worker_processes 1;", "0644") // content drift
        .with_service("nginx", false, true) // stopped, should be running
        .with_sysctl("net.ipv4.ip_forward", "1"); // should be "0"

    let report = engine.check(&probe);

    assert_eq!(report.spec_name, "web-server-baseline");
    assert!(!report.is_converged());
    assert_eq!(report.drift_count(), 5);

    // Verify each drift type is detected
    let drifted: Vec<_> = report
        .items
        .iter()
        .filter(|i| i.status == DriftStatus::Drifted)
        .collect();

    assert_eq!(drifted.len(), 5);

    // Package version drift
    assert!(
        drifted
            .iter()
            .any(|i| i.resource_type == ResourceType::Package && i.resource_name == "nginx")
    );
    // Package should-be-absent drift
    assert!(
        drifted
            .iter()
            .any(|i| i.resource_type == ResourceType::Package && i.resource_name == "telnet")
    );
    // File content drift
    assert!(drifted.iter().any(
        |i| i.resource_type == ResourceType::File && i.resource_name == "/etc/nginx/nginx.conf"
    ));
    // Service not running
    assert!(
        drifted
            .iter()
            .any(|i| i.resource_type == ResourceType::Service && i.resource_name == "nginx")
    );
    // Sysctl wrong value
    assert!(drifted.iter().any(
        |i| i.resource_type == ResourceType::Sysctl && i.resource_name == "net.ipv4.ip_forward"
    ));
}

/// Detect drift when a required package is completely missing.
#[test]
fn test_desired_state_missing_package() {
    let engine = ConvergenceEngine::from_yaml(WEB_SERVER_SPEC).unwrap();

    // System has no nginx at all
    let probe = MockProbe::new()
        .with_file("/etc/nginx/nginx.conf", "worker_processes auto;", "0644")
        .with_service("nginx", true, true)
        .with_sysctl("net.ipv4.ip_forward", "0");

    let report = engine.check(&probe);
    assert!(!report.is_converged());

    let nginx_pkg = report
        .items
        .iter()
        .find(|i| i.resource_type == ResourceType::Package && i.resource_name == "nginx")
        .unwrap();
    assert_eq!(nginx_pkg.status, DriftStatus::Drifted);
    assert!(nginx_pkg.detail.contains("should be installed but is not"));
}

// ===================================================================
// Scenario 2: Remediation
// ===================================================================

/// Remediate all drifted resources back to desired state.
#[test]
fn test_desired_state_remediation() {
    let engine = ConvergenceEngine::from_yaml(WEB_SERVER_SPEC).unwrap();

    // Everything is drifted
    let probe = MockProbe::new()
        .with_package("nginx", Some("1.22.0"))
        .with_package("telnet", None)
        .with_file("/etc/nginx/nginx.conf", "wrong content", "0755")
        .with_service("nginx", false, false)
        .with_sysctl("net.ipv4.ip_forward", "1");

    let remediator = MockRemediator::succeeding();
    let report = engine.converge(&probe, &remediator);

    // All drifted items should be marked as Remediated
    assert!(report.is_converged());
    assert_eq!(report.drift_count(), 0);

    let remediated: Vec<_> = report
        .items
        .iter()
        .filter(|i| i.status == DriftStatus::Remediated)
        .collect();
    assert!(!remediated.is_empty());
    assert!(remediator.count() > 0);
}

/// Remediation failure is recorded correctly.
#[test]
fn test_desired_state_remediation_failure() {
    let engine = ConvergenceEngine::from_yaml(WEB_SERVER_SPEC).unwrap();

    let probe = MockProbe::new()
        .with_package("nginx", Some("1.22.0")) // version drift
        .with_sysctl("net.ipv4.ip_forward", "0");

    let remediator = MockRemediator::failing();
    let report = engine.converge(&probe, &remediator);

    // Remediation failed — items should be Failed, not Remediated
    assert!(!report.is_converged());
    let failed: Vec<_> = report
        .items
        .iter()
        .filter(|i| i.status == DriftStatus::Failed)
        .collect();
    assert!(!failed.is_empty());
}

// ===================================================================
// Scenario 3: Report-Only Mode
// ===================================================================

/// Report mode detects drift but makes no changes.
#[test]
fn test_desired_state_report_mode() {
    let engine = ConvergenceEngine::from_yaml(REPORT_ONLY_SPEC).unwrap();

    // curl missing, motd has wrong content
    let probe = MockProbe::new()
        .with_package("wget", Some("1.21"))
        .with_file("/etc/motd", "Old MOTD", "0644")
        .with_service("sshd", true, true);

    let remediator = MockRemediator::succeeding();
    let report = engine.converge(&probe, &remediator);

    // Report mode: drift detected but remediator never called
    assert!(!report.is_converged());
    assert_eq!(remediator.count(), 0); // zero remediation calls

    // Drifted items stay Drifted (not Remediated)
    let drifted: Vec<_> = report
        .items
        .iter()
        .filter(|i| i.status == DriftStatus::Drifted)
        .collect();
    assert!(!drifted.is_empty());
}

/// Report mode with everything converged.
#[test]
fn test_desired_state_report_mode_converged() {
    let engine = ConvergenceEngine::from_yaml(REPORT_ONLY_SPEC).unwrap();

    let probe = MockProbe::new()
        .with_package("curl", Some("8.0"))
        .with_package("wget", Some("1.21"))
        .with_file("/etc/motd", "Managed by RavenFabric", "0644")
        .with_service("sshd", true, true);

    let report = engine.check(&probe);
    assert!(report.is_converged());
    assert_eq!(report.drift_count(), 0);
}

// ===================================================================
// Scenario 4: Grains-Based Targeting
// ===================================================================

/// Grains match target labels — agent should apply this spec.
#[test]
fn test_desired_state_grains_match() {
    let mut grains = Grains::new();
    grains.set("role", "web-server");
    grains.set("os", "linux");
    grains.set("arch", "x86_64");

    let labels: HashMap<String, String> = [("role".to_string(), "web-server".to_string())]
        .into_iter()
        .collect();

    assert!(grains.matches_labels(&labels));
}

/// Grains do NOT match — agent should skip this spec.
#[test]
fn test_desired_state_grains_no_match() {
    let mut grains = Grains::new();
    grains.set("role", "database");
    grains.set("os", "linux");

    let labels: HashMap<String, String> = [("role".to_string(), "web-server".to_string())]
        .into_iter()
        .collect();

    assert!(!grains.matches_labels(&labels));
}

/// Empty label selector matches all agents.
#[test]
fn test_desired_state_grains_empty_matches_all() {
    let grains = Grains::collect(); // real system grains
    let empty: HashMap<String, String> = HashMap::new();
    assert!(grains.matches_labels(&empty));
}

/// Multi-label selector requires ALL labels to match.
#[test]
fn test_desired_state_grains_multi_label() {
    let mut grains = Grains::new();
    grains.set("role", "web-server");
    grains.set("env", "production");
    grains.set("region", "eu-west-1");

    let labels: HashMap<String, String> = [
        ("role".to_string(), "web-server".to_string()),
        ("env".to_string(), "production".to_string()),
    ]
    .into_iter()
    .collect();

    assert!(grains.matches_labels(&labels));

    // Add a label that doesn't match
    let labels_mismatch: HashMap<String, String> = [
        ("role".to_string(), "web-server".to_string()),
        ("env".to_string(), "staging".to_string()),
    ]
    .into_iter()
    .collect();

    assert!(!grains.matches_labels(&labels_mismatch));
}

// ===================================================================
// Scenario 5: Event Triggers
// ===================================================================

/// Timer trigger with converge action can be registered and fired.
#[tokio::test]
async fn test_desired_state_timer_trigger() {
    let bus = EventBus::new();
    let mut rx = bus.subscribe();

    let trigger = EventTrigger::Timer {
        name: "convergence-check".to_string(),
        interval_seconds: 300,
        repeat: true,
        action: Action::Converge {
            spec: "web-server-baseline".to_string(),
        },
    };

    bus.register_trigger(trigger).await;

    let triggers = bus.list_triggers().await;
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].name(), "convergence-check");

    // Fire the trigger
    bus.fire_trigger("convergence-check", HashMap::new())
        .await
        .unwrap();

    // Verify the event was received with Converge action
    let event = rx.recv().await.unwrap();
    assert_eq!(event.trigger_name, "convergence-check");
    assert_eq!(event.trigger_type, "timer");
    match &event.action {
        Action::Converge { spec } => assert_eq!(spec, "web-server-baseline"),
        other => panic!("expected Converge action, got {other:?}"),
    }
}

/// Webhook trigger fires convergence.
#[tokio::test]
async fn test_desired_state_webhook_trigger() {
    let bus = EventBus::new();
    let mut rx = bus.subscribe();

    let trigger = EventTrigger::Webhook {
        name: "deploy-hook".to_string(),
        secret: Some("s3cret".to_string()),
        action: Action::Converge {
            spec: "deploy-baseline".to_string(),
        },
    };

    bus.register_trigger(trigger).await;
    bus.fire_trigger("deploy-hook", HashMap::new())
        .await
        .unwrap();

    let event = rx.recv().await.unwrap();
    assert_eq!(event.trigger_type, "webhook");
    match &event.action {
        Action::Converge { spec } => assert_eq!(spec, "deploy-baseline"),
        other => panic!("expected Converge, got {other:?}"),
    }
}

/// Multiple triggers can be registered and individually removed.
#[tokio::test]
async fn test_desired_state_trigger_lifecycle() {
    let bus = EventBus::new();

    bus.register_trigger(EventTrigger::Timer {
        name: "check-a".to_string(),
        interval_seconds: 60,
        repeat: true,
        action: Action::Converge {
            spec: "spec-a".to_string(),
        },
    })
    .await;

    bus.register_trigger(EventTrigger::Timer {
        name: "check-b".to_string(),
        interval_seconds: 120,
        repeat: false,
        action: Action::Converge {
            spec: "spec-b".to_string(),
        },
    })
    .await;

    assert_eq!(bus.list_triggers().await.len(), 2);

    // Remove one
    assert!(bus.remove_trigger("check-a").await);
    assert_eq!(bus.list_triggers().await.len(), 1);
    assert_eq!(bus.list_triggers().await[0].name(), "check-b");

    // Remove non-existent
    assert!(!bus.remove_trigger("nonexistent").await);
}

// ===================================================================
// Scenario 6: Version Constraints
// ===================================================================

/// Exact version match.
#[test]
fn test_desired_state_version_exact() {
    let spec = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: version-exact
spec:
  targets:
    selector:
      labels: {}
  state:
    packages:
      - name: openssl
        state: installed
        version: "3.1.0"
  convergence:
    mode: report
"#;

    let engine = ConvergenceEngine::from_yaml(spec).unwrap();

    // Exact match
    let probe = MockProbe::new().with_package("openssl", Some("3.1.0"));
    let report = engine.check(&probe);
    assert!(report.is_converged());

    // Version mismatch
    let probe = MockProbe::new().with_package("openssl", Some("3.0.9"));
    let report = engine.check(&probe);
    assert!(!report.is_converged());
}

/// Greater-than-or-equal version constraint.
#[test]
fn test_desired_state_version_gte() {
    let spec = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: version-gte
spec:
  targets:
    selector:
      labels: {}
  state:
    packages:
      - name: curl
        state: installed
        version: ">=8.0.0"
  convergence:
    mode: report
"#;

    let engine = ConvergenceEngine::from_yaml(spec).unwrap();

    // Satisfies >=8.0.0
    let probe = MockProbe::new().with_package("curl", Some("8.5.0"));
    assert!(engine.check(&probe).is_converged());

    // Exact boundary
    let probe = MockProbe::new().with_package("curl", Some("8.0.0"));
    assert!(engine.check(&probe).is_converged());

    // Below minimum
    let probe = MockProbe::new().with_package("curl", Some("7.88.1"));
    assert!(!engine.check(&probe).is_converged());
}

/// Less-than version constraint.
#[test]
fn test_desired_state_version_lt() {
    let spec = r#"
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: version-lt
spec:
  targets:
    selector:
      labels: {}
  state:
    packages:
      - name: legacy-lib
        state: installed
        version: "<2.0.0"
  convergence:
    mode: report
"#;

    let engine = ConvergenceEngine::from_yaml(spec).unwrap();

    let probe = MockProbe::new().with_package("legacy-lib", Some("1.9.9"));
    assert!(engine.check(&probe).is_converged());

    let probe = MockProbe::new().with_package("legacy-lib", Some("2.0.0"));
    assert!(!engine.check(&probe).is_converged());
}

// ===================================================================
// Full lifecycle: parse → target → check → remediate → verify
// ===================================================================

/// End-to-end: parse spec, check grains, detect drift, remediate, verify report.
#[test]
fn test_desired_state_full_lifecycle() {
    // 1. Parse spec
    let engine = ConvergenceEngine::from_yaml(WEB_SERVER_SPEC).unwrap();
    assert_eq!(engine.name(), "web-server-baseline");
    assert_eq!(engine.mode(), ConvergenceMode::Remediate);
    assert_eq!(engine.interval_seconds(), 300);

    // 2. Check grains — agent is a web-server
    let mut grains = Grains::new();
    grains.set("role", "web-server");
    let labels: HashMap<String, String> = [("role".to_string(), "web-server".to_string())]
        .into_iter()
        .collect();
    assert!(grains.matches_labels(&labels));

    // 3. Probe actual state — multiple drift items
    let probe = MockProbe::new()
        .with_package("nginx", Some("1.22.0")) // wrong version
        .with_package("telnet", None) // should be absent
        .with_file("/etc/nginx/nginx.conf", "worker_processes auto;", "0644") // OK
        .with_service("nginx", false, true) // stopped
        .with_sysctl("net.ipv4.ip_forward", "0"); // OK

    // 4. First: check-only to see drift
    let check_report = engine.check(&probe);
    assert!(!check_report.is_converged());
    assert_eq!(check_report.drift_count(), 3); // nginx version, telnet present, nginx stopped

    // 5. Converge with remediation
    let remediator = MockRemediator::succeeding();
    let converge_report = engine.converge(&probe, &remediator);
    assert!(converge_report.is_converged());
    assert_eq!(converge_report.drift_count(), 0);
    assert_eq!(remediator.count(), 3); // 3 items remediated

    // 6. Verify report is serializable (for audit logging)
    let json = serde_json::to_string_pretty(&converge_report).unwrap();
    assert!(json.contains("web-server-baseline"));
    assert!(json.contains("remediated"));
}

/// Convergence report serializes to JSON for audit logging.
#[test]
fn test_desired_state_report_json() {
    let engine = ConvergenceEngine::from_yaml(WEB_SERVER_SPEC).unwrap();

    let probe = MockProbe::new()
        .with_package("nginx", Some("1.26.0"))
        .with_file("/etc/nginx/nginx.conf", "worker_processes auto;", "0644")
        .with_service("nginx", true, true)
        .with_sysctl("net.ipv4.ip_forward", "0");

    let report = engine.check(&probe);

    // Serialize to JSON
    let json = serde_json::to_string(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["spec_name"], "web-server-baseline");
    assert_eq!(parsed["mode"], "remediate");
    assert!(parsed["items"].is_array());
    assert!(parsed["timestamp"].is_string());
}
