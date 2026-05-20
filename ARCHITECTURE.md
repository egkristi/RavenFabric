# RavenFabric — Architecture

## Overview

RavenFabric is built as a Rust Cargo workspace with 13 focused crates. Each crate has a single responsibility and clear dependency boundaries. The architecture follows a strict layered model where higher layers depend on lower layers, never the reverse.

**Current state:** ~62,819 lines of Rust across 13 crates with 1,268 tests.

---

## Layer Model

```
┌─────────────────────────────────────────────────────┐
│  Layer 6: Interface                                 │
│  CLI · Web UI · API · Operator · SDK · MCP server   │
├─────────────────────────────────────────────────────┤
│  Layer 5: Orchestration                             │
│  ExecutionController · PlaybookEngine ·             │
│  DesiredStateEngine · SessionManager                │
├─────────────────────────────────────────────────────┤
│  Layer 4: Execution (Agent-side)                    │
│  Executor · Grains · MetricsCollector ·             │
│  ShellHandler · FileTransfer · TunnelManager ·      │
│  WASM Plugins                                       │
├─────────────────────────────────────────────────────┤
│  Layer 3: Policy (Agent — FINAL AUTHORITY)          │
│  SecurityPolicy · RPCPolicy · CollectionPolicy ·    │
│  RBAC · AuditLogger · Secrets · InjectionDetector · │
│  AnomalyDetection                                   │
├─────────────────────────────────────────────────────┤
│  Layer 2: Communication                             │
│  RPC types · msgpack codec · yamux mux ·            │
│  DTN queue · SOCKS5 · Controller API                │
├─────────────────────────────────────────────────────┤
│  Layer 1: Connectivity                              │
│  Driver trait · Registry · NAT traversal ·          │
│  ConnectionManager · Path selection ·               │
│  Mesh · Discovery · Upgrade · Health monitor        │
├─────────────────────────────────────────────────────┤
│  Layer 0: Foundation                                │
│  Noise XX · SecureChannel · StaticKey ·             │
│  PQ hybrid KEM · Sealed secrets · 0-RTT resumption  │
└─────────────────────────────────────────────────────┘
```

---

## Crate Map

| Crate | Layer | LOC | Tests | Purpose |
|-------|-------|-----|-------|---------|
| `rf-crypto` | 0 | 1,800 | 42 | Noise XX handshake, SecureChannel, key management, PQ hybrid KEM, sealed secrets, no_std frame codec |
| `rf-transport` | 1 | 21,900 | 542 | Driver trait, 30+ transport backends (WebSocket, QUIC, WireGuard, UNIX, stdio, LoRa, BLE, etc.), NAT traversal, mesh, discovery |
| `rf-rpc` | 2 | 6,300 | 112 | Request/Response types, Action enum (28+ variants), msgpack codec, yamux mux, DTN queue, controller API, Web UI |
| `rf-audit` | 3 | 650 | 14 | Structured JSON-lines audit logging, EU AI Act compliance, NIST AI RMF mapping |
| `rf-policy` | 3 | 4,700 | 105 | Deny-by-default policy, RBAC, injection detection, anomaly detection, CRDT convergence, SPIFFE identity |
| `rf-executor` | 4 | 10,100 | 167 | Command execution, desired-state convergence, event triggers, grains, PTY, log tailing, metrics, WASM plugins |
| `rf-bootstrap` | 0 | 430 | 11 | OTP enrollment, TrustStore |
| `rf-relay` | 1 | 400 | 7 | Stateless encrypted relay broker binary |
| `rf-agent` | 6 | 380 | 0 | Agent binary (connects outbound, serves RPC under policy) |
| `rf-cli` | 6 | 1,200 | 0 | CLI binary (`rf exec`, `rf shell`, `rf forward`, `rf playbook`, `rf policy`, `rf dev`) |
| `rf-mcp-server` | 6 | 3,300 | 46 | MCP server binary (AI agent integration, stdio + HTTP+SSE, 8 tools, approval workflow) |
| `rf-mcp-client` | — | 720 | 15 | MCP client SDK (Rust library, standalone, no internal deps) |
| `rf-integration-tests` | — | 1,700 | 33 | End-to-end integration tests |

