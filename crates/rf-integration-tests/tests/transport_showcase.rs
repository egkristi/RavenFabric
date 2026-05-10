//! Transport Showcase — End-to-end tests demonstrating each transport driver.
//!
//! Every test performs the identical operation over a different transport:
//!   1. Create transport (listener + dialer)
//!   2. Noise XX mutual authentication
//!   3. Establish encrypted SecureChannel
//!   4. Send RPC request → execute command → receive response
//!   5. Verify identical results regardless of transport
//!
//! This proves that the transport layer is fully interchangeable:
//! the same encryption, the same policy, the same execution —
//! only the byte-moving layer changes.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use rf_audit::logger::{AuditLogger, NullAuditLogger};
use rf_crypto::channel::SecureChannel;
use rf_crypto::keys::StaticKey;
use rf_crypto::noise::handshake;
use rf_executor::command::Executor;
use rf_policy::rpc_policy::RpcPolicy;
use rf_rpc::codec;
use rf_rpc::types::{Action, Request, Response, RpcResult};

/// Create a permissive policy + executor for testing.
fn make_executor(peer_key: [u8; 32]) -> Executor {
    let yaml = r#"
spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30
"#;
    let policy = RpcPolicy::from_yaml(yaml).unwrap();
    let policy = Arc::new(RwLock::new(policy));
    let audit: Arc<dyn AuditLogger> = Arc::new(NullAuditLogger);
    Executor::new(policy, audit, hex::encode(peer_key))
}

/// Build a standard RPC request for "echo hello-<transport>".
fn make_request(transport_name: &str) -> Request {
    Request {
        id: uuid::Uuid::new_v4().to_string(),
        action: Action::Execute {
            command: format!("echo hello-{transport_name}"),
            env: Default::default(),
            workdir: None,
        },
        timeout_ms: Some(5000),
        reason: None,
    }
}

/// Verify the response is a successful "hello-<transport>" output.
fn assert_success(response: &Response, transport_name: &str) {
    match &response.result {
        RpcResult::Success {
            stdout, exit_code, ..
        } => {
            assert_eq!(
                stdout.trim(),
                format!("hello-{transport_name}"),
                "stdout mismatch for {transport_name} transport"
            );
            assert_eq!(
                exit_code, &0,
                "non-zero exit for {transport_name} transport"
            );
        }
        other => panic!("expected Success for {transport_name}, got: {other:?}"),
    }
}

