//! Streaming command execution with incremental stdout/stderr output.
//!
//! Instead of buffering all output and returning it at once, this module
//! spawns a child process with piped stdout/stderr and sends chunks as
//! they become available via a channel.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{RwLock, mpsc};
use tokio::time::{Duration, timeout};

use rf_audit::logger::AuditLogger;
use rf_audit::types::AuditEntry;
use rf_policy::rpc_policy::RpcPolicy;
use rf_rpc::types::{Response, RpcResult, StreamType};

/// Size of read buffer for streaming output.
const CHUNK_SIZE: usize = 8192;

/// Execute a command with streaming output.
///
/// Sends `StreamChunk` messages for stdout/stderr as data arrives,
/// followed by a final `StreamEnd` when the process exits.
///
/// Returns immediately after spawning; output is delivered via `tx`.
pub async fn stream_execute(
    request_id: String,
    command: &str,
    env: &HashMap<String, String>,
    workdir: &Option<String>,
    policy: Arc<RwLock<RpcPolicy>>,
    audit: Arc<dyn AuditLogger>,
    caller_key: &str,
    tx: mpsc::Sender<Response>,
) {
    let start = Instant::now();

    // Policy check
    let pol = policy.read().await;
    let decision = pol.check_command(command);

    if !decision.allowed {
        let duration_ms = start.elapsed().as_millis() as u64;
        if let Err(e) = audit.log(AuditEntry {
            timestamp: Utc::now(),
            request_id: request_id.clone(),
            action: "stream_execute".into(),
            command: Some(command.to_string()),
            decision: "denied".into(),
            matched_rule: decision.matched_rule.clone(),
            exit_code: None,
            duration_ms,
            caller_key: caller_key.to_string(),
            reason: None,
        }) {
            tracing::error!("audit log write failed: {}", e);
        }

        let _ = tx
            .send(Response {
                id: request_id,
                result: RpcResult::Denied {
                    reason: decision.reason,
                    rule: decision.matched_rule,
                },
            })
            .await;
        return;
    }

    let timeout_secs = pol.timeout_seconds as u64;
    let max_output = pol.max_output_bytes as usize;
    drop(pol);

    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    if let Some(dir) = workdir {
        cmd.current_dir(dir);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            let _ = tx
                .send(Response {
                    id: request_id,
                    result: RpcResult::Error {
                        message: format!("spawn failed: {e}"),
                    },
                })
                .await;
            return;
        }
    };

    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");

    let req_id = request_id.clone();
    let tx_out = tx.clone();
    let stdout_handle = tokio::spawn(async move {
        let mut total = 0usize;
        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            let n = match stdout.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            total += n;
            if total > max_output {
                break;
            }
            let _ = tx_out
                .send(Response {
                    id: req_id.clone(),
                    result: RpcResult::StreamChunk {
                        stream: StreamType::Stdout,
                        data: buf[..n].to_vec(),
                    },
                })
                .await;
        }
    });

    let req_id = request_id.clone();
    let tx_err = tx.clone();
    let max_output_err = max_output;
    let stderr_handle = tokio::spawn(async move {
        let mut total = 0usize;
        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            let n = match stderr.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            total += n;
            if total > max_output_err {
                break;
            }
            let _ = tx_err
                .send(Response {
                    id: req_id.clone(),
                    result: RpcResult::StreamChunk {
                        stream: StreamType::Stderr,
                        data: buf[..n].to_vec(),
                    },
                })
                .await;
        }
    });

    // Wait for process with timeout
    let exit_code = match timeout(Duration::from_secs(timeout_secs), child.wait()).await {
        Ok(Ok(status)) => status.code().unwrap_or(-1),
        Ok(Err(_)) => -1,
        Err(_) => {
            let _ = child.kill().await;
            -1
        }
    };

    // Wait for output readers to finish
    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    let duration_ms = start.elapsed().as_millis() as u64;

    if let Err(e) = audit.log(AuditEntry {
        timestamp: Utc::now(),
        request_id: request_id.clone(),
        action: "stream_execute".into(),
        command: Some(command.to_string()),
        decision: "allowed".into(),
        matched_rule: decision.matched_rule,
        exit_code: Some(exit_code),
        duration_ms,
        caller_key: caller_key.to_string(),
        reason: None,
    }) {
        tracing::error!("audit log write failed: {}", e);
    }

    let _ = tx
        .send(Response {
            id: request_id,
            result: RpcResult::StreamEnd {
                exit_code,
                duration_ms,
            },
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_audit::logger::{AuditError, AuditLogger};
    use rf_audit::types::AuditEntry;

    struct NoopAudit;
    impl AuditLogger for NoopAudit {
        fn log(&self, _entry: AuditEntry) -> Result<(), AuditError> {
            Ok(())
        }
    }

    fn allow_all_policy() -> RpcPolicy {
        let yaml = r#"
spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 30
"#;
        RpcPolicy::from_yaml(yaml).unwrap()
    }

    fn deny_all_policy() -> RpcPolicy {
        let yaml = r#"
spec:
  commands:
    deny:
      - pattern: ".*"
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 30
"#;
        RpcPolicy::from_yaml(yaml).unwrap()
    }

    #[tokio::test]
    async fn test_stream_execute_stdout() {
        let policy = Arc::new(RwLock::new(allow_all_policy()));
        let audit: Arc<dyn AuditLogger> = Arc::new(NoopAudit);
        let (tx, mut rx) = mpsc::channel(64);

        stream_execute(
            "test-1".into(),
            "echo hello",
            &HashMap::new(),
            &None,
            policy,
            audit,
            "test-key",
            tx,
        )
        .await;

        let mut stdout_data = Vec::new();
        let mut got_end = false;

        while let Some(resp) = rx.recv().await {
            assert_eq!(resp.id, "test-1");
            match resp.result {
                RpcResult::StreamChunk { stream, data } => {
                    assert_eq!(stream, StreamType::Stdout);
                    stdout_data.extend_from_slice(&data);
                }
                RpcResult::StreamEnd {
                    exit_code,
                    duration_ms: _,
                } => {
                    assert_eq!(exit_code, 0);
                    got_end = true;
                }
                _ => panic!("unexpected result"),
            }
        }

        assert!(got_end);
        assert_eq!(String::from_utf8_lossy(&stdout_data).trim(), "hello");
    }

    #[tokio::test]
    async fn test_stream_execute_stderr() {
        let policy = Arc::new(RwLock::new(allow_all_policy()));
        let audit: Arc<dyn AuditLogger> = Arc::new(NoopAudit);
        let (tx, mut rx) = mpsc::channel(64);

        stream_execute(
            "test-2".into(),
            "echo error >&2",
            &HashMap::new(),
            &None,
            policy,
            audit,
            "test-key",
            tx,
        )
        .await;

        let mut stderr_data = Vec::new();
        let mut got_end = false;

        while let Some(resp) = rx.recv().await {
            match resp.result {
                RpcResult::StreamChunk { stream, data } => {
                    if stream == StreamType::Stderr {
                        stderr_data.extend_from_slice(&data);
                    }
                }
                RpcResult::StreamEnd { exit_code, .. } => {
                    assert_eq!(exit_code, 0);
                    got_end = true;
                }
                _ => panic!("unexpected result"),
            }
        }

        assert!(got_end);
        assert_eq!(String::from_utf8_lossy(&stderr_data).trim(), "error");
    }

    #[tokio::test]
    async fn test_stream_execute_denied() {
        let policy = Arc::new(RwLock::new(deny_all_policy()));
        let audit: Arc<dyn AuditLogger> = Arc::new(NoopAudit);
        let (tx, mut rx) = mpsc::channel(64);

        stream_execute(
            "test-3".into(),
            "echo denied",
            &HashMap::new(),
            &None,
            policy,
            audit,
            "test-key",
            tx,
        )
        .await;

        let resp = rx.recv().await.unwrap();
        assert_eq!(resp.id, "test-3");
        assert!(matches!(resp.result, RpcResult::Denied { .. }));
    }
}