---

## Dependency Graph

```
rf-crypto  (no internal deps)
  ↑
rf-transport (depends on rf-crypto)
rf-bootstrap (depends on rf-crypto)
  ↑
rf-rpc (depends on rf-crypto, rf-transport)
rf-audit (no internal deps)
rf-policy (depends on rf-audit)
  ↑
rf-executor (depends on rf-policy, rf-rpc, rf-audit, rf-crypto)
  ↑
rf-relay   (depends on rf-transport)
rf-agent   (depends on rf-crypto, rf-transport, rf-rpc, rf-executor, rf-policy, rf-audit, rf-bootstrap)
rf-cli     (depends on rf-crypto, rf-transport, rf-rpc, rf-relay, rf-executor, rf-policy, rf-audit)
rf-mcp-server (depends on rf-crypto, rf-policy, rf-audit, rf-executor, rf-rpc)
rf-mcp-client (no internal deps — standalone SDK)
```

---

## Core Traits & Types

### Transport Layer (`rf-transport`)

```rust
/// A bidirectional async stream (combined AsyncRead + AsyncWrite).
pub trait AsyncStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncStream for T {}

/// Identifies a remote endpoint.
pub struct Target {
    pub agent_id: String,
    pub relay_url: Option<String>,
    pub meet_token: Option<String>,
}

/// Driver-specific configuration.
pub type DriverConfig = HashMap<String, String>;

/// A transport driver that can establish connections over a specific protocol.
#[async_trait]
pub trait Driver: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn available(&self) -> bool;
    async fn dial(&self, target: &Target, config: &DriverConfig)
        -> Result<Box<dyn AsyncStream>, TransportError>;
}
```

### Crypto Layer (`rf-crypto`)

```rust
/// Long-lived Curve25519 identity key pair.
pub struct StaticKey {
    pub public: [u8; 32],
    private: [u8; 32],  // Never exposed, zeroed on drop via write_volatile
}

/// Established encrypted channel after Noise XX handshake.
/// Thread-safe: send and recv can be called concurrently via split Mutex.
pub struct SecureChannel<R, W> {
    reader: Mutex<ChannelReader<R>>,
    writer: Mutex<ChannelWriter<W>>,
    peer_key: [u8; 32],
}

/// Perform Noise XX handshake. Returns StatelessTransportState for concurrent use.
pub async fn handshake(
    transport: &mut T, is_initiator: bool, static_key: &StaticKey
) -> Result<(StatelessTransportState, [u8; 32]), CryptoError>;

/// Post-quantum hybrid KEM (ML-KEM + X25519 via HKDF-SHA256).
pub struct HybridKemContext { /* ... */ }

/// Sealed secret store (ChaCha20-Poly1305, keys zeroed on drop).
pub struct SecretStore { /* seal, unseal, resolve {{ secrets.KEY }} templates */ }

/// 0-RTT session resumption with replay protection.
pub struct ZeroRttCache { /* ticket store, use-count tracking, eviction */ }

/// no_std frame codec for WASM/bare-metal targets.
pub mod frame_codec {
    pub fn encrypt_frame(/* ... */) -> Result<Vec<u8>, FrameError>;
    pub fn decrypt_frame(/* ... */) -> Result<Vec<u8>, FrameError>;
}
```

### RPC Layer (`rf-rpc`)