// ============================================================================
// Transport 1: WebSocket (TCP)
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_transport_websocket() {
    use rf_transport::driver::{Driver, Target};
    use rf_transport::websocket::WebSocketDriver;
    use tokio_util::sync::CancellationToken;

    let cancel = CancellationToken::new();
    let port = 19200u16;
    let token = "ws-transport-test";

    // Start relay (WebSocket listener)
    let relay_cancel = cancel.clone();
    tokio::spawn(async move {
        rf_relay::run_relay(&format!("127.0.0.1:{port}"), relay_cancel)
            .await
            .ok();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Agent side
    let agent_key = StaticKey::generate();
    let agent_key_clone = agent_key.clone();
    let agent_token = token.to_string();
    tokio::spawn(async move {
        let driver = WebSocketDriver::new();
        let target = Target {
            agent_id: "ws-agent".into(),
            relay_url: Some(format!("ws://127.0.0.1:{port}")),
            meet_token: Some(agent_token),
        };
        let mut stream = driver.dial(&target, &Default::default()).await.unwrap();
        let (state, peer_key) = handshake(&mut stream, false, &agent_key_clone)
            .await
            .unwrap();
        let (r, w) = tokio::io::split(stream);
        let chan = SecureChannel::new(r, w, state, peer_key);
        let executor = make_executor(peer_key);

        let data = chan.recv().await.unwrap();
        let request: Request = codec::decode(&data).unwrap();
        let response = executor.handle(request).await;
        chan.send(&codec::encode(&response).unwrap()).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Client side
    let client_key = StaticKey::generate();
    let driver = WebSocketDriver::new();
    let target = Target {
        agent_id: String::new(),
        relay_url: Some(format!("ws://127.0.0.1:{port}")),
        meet_token: Some(token.to_string()),
    };
    let mut stream = driver.dial(&target, &Default::default()).await.unwrap();
    let (state, peer_key) = handshake(&mut stream, true, &client_key).await.unwrap();
    let (r, w) = tokio::io::split(stream);
    let chan = SecureChannel::new(r, w, state, peer_key);

    let request = make_request("websocket");
    chan.send(&codec::encode(&request).unwrap()).await.unwrap();
    let resp_data = chan.recv().await.unwrap();
    let response: Response = codec::decode(&resp_data).unwrap();

    assert_success(&response, "websocket");
    cancel.cancel();
}

// ============================================================================
// Transport 2: UNIX Socket (IPC)
// ============================================================================

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn test_transport_unix_socket() {
    use rf_transport::driver::{Driver, Target};
    use rf_transport::unix_socket::UnixSocketDriver;

    let tmp = tempfile::TempDir::new().unwrap();
    let sock_path = tmp.path().join("transport-test.sock");
    let sock_str = sock_path.to_str().unwrap().to_string();

    let driver = UnixSocketDriver::with_path(&sock_path);
    let listener = driver.listen(&sock_str).await.unwrap();

    // Agent side — accepts connection on UNIX socket
    let agent_key = StaticKey::generate();
    let agent_key_clone = agent_key.clone();
    let agent_handle = tokio::spawn(async move {
        let mut stream = listener.accept().await.unwrap();
        let (state, peer_key) = handshake(&mut stream, false, &agent_key_clone)
            .await
            .unwrap();
        let (r, w) = tokio::io::split(stream);
        let chan = SecureChannel::new(r, w, state, peer_key);
        let executor = make_executor(peer_key);

        let data = chan.recv().await.unwrap();
        let request: Request = codec::decode(&data).unwrap();
        let response = executor.handle(request).await;
        chan.send(&codec::encode(&response).unwrap()).await.unwrap();
    });

    // Client side — dials UNIX socket
    let client_key = StaticKey::generate();
    let dial_driver = UnixSocketDriver::with_path(&sock_path);
    let target = Target {
        agent_id: "unix-agent".into(),
        relay_url: None,
        meet_token: None,
    };
    let mut config = HashMap::new();
    config.insert("socket_path".to_string(), sock_str);
    let mut stream = dial_driver.dial(&target, &config).await.unwrap();

    let (state, peer_key) = handshake(&mut stream, true, &client_key).await.unwrap();
    let (r, w) = tokio::io::split(stream);
    let chan = SecureChannel::new(r, w, state, peer_key);

    let request = make_request("unix-socket");
    chan.send(&codec::encode(&request).unwrap()).await.unwrap();
    let resp_data = chan.recv().await.unwrap();
    let response: Response = codec::decode(&resp_data).unwrap();

    assert_success(&response, "unix-socket");
    agent_handle.await.unwrap();
}

// ============================================================================
// Transport 3: Memory (In-Process Duplex)
// ============================================================================

#[tokio::test]
async fn test_transport_memory() {
    // tokio::io::duplex creates an in-memory bidirectional byte channel —
    // the simplest possible transport. Used for testing and rf dev mode.
    let (client_io, agent_io) = tokio::io::duplex(8192);

    let agent_key = StaticKey::generate();
    let agent_key_clone = agent_key.clone();

    // Agent side
    let agent_handle = tokio::spawn(async move {
        let mut stream = agent_io;
        let (state, peer_key) = handshake(&mut stream, false, &agent_key_clone)
            .await
            .unwrap();
        let (r, w) = tokio::io::split(stream);
        let chan = SecureChannel::new(r, w, state, peer_key);
        let executor = make_executor(peer_key);

        let data = chan.recv().await.unwrap();
        let request: Request = codec::decode(&data).unwrap();
        let response = executor.handle(request).await;
        chan.send(&codec::encode(&response).unwrap()).await.unwrap();
    });

    // Client side
    let client_key = StaticKey::generate();
    let mut stream = client_io;
    let (state, peer_key) = handshake(&mut stream, true, &client_key).await.unwrap();
    let (r, w) = tokio::io::split(stream);
    let chan = SecureChannel::new(r, w, state, peer_key);

    let request = make_request("memory");
    chan.send(&codec::encode(&request).unwrap()).await.unwrap();
    let resp_data = chan.recv().await.unwrap();
    let response: Response = codec::decode(&resp_data).unwrap();

    assert_success(&response, "memory");
    agent_handle.await.unwrap();
}

// ============================================================================
// Transport 4: QUIC (UDP)
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_transport_quic() {
    use rf_transport::driver::{Driver, Target};
    use rf_transport::quic::QuicDriver;

    let driver = QuicDriver::new();

    // Listen on port 0 — OS assigns a free port.
    // We use the Driver trait's listen + accept for the server side.
    // To get the bound address, we parse it from the listen call.
    let port = 19300u16; // Use a fixed port (unlikely collision in tests)
    let addr = format!("127.0.0.1:{port}");

    let listener = driver.listen(&addr).await.unwrap();

    // Agent side — accepts QUIC connection
    let agent_key = StaticKey::generate();
    let agent_key_clone = agent_key.clone();
    let agent_handle = tokio::spawn(async move {
        let mut stream = listener.accept().await.unwrap();
        let (state, peer_key) = handshake(&mut stream, false, &agent_key_clone)
            .await
            .unwrap();
        let (r, w) = tokio::io::split(stream);
        let chan = SecureChannel::new(r, w, state, peer_key);
        let executor = make_executor(peer_key);

        let data = chan.recv().await.unwrap();
        let request: Request = codec::decode(&data).unwrap();
        let response = executor.handle(request).await;
        chan.send(&codec::encode(&response).unwrap()).await.unwrap();
    });

    // Small yield to ensure listener is ready
    tokio::task::yield_now().await;

    // Client side
    let client_key = StaticKey::generate();
    let target = Target {
        agent_id: "quic-agent".into(),
        relay_url: Some(addr),
        meet_token: None,
    };
    let mut stream = driver.dial(&target, &Default::default()).await.unwrap();
    let (state, peer_key) = handshake(&mut stream, true, &client_key).await.unwrap();
    let (r, w) = tokio::io::split(stream);
    let chan = SecureChannel::new(r, w, state, peer_key);

    let request = make_request("quic");
    chan.send(&codec::encode(&request).unwrap()).await.unwrap();
    let resp_data = chan.recv().await.unwrap();
    let response: Response = codec::decode(&resp_data).unwrap();

    assert_success(&response, "quic");
    agent_handle.await.unwrap();
}

// ============================================================================
// Transport 5: Stdio Pipe (Parent-Child Process)
// ============================================================================

#[tokio::test]
async fn test_transport_stdio_pipe() {
    // Simulate stdio pipe using tokio::io::duplex (same mechanism as StdioPipe
    // but without spawning an actual child process, since we can't exec ourselves
    // in a test). The StdioPipe driver wraps stdin/stdout into an AsyncStream —
    // which is identical to a duplex pipe from the handshake layer's perspective.
    //
    // The real stdio flow: parent spawns child with piped stdin/stdout,
    // then both sides run Noise XX over the pipe. We test the same handshake
    // and encryption over an equivalent byte channel.

    let (parent_io, child_io) = tokio::io::duplex(8192);

    let agent_key = StaticKey::generate();
    let agent_key_clone = agent_key.clone();

    // "Child process" (agent) side — reads from its stdin, writes to its stdout
    let child_handle = tokio::spawn(async move {
        let mut stream = child_io;
        let (state, peer_key) = handshake(&mut stream, false, &agent_key_clone)
            .await
            .unwrap();
        let (r, w) = tokio::io::split(stream);
        let chan = SecureChannel::new(r, w, state, peer_key);
        let executor = make_executor(peer_key);

        let data = chan.recv().await.unwrap();
        let request: Request = codec::decode(&data).unwrap();
        let response = executor.handle(request).await;
        chan.send(&codec::encode(&response).unwrap()).await.unwrap();
    });

    // "Parent process" (client) side — writes to child's stdin, reads from stdout
    let client_key = StaticKey::generate();
    let mut stream = parent_io;
    let (state, peer_key) = handshake(&mut stream, true, &client_key).await.unwrap();
    let (r, w) = tokio::io::split(stream);
    let chan = SecureChannel::new(r, w, state, peer_key);

    let request = make_request("stdio-pipe");
    chan.send(&codec::encode(&request).unwrap()).await.unwrap();
    let resp_data = chan.recv().await.unwrap();
    let response: Response = codec::decode(&resp_data).unwrap();

    assert_success(&response, "stdio-pipe");
    child_handle.await.unwrap();
}
