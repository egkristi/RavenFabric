# RavenFabric — Architecture

## Overview

RavenFabric is built as a Rust Cargo workspace with 10 focused crates. Each crate has a single responsibility and clear dependency boundaries. The architecture follows a strict layered model where higher layers depend on lower layers, never the reverse.

**Current state:** ~16,700 lines of Rust across 11 crates with 337 tests.

---

## Layer Model

```
┌─────────────────────────────────────────────────────┐
│  Layer 4: Interface                                 │
│  CLI (rf) · Agent binary · Relay binary             │
├─────────────────────────────────────────────────────┤
│  Layer 3: Execution                                 │
│  Executor · Bootstrap (OTP enrollment)              │
├─────────────────────────────────────────────────────┤
│  Layer 2: Policy + Audit                            │
│  RpcPolicy · Decision · AuditLogger                 │
├─────────────────────────────────────────────────────┤
│  Layer 1: Communication                             │
│  RPC types · Transport (Driver trait)               │
├─────────────────────────────────────────────────────┤
│  Layer 0: Foundation                                │
│  Crypto (Noise XX · SecureChannel · StaticKey)      │
└─────────────────────────────────────────────────────┘
```

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
rf-executor (depends on rf-policy, rf-rpc, rf-audit)
  ↑
rf-relay   (depends on rf-transport)
rf-agent   (depends on rf-crypto, rf-transport, rf-rpc, rf-executor, rf-policy, rf-audit, rf-bootstrap)
rf-cli     (depends on rf-crypto, rf-transport, rf-rpc)
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
    private: [u8; 32],  // Never exposed, zeroed on drop
}

impl StaticKey {
    pub fn generate() -> Self;
    pub fn load(path: &Path) -> Result<Self, CryptoError>;
    pub fn save(&self, path: &Path) -> Result<(), CryptoError>;
    pub fn load_or_generate(path: &Path) -> Result<Self, CryptoError>;
}

/// Established encrypted channel after Noise XX handshake.
/// Thread-safe: send and recv can be called concurrently.
pub struct SecureChannel<T> {
    reader: Mutex<ChannelReader<T>>,
    writer: Mutex<ChannelWriter<T>>,
    peer_key: [u8; 32],
}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> SecureChannel<T> {
    pub fn new(read_half: T, write_half: T,
               read_state: TransportState, write_state: TransportState,
               peer_key: [u8; 32]) -> Self;
    pub fn peer_key(&self) -> &[u8; 32];
    pub async fn send(&self, plaintext: &[u8]) -> Result<(), CryptoError>;
    pub async fn recv(&self) -> Result<Vec<u8>, CryptoError>;
}

/// Perform Noise XX handshake, returning TransportState + peer's public key.
pub async fn handshake<T>(transport: &mut T, is_initiator: bool, static_key: &StaticKey)
    -> Result<(TransportState, [u8; 32]), CryptoError>;
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
    Signal { pid: u32, signal: i32 },
}

#[derive(Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub result: RpcResult,
}

#[derive(Serialize, Deserialize)]
pub enum RpcResult {
    Success { stdout: String, stderr: String, exit_code: i32, duration_ms: u64 },
    Denied { reason: String, rule: String },
    Error { message: String },
}
```

### Policy Layer (`rf-policy`)

```rust
pub struct Decision {
    pub allowed: bool,
    pub reason: String,
    pub matched_rule: String,
}

pub struct RpcPolicy {
    allowed_commands: Vec<Regex>,
    denied_commands: Vec<Regex>,
    allowed_paths: Vec<PathBuf>,
    denied_paths: Vec<PathBuf>,
    pub max_output_bytes: u64,
    pub timeout_seconds: u32,
}

impl RpcPolicy {
    pub fn load(path: &Path) -> Result<Self, Box<dyn Error>>;
    pub fn from_yaml(yaml: &str) -> Result<Self, Box<dyn Error>>;
    pub fn check_command(&self, cmd: &str) -> Decision;  // deny first, then allow, default deny
    pub fn check_path(&self, path: &Path) -> Decision;   // resolves symlinks before checking
}
```

### Executor (`rf-executor`)

```rust
pub struct Executor {
    policy: Arc<RwLock<RpcPolicy>>,
    audit: Arc<dyn AuditLogger>,
    caller_key: String,
}

impl Executor {
    pub fn new(policy: Arc<RwLock<RpcPolicy>>, audit: Arc<dyn AuditLogger>, caller_key: String) -> Self;
    pub async fn handle(&self, request: Request) -> Response;
}
```

### Audit (`rf-audit`)

```rust
#[derive(Serialize)]
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
}

pub trait AuditLogger: Send + Sync {
    fn log(&self, entry: AuditEntry);
}

pub struct FileAuditLogger { /* Mutex<File>, JSON-lines append */ }
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
4. yamux session established over SecureChannel
5. CLI opens yamux stream, sends msgpack-encoded Request
6. Agent receives Request on stream
7. Agent checks RpcPolicy.check_command("command")
   - If DENIED → return Response::Denied, write audit, done
   - If ALLOWED → proceed
8. Agent spawns process via sh -c "command"
   - Applies timeout (kill after N seconds)
   - Applies output limit (truncate after N bytes)
   - Captures stdout/stderr
9. Process completes (or times out)
10. Agent writes AuditEntry (action, decision, exit_code, duration)
11. Agent sends Response::Success back on same yamux stream
12. CLI receives Response, formats output, exits
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

All errors are typed using `thiserror`. No `unwrap()` in library code.

```rust
// rf-crypto
pub enum CryptoError {
    Handshake(String),
    Encrypt(String),
    Decrypt(String),
    KeyFile(String),
    InvalidKey(String),
    Disconnected,
    FrameTooLarge { size: usize, max: usize },
}

// rf-transport
pub enum TransportError {
    NoDriver,
    Connection(String),
    Unavailable { driver: String, reason: String },
    Io(std::io::Error),
}
```

---

## Concurrency Model

- **SecureChannel**: Split reader/writer behind independent Mutexes (concurrent send/recv)
- **Executor**: `Arc<RwLock<RpcPolicy>>` allows hot-reloading policy without restart
- **AuditLogger**: `Mutex<File>` for append-only writes (low contention)
- **OtpStore**: `RwLock<HashMap>` for concurrent reads, exclusive writes on validate
- **Agent (future)**: Single tokio runtime, yamux multiplexer for concurrent RPC streams
- **Relay (future)**: One task per connection pair, no shared mutable state

---

## Security Invariants

These MUST hold at all times. Violations are bugs:

1. **Agent never executes without policy check** — no code path bypasses `policy.check_command()`
2. **Relay never decrypts** — relay has no access to Noise keys, only copies opaque bytes
3. **Private key never leaves agent** — `StaticKey.private` is not serializable, zeroed on Drop
4. **Audit log is append-only** — file opened with `O_APPEND`, no delete API
5. **Denied commands produce audit entries** — denial is a security event, always logged
6. **Output is always limited** — executor truncates at `max_output_bytes`
7. **Timeouts are always enforced** — executor wraps commands in `tokio::time::timeout`
8. **Symlinks are resolved before policy check** — `check_path()` calls `canonicalize()`
9. **Policy deny rules checked before allow** — deny always wins
10. **Wire protocol rejects unknown versions** — no silent fallback to insecure mode