```rust
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub action: Action,
    pub timeout_ms: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub enum Action {
    Execute { command: String, env: HashMap<String, String>, workdir: Option<String> },
    Read { path: String },
    Write { path: String, data: Vec<u8>, mode: Option<u32> },
    List { path: String },
    Metrics,
    Status,
    Signal { pid: u32, signal: i32 },
    Shell { cols: u16, rows: u16 },
    ShellInput { data: Vec<u8> },
    ShellResize { cols: u16, rows: u16 },
    ShellClose,
    PortForward { remote_host: String, remote_port: u16 },
    PortForwardData { data: Vec<u8> },
    PortForwardClose,
    RemoteForward { listen_port: u16 },
    Ping, Pong,
    Converge { spec: String },
    ConvergeReport,
    // ... 28+ variants total (streaming, background, approval, etc.)
}

#[derive(Serialize, Deserialize)]
pub enum RpcResult {
    Success { stdout: String, stderr: String, exit_code: i32, duration_ms: u64 },
    Denied { reason: String, rule: String },
    Error { message: String },
    Streaming { stream_type: StreamType },
    // + approval, forwarding, shell, status, converge variants
}
```

### Policy Layer (`rf-policy`)

```rust
pub struct RpcPolicy {
    allowed_commands: Vec<Regex>,
    denied_commands: Vec<Regex>,
    allowed_paths: Vec<PathBuf>,
    denied_paths: Vec<PathBuf>,
    pub max_output_bytes: u64,
    pub timeout_seconds: u32,
}

impl RpcPolicy {
    pub fn load(path: &Path) -> Result<Self, PolicyError>;
    pub fn from_yaml(yaml: &str) -> Result<Self, PolicyError>;
    pub fn check_command(&self, cmd: &str) -> Decision;  // deny first, then allow, default deny
    pub fn check_path(&self, path: &Path) -> Decision;   // resolves symlinks before checking
}

/// Immutable security rules that cannot be overridden by any RBAC role.
pub struct SecurityPolicy { /* immutable deny list, delegation depth, PQ requirements */ }

/// Prompt injection detection with suspicion scoring.
pub struct InjectionDetector { /* base64, hex, homoglyphs, shell evasion patterns */ }

/// Per-identity behavioral anomaly detection.
pub struct AnomalyConfig { /* velocity, novelty, timing, escalation thresholds */ }

/// CRDT-based policy convergence for distributed deployment.
pub struct PolicyCrdt { /* GSet, LwwRegister, OrSet with deny-wins semantics */ }
```

### Executor (`rf-executor`)

```rust
pub struct Executor {
    policy: Arc<RwLock<RpcPolicy>>,
    audit: Arc<dyn AuditLogger>,
    caller_key: String,
    agent_id: Option<String>,
    secrets: Option<Arc<SecretStore>>,
}

impl Executor {
    pub fn new(policy: Arc<RwLock<RpcPolicy>>, audit: Arc<dyn AuditLogger>, caller_key: String) -> Self;
    pub fn with_agent_id(self, id: String) -> Self;
    pub fn with_secrets(self, store: Arc<SecretStore>) -> Self;
    pub async fn handle(&self, request: Request) -> Response;
}

/// Desired-state convergence engine.
pub struct ConvergenceEngine { /* check, remediate, report */ }

/// Event-driven execution (cron, file watch, process exit, timer).
pub struct EventBus { /* pub/sub, trigger registration, fire */ }

/// System facts for targeting.
pub struct Grains { /* os, arch, hostname, custom labels, matches_labels() */ }
```

### Audit (`rf-audit`)

```rust
#[derive(Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub request_id: String,
    pub action: String,
    pub command: Option<String>,
    pub decision: String,       // "allowed" | "denied"
    pub matched_rule: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub caller_key: String,
    pub reason: Option<String>, // AI reasoning
}

pub trait AuditLogger: Send + Sync {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError>;
}

pub struct FileAuditLogger { /* Mutex<File>, JSON-lines append, O_APPEND */ }

/// EU AI Act + NIST AI RMF compliance reporting.
pub struct ReportGenerator { /* generate reports from audit data */ }
```

### Bootstrap (`rf-bootstrap`)

