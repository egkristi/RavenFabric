//! End-to-end integration test: relay + agent + client in a single process.
//!
//! Validates the full RPC pipeline:
//!   client → relay → agent → policy check → execute → response → client

use std::sync::Arc;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use rf_audit::logger::NullAuditLogger;
use rf_crypto::channel::SecureChannel;
use rf_crypto::keys::StaticKey;
use rf_crypto::noise::handshake;
use rf_executor::command::Executor;
use rf_policy::rpc_policy::RpcPolicy;
use rf_rpc::codec;
use rf_rpc::types::{Action, Request, Response, RpcResult};
use rf_transport::driver::{Driver, Target};
use rf_transport::websocket::WebSocketDriver;

/// Full E2E: client sends `echo hello` via relay to agent, gets response.
#[tokio::test(flavor = "multi_thread")]
async fn test_e2e_exec_through_relay() {
    let cancel = CancellationToken::new();
    let token = "integration-test-token";
    let port = 19090u16;

    // Start relay
    let relay_cancel = cancel.clone();
    let relay_handle = tokio::spawn(async move {
        rf_relay::run_relay(&format!("127.0.0.1:{port}"), relay_cancel)
            .await
            .ok();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Start agent
    let agent_key = StaticKey::generate();
    let agent_handle = {
        let agent_key = agent_key.clone();
        let token = token.to_string();
        tokio::spawn(async move {
            run_test_agent(port, &token, &agent_key).await.ok();
        })
    };

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Client
    let result = tokio::time::timeout(
        tokio::time::Duration::from_secs(10),
        run_client_exec(port, token, "echo hello"),
    )
    .await;

    let response = result
        .expect("test timed out")
        .expect("client execution failed");

    match response.result {
        RpcResult::Success {
            stdout, exit_code, ..
        } => {
            assert_eq!(stdout.trim(), "hello");
            assert_eq!(exit_code, 0);
        }
        other => panic!("expected Success, got: {other:?}"),
    }

    cancel.cancel();
    relay_handle.abort();
    agent_handle.abort();
}

/// Full E2E: policy denial flows correctly.
#[tokio::test(flavor = "multi_thread")]
async fn test_e2e_policy_denial() {
    let cancel = CancellationToken::new();
    let token = "denial-test-token";
    let port = 19091u16;

    let relay_cancel = cancel.clone();
    let relay_handle = tokio::spawn(async move {
        rf_relay::run_relay(&format!("127.0.0.1:{port}"), relay_cancel)
            .await
            .ok();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let agent_key = StaticKey::generate();
    let agent_handle = {
        let agent_key = agent_key.clone();
        let token = token.to_string();
        tokio::spawn(async move {
            run_restrictive_agent(port, &token, &agent_key).await.ok();
        })
    };

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let result = tokio::time::timeout(
        tokio::time::Duration::from_secs(10),
        run_client_exec(port, token, "rm -rf /"),
    )
    .await;

    let response = result
        .expect("test timed out")
        .expect("client execution failed");

    match response.result {
        RpcResult::Denied { .. } => {} // Expected
        other => panic!("expected Denied, got: {other:?}"),
    }

    cancel.cancel();
    relay_handle.abort();
    agent_handle.abort();
}

async fn run_client_exec(port: u16, token: &str, command: &str) -> anyhow::Result<Response> {
    let client_key = StaticKey::generate();
    let driver = WebSocketDriver::new();
    let target = Target {
        agent_id: String::new(),
        relay_url: Some(format!("ws://127.0.0.1:{port}")),
        meet_token: Some(token.to_string()),
    };

    let mut stream = driver.dial(&target, &Default::default()).await?;

    let (state, peer_key) = handshake(&mut stream, true, &client_key).await?;
    let (r, w) = tokio::io::split(stream);
    let chan = SecureChannel::new(r, w, state, peer_key);

    let request = Request {
        id: uuid::Uuid::new_v4().to_string(),
        action: Action::Execute {
            command: command.to_string(),
            env: Default::default(),
            workdir: None,
        },
        timeout_ms: Some(5000),
        reason: None,
    };

    let req_data = codec::encode(&request)?;
    chan.send(&req_data).await?;

    let resp_data = chan.recv().await?;
    let response: Response = codec::decode(&resp_data)?;
    Ok(response)
}

async fn run_test_agent(port: u16, token: &str, key: &StaticKey) -> anyhow::Result<()> {
    let yaml = r#"
spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30
"#;
    let policy = RpcPolicy::from_yaml(yaml)?;
    let policy = Arc::new(RwLock::new(policy));
    let audit: Arc<dyn rf_audit::logger::AuditLogger> = Arc::new(NullAuditLogger);

    let driver = WebSocketDriver::new();
    let target = Target {
        agent_id: "test-agent".to_string(),
        relay_url: Some(format!("ws://127.0.0.1:{port}")),
        meet_token: Some(token.to_string()),
    };

    let mut stream = driver.dial(&target, &Default::default()).await?;

    let (state, peer_key) = handshake(&mut stream, false, key).await?;
    let (r, w) = tokio::io::split(stream);
    let chan = SecureChannel::new(r, w, state, peer_key);
    let executor = Executor::new(policy, audit, hex::encode(peer_key));

    loop {
        let data = match chan.recv().await {
            Ok(d) => d,
            Err(_) => break,
        };
        let request: Request = codec::decode(&data)?;
        let response: Response = executor.handle(request).await;
        let resp_data = codec::encode(&response)?;
        chan.send(&resp_data).await?;
    }
    Ok(())
}

async fn run_restrictive_agent(port: u16, token: &str, key: &StaticKey) -> anyhow::Result<()> {
    let yaml = r#"
spec:
  commands:
    allow:
      - pattern: "^echo .*"
    deny:
      - pattern: ".*rm.*"
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30
"#;
    let policy = RpcPolicy::from_yaml(yaml)?;
    let policy = Arc::new(RwLock::new(policy));
    let audit: Arc<dyn rf_audit::logger::AuditLogger> = Arc::new(NullAuditLogger);

    let driver = WebSocketDriver::new();
    let target = Target {
        agent_id: "test-agent".to_string(),
        relay_url: Some(format!("ws://127.0.0.1:{port}")),
        meet_token: Some(token.to_string()),
    };

    let mut stream = driver.dial(&target, &Default::default()).await?;

    let (state, peer_key) = handshake(&mut stream, false, key).await?;
    let (r, w) = tokio::io::split(stream);
    let chan = SecureChannel::new(r, w, state, peer_key);
    let executor = Executor::new(policy, audit, hex::encode(peer_key));

    loop {
        let data = match chan.recv().await {
            Ok(d) => d,
            Err(_) => break,
        };
        let request: Request = codec::decode(&data)?;
        let response: Response = executor.handle(request).await;
        let resp_data = codec::encode(&response)?;
        chan.send(&resp_data).await?;
    }
    Ok(())
}