```rust
pub struct OtpStore {
    tokens: RwLock<HashMap<String, OtpEntry>>,
    ttl: Duration,
}

impl OtpStore {
    pub fn new(ttl: Duration) -> Self;
    pub fn generate(&self, agent_id: Option<String>) -> String;         // returns plaintext token
    pub fn validate_and_consume(&self, token: &str) -> Result<(), &str>; // single-use
    pub fn purge_expired(&self);
}
```

---

## Data Flow: `rf exec agent "command"`

```
1. CLI parses command, resolves agent target
2. CLI connects to relay via WebSocket (or direct if available)
3. Noise XX handshake (CLI = initiator, Agent = responder via relay pairing)
   - Wire magic: RVNF (4 bytes) + version byte validated
4. yamux session established over SecureChannel
5. CLI opens yamux stream, sends msgpack-encoded Request
6. Agent receives Request on stream
7. Agent checks RpcPolicy.check_command("command")
   - If DENIED → return Response::Denied, write audit, done
   - If ALLOWED → proceed
8. Agent resolves {{ secrets.KEY }} templates in command
9. Agent spawns process via sh -c "command"
   - Applies timeout (kill after N seconds)
   - Applies output limit (truncate after N bytes)
   - Captures stdout/stderr
10. Process completes (or times out)
11. Agent writes AuditEntry (action, decision, exit_code, duration, caller_key)
12. Agent sends Response::Success back on same yamux stream
13. CLI receives Response, formats output, exits
```

---

## Data Flow: MCP AI Agent

```
AI Agent (Claude/Cursor/Aider)
  │
  │ JSON-RPC 2.0 (stdio or HTTP+SSE)
  ▼
rf-mcp-server
  ├── Authenticate (API token, constant-time compare)
  ├── Rate limit (sliding window per session)
  ├── Parse MCP tool call (rf_exec, rf_query_policy, rf_file_read, etc.)
  ├── Check policy (deny-by-default)
  ├── Detect injection (base64, hex, homoglyphs, evasion)
  ├── Require approval if --approval-pattern matches
  │   └── SHA-256 command hash binding, one-time-use, 30-min TTL
  ├── Execute via Executor (same path as CLI)
  ├── Score behavioral anomaly (velocity, novelty, timing, escalation)
  ├── Write audit entry (includes AI reasoning if provided)
  └── Return JSON-RPC result
```

---

## Data Flow: Relay Pairing

```
Agent                         Relay                         Client
  │                             │                             │
  │── WSS CONNECT ─────────────►│                             │
  │   Header: X-Agent-Id: foo   │                             │
  │   Header: X-Token: hmac..   │                             │
  │                             │── store AgentSession ──┐    │
  │                             │                        │    │
  │                             │◄─── WSS CONNECT ───────────│
  │                             │     Header: X-Target: foo   │
  │                             │     Header: X-Token: hmac.. │
  │                             │                             │
  │                             │── pair via channel ────►    │
  │                             │                             │
  │◄════ relay copies bytes both directions (opaque) ════════►│
  │                                                           │
  │◄──── Noise XX handshake (E2E, relay sees nothing) ───────►│
  │                                                           │
  │◄──── yamux + msgpack RPC ────────────────────────────────►│
```

---

## Wire Protocol

```
Handshake:
┌─────────────────┬──────────────────────────────────────┐
│  Magic (4B)     │  "RVNF"                              │
│  Version (1B)   │  Protocol version (1 = current)      │
│  Noise msg1     │  Initiator ephemeral key             │
└─────────────────┴──────────────────────────────────────┘

Transport frames (post-handshake):
┌──────────────────┬────────────────────────────────┐
│  Length (4B BE)  │  Ciphertext + 16-byte MAC      │
│  u32 big-endian  │  ChaCha20-Poly1305 encrypted   │
└──────────────────┴────────────────────────────────┘

Constants:
- MAX_FRAME_PAYLOAD: 65,535 bytes
- FRAME_OVERHEAD: 16 bytes (Poly1305 MAC)
- NOISE_PATTERN: Noise_XX_25519_ChaChaPoly_BLAKE2s
```

---

## Configuration Files

### Agent Config (`raven.toml`)

```toml
[agent]
id = "web-01"
relay = "wss://relay.example.com/meet"
key_path = "/etc/ravenfabric/agent.key"
policy_path = "/etc/ravenfabric/policy.yaml"
audit_path = "/var/log/ravenfabric/audit.jsonl"

[transport]
driver = "websocket"
reconnect_interval = 5
max_retries = 0  # infinite
```

### Policy (`policy.yaml`)

```yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl status .*"
      - pattern: "^journalctl.*"
    deny:
      - pattern: ".*rm.*-rf.*"
  filesystem:
    allow:
      - path: /opt/app
      - path: /var/log
    deny:
      - path: /etc/shadow
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

---

## Error Handling

All errors are typed using `thiserror` in library crates. Binaries use `anyhow`. No `unwrap()` in library code (enforced via `clippy::unwrap_used = "warn"`, `unsafe_code = "forbid"` at workspace level).

```rust
// rf-crypto
pub enum CryptoError {
    Handshake(String), Encrypt(String), Decrypt(String),
    KeyFile(String), InvalidKey(String), Disconnected,
    FrameTooLarge { size: usize, max: usize },
}

// rf-transport
pub enum TransportError {
    NoDriver, Connection(String),
    Unavailable { driver: String, reason: String },
    Io(std::io::Error),
}

// rf-policy
pub enum PolicyError {
    InvalidYaml(String), InvalidRegex(String),
    FileRead(String), MissingSpec,
}

// rf-rpc
pub enum RpcError {
    Codec(String), Io(String), Timeout, ChannelClosed,
}
```

---

## Concurrency Model

- **SecureChannel**: Split reader/writer behind independent Mutexes (concurrent send/recv)
- **Executor**: `Arc<RwLock<RpcPolicy>>` allows SIGHUP hot-reload without restart
- **AuditLogger**: `Mutex<File>` for append-only writes (low contention)
- **OtpStore**: `RwLock<HashMap>` with poison handling (`unwrap_or_else(|p| p.into_inner())`)
- **Agent**: Single tokio runtime (or `current_thread` via `rt-single-thread` feature), yamux multiplexer for concurrent RPC streams
- **Relay**: One task per connection pair, no shared mutable state between sessions
- **MCP server**: Per-session state, rate limiter per session, shared policy via `Arc<RwLock>`

---

## Security Invariants

These MUST hold at all times. Violations are bugs:

1. **Agent never executes without policy check** — no code path bypasses `policy.check_command()`
2. **Relay never decrypts** — relay has no access to Noise keys, only copies opaque bytes
3. **Private key never leaves agent** — `StaticKey.private` is not serializable, zeroed on Drop via `write_volatile`
4. **Audit log is append-only** — file opened with `O_APPEND`, no delete API
5. **Denied commands produce audit entries** — denial is a security event, always logged
6. **Output is always limited** — executor truncates at `max_output_bytes`
7. **Timeouts are always enforced** — executor wraps commands in `tokio::time::timeout`
8. **Symlinks are resolved before policy check** — `check_path()` calls `canonicalize()`
9. **Policy deny rules checked before allow** — deny always wins
10. **Wire protocol rejects unknown versions** — no silent fallback to insecure mode
11. **OTP tokens are single-use** — hash-stored, consumed atomically on validation
12. **Immutable deny rules cannot be overridden** — `SecurityPolicy` deny list is checked before all other policy
13. **Tamper detection triggers transport migration** — MAC failure or injection abandons path immediately
14. **Connection metrics propagate via DTN** — no monitoring blind spots regardless of topology

---

## Workspace Configuration

```toml
# Root Cargo.toml (key settings)
[workspace.package]
version = "0.5.0"
edition = "2024"
rust-version = "1.88"
license = "AGPL-3.0-or-later"

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
unwrap_used = "warn"
pedantic = "warn"

[profile.release]
lto = true
codegen-units = 1
strip = true
panic = "abort"
```
