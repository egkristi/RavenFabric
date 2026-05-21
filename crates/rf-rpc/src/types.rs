use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// RPC request sent from client/orchestrator to agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub action: Action,
    pub timeout_ms: Option<u64>,
    /// Optional reasoning from AI agent explaining why this action is needed.
    /// Recorded in audit log for compliance and forensics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The action to perform on the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Action {
    Execute {
        command: String,
        env: HashMap<String, String>,
        workdir: Option<String>,
    },
    /// Like Execute, but streams stdout/stderr incrementally over the connection.
    StreamExecute {
        command: String,
        env: HashMap<String, String>,
        workdir: Option<String>,
    },
    Read {
        path: String,
    },
    Write {
        path: String,
        data: Vec<u8>,
        mode: Option<u32>,
    },
    List {
        path: String,
    },
    Metrics,
    Signal {
        pid: u32,
        signal: i32,
    },
    /// Ping/status check — agent responds with its version and uptime.
    Status,
    /// Execute a command in the background. Returns a job ID immediately.
    BackgroundExec {
        command: String,
        env: HashMap<String, String>,
        workdir: Option<String>,
    },
    /// Query the status of a background job by its ID.
    JobQuery {
        job_id: String,
    },
    /// Wait for a background job to complete and return its output.
    JobWait {
        job_id: String,
    },
    /// Lightweight heartbeat ping — agent responds with Pong immediately.
    /// Used for liveness detection. No policy check required.
    Ping,
    /// Open an interactive shell session via PTY on the agent.
    Shell {
        shell: Option<String>,
        rows: u16,
        cols: u16,
        env: HashMap<String, String>,
    },
    /// Send data (input) to an active shell session.
    ShellInput {
        session_id: String,
        data: Vec<u8>,
    },
    /// Resize an active shell session.
    ShellResize {
        session_id: String,
        rows: u16,
        cols: u16,
    },
    /// Close an active shell session.
    ShellClose {
        session_id: String,
    },
    /// Start a local-to-remote port forward on the agent.
    PortForward {
        bind_addr: String,
        target_addr: String,
    },
    /// Stop a port forward by ID.
    PortForwardClose {
        forward_id: String,
    },
    /// Remote port forward (ssh -R equivalent): agent listens on bind_addr,
    /// and for each accepted connection, data is forwarded back to the client
    /// which connects to target_addr locally.
    RemoteForward {
        bind_addr: String,
        target_addr: String,
    },
    /// Start a SOCKS5 dynamic forward proxy on the agent.
    Socks5Forward {
        bind_addr: String,
    },
    /// Stop a SOCKS5 forward by ID.
    Socks5Close {
        forward_id: String,
    },
    /// Run a health check probe.
    HealthCheck {
        probe_type: String,
        target: String,
        timeout_ms: u64,
    },
    /// Tail a log file on the agent.
    TailLog {
        path: String,
        lines: Option<u32>,
    },
    /// Push a file chunk from client to agent (upload).
    /// Chunked transfer: client sends multiple FilePush requests for large files.
    FilePush {
        /// Destination path on agent filesystem
        path: String,
        /// Byte offset within the file (for resumable transfers)
        offset: u64,
        /// File data chunk
        data: Vec<u8>,
        /// If true, this is the final chunk — agent should finalize the file
        done: bool,
        /// Expected SHA-256 of the complete file (sent with done=true for verification)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checksum: Option<String>,
        /// File mode (permissions) to set on the final file (Unix octal, e.g. 0o644)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<u32>,
        /// If true, data is zstd-compressed; agent decompresses before writing.
        #[serde(default)]
        compress: bool,
    },
    /// Pull a file chunk from agent to client (download).
    /// Client specifies offset and max chunk size; agent responds with data.
    FilePull {
        /// Source path on agent filesystem
        path: String,
        /// Byte offset to start reading from
        offset: u64,
        /// Maximum bytes to return in this chunk
        max_chunk: u32,
        /// If true, agent compresses the response chunk with zstd.
        #[serde(default)]
        compress: bool,
    },
    /// Streaming file upload — agent receives raw bytes from the client.
    ///
    /// Protocol:
    /// 1. Client sends `FilePushStream` (this action) as a normal RPC frame.
    /// 2. Agent responds with `FileStreamReady { total_size: 0, checksum: None }`.
    /// 3. Client sends file data as raw `SecureChannel` frames (each up to 64 KB) until
    ///    exactly `total_size` bytes have been delivered.
    /// 4. Agent writes all bytes to a temp file, verifies the optional SHA-256 checksum,
    ///    and atomically renames to `path`.
    /// 5. Agent responds with `FileStreamDone { bytes_transferred, checksum_verified }`.
    /// 6. Connection returns to normal RPC mode.
    FilePushStream {
        /// Destination path on agent filesystem
        path: String,
        /// Total number of bytes the client will send (agent reads exactly this many)
        total_size: u64,
        /// Expected SHA-256 hex checksum of the complete file (verified on finalization)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checksum: Option<String>,
        /// File mode (permissions) to set on the final file (Unix octal, e.g. 0o644)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<u32>,
        /// Reserved for future use: zstd-compressed data frames
        #[serde(default)]
        compress: bool,
    },
    /// Streaming file download — agent sends raw bytes to the client.
    ///
    /// Protocol:
    /// 1. Client sends `FilePullStream` (this action) as a normal RPC frame.
    /// 2. Agent responds with `FileStreamReady { total_size, checksum }`.
    /// 3. Agent streams file data as raw `SecureChannel` frames (each up to 64 KB)
    ///    until exactly `total_size` bytes have been delivered.
    /// 4. Client reads frames and accumulates until `total_size` bytes received,
    ///    then verifies the checksum.
    /// 5. Connection returns to normal RPC mode.
    FilePullStream {
        /// Source path on agent filesystem
        path: String,
        /// Reserved for future use: zstd-compressed data frames
        #[serde(default)]
        compress: bool,
    },
    /// Open a TCP proxy connection through the agent to a target.
    /// Agent connects to target and bridges traffic over yamux stream.
    Proxy {
        /// Target address (host:port) the agent should connect to
        target: String,
        /// Idle timeout in seconds (no data flowing = connection closed). None = use policy default.
        #[serde(skip_serializing_if = "Option::is_none")]
        idle_timeout_secs: Option<u32>,
        /// Maximum connection duration in seconds (hard cap). None = use policy default.
        #[serde(skip_serializing_if = "Option::is_none")]
        max_duration_secs: Option<u32>,
    },
    /// Open a dedicated proxy tunnel on this connection.
    ///
    /// After the agent responds with `ProxyReady`, the connection switches to raw
    /// bidirectional forwarding mode — bytes flow directly between the caller and
    /// the target TCP endpoint, encrypted by the existing Noise channel.
    ///
    /// Each concurrent tunnel uses its own dedicated agent connection so that
    /// multiple tunnels can run in parallel without serialisation.
    ProxyOpen {
        /// Target address (host:port) the agent should connect to
        target: String,
        /// Idle timeout in seconds (no data flowing = connection closed). None = use policy default.
        #[serde(skip_serializing_if = "Option::is_none")]
        idle_timeout_secs: Option<u32>,
        /// Maximum connection duration in seconds (hard cap). None = use policy default.
        #[serde(skip_serializing_if = "Option::is_none")]
        max_duration_secs: Option<u32>,
    },
    /// Forward an HTTP request through the agent to an upstream target.
    /// Agent inspects method + path against HTTP policy rules before forwarding.
    HttpForward {
        /// Target base URL (e.g., "localhost:8080")
        target: String,
        /// HTTP method (GET, POST, PUT, DELETE, etc.)
        method: String,
        /// Request path (e.g., "/api/v1/users")
        path: String,
        /// Request headers
        headers: HashMap<String, String>,
        /// Request body (empty for GET/HEAD/DELETE)
        #[serde(default)]
        body: Vec<u8>,
    },
    /// Manually trigger rotation for the named secret (runs its hook, or returns error if no hook).
    RotateSecret {
        /// Name of the secret to rotate.
        name: String,
    },
    /// Configure automatic rotation for a named secret.
    SetSecretRotation {
        /// Name of the secret to configure.
        name: String,
        /// TTL in seconds — how long until the secret must be rotated.
        ttl_secs: u64,
        /// Optional shell command whose stdout becomes the new secret value.
        #[serde(skip_serializing_if = "Option::is_none")]
        hook: Option<String>,
        /// Grace period in seconds — old value remains valid for this long after rotation.
        #[serde(default)]
        grace_period_secs: u64,
        /// Optional health-check command (must exit 0 before old value is retired).
        #[serde(skip_serializing_if = "Option::is_none")]
        health_check: Option<String>,
    },
    /// Register an external secret backend on the agent (Vault, AWS, Azure, GCP, or generic HTTP).
    ///
    /// The `config` field is a JSON object whose schema depends on `backend_type`.
    /// See `rf-executor::secret_backends::build_backend` for the expected shapes.
    ConfigureSecretBackend {
        /// Unique name for this backend instance (e.g. `prod-vault`).
        name: String,
        /// Backend type: `vault`, `aws-secrets-manager`, `azure-key-vault`,
        /// `gcp-secret-manager`, or `generic-http`.
        backend_type: String,
        /// Backend-specific JSON configuration (credentials, endpoint, etc.).
        config: String,
        /// Optional periodic sync interval in seconds (0 = on-demand only).
        #[serde(default)]
        sync_interval_secs: u64,
        /// Paths to prefetch on each sync tick (empty = on-demand only).
        #[serde(default)]
        sync_paths: Vec<String>,
    },
    /// Fetch a secret from a configured external backend.
    FetchFromBackend {
        /// Name of the registered backend to query.
        backend: String,
        /// Secret path within the backend (backend-specific syntax).
        path: String,
    },
    /// Seal (push) a secret value on the agent.
    ///
    /// If the secret already exists and `grace_period_secs > 0`, the old value
    /// enters a grace period so in-flight operations using the old value continue
    /// to work during roll-over. This enables zero-downtime fleet-wide rotation:
    /// push the new value to each agent, and the previous value stays valid for
    /// `grace_period_secs` seconds before expiring.
    ///
    /// If the secret does not yet exist, it is sealed immediately (grace period
    /// is ignored for new secrets).
    ///
    /// **Sensitive** — the `value` field is transmitted over the encrypted Noise
    /// channel and never written to audit logs.
    SealSecret {
        /// Name of the secret.
        name: String,
        /// Plaintext value to seal.
        value: String,
        /// Grace period in seconds for zero-downtime rotation (default: 0).
        ///
        /// When > 0 and the secret already exists, the old value remains valid
        /// for this many seconds while in-flight requests finish.
        #[serde(default)]
        grace_period_secs: u64,
    },
    /// List the names of all secrets currently sealed on the agent.
    ///
    /// Returns only names — never values.
    ListSecrets,
    /// Query the agent for block-level checksums of an existing remote file.
    ///
    /// The client uses this to compute which blocks differ before sending a delta patch.
    /// If the file does not exist on the agent the response carries `file_missing: true`
    /// and an empty block list — the caller should fall back to a full transfer.
    ///
    /// Block size is fixed per request; both sides must use the same value.
    FileDeltaQuery {
        /// Path on the agent filesystem to inspect.
        path: String,
        /// Block size in bytes (default 262144 = 256 KB).
        #[serde(default = "default_block_size")]
        block_size: u32,
    },
    /// Apply a delta patch to a file on the agent.
    ///
    /// The client sends only the blocks that changed (as identified by comparing the
    /// checksums from `FileDeltaQuery` against the local file).  Unchanged blocks are
    /// reconstructed from the existing remote file.
    FileDeltaPatch {
        /// Destination path on the agent filesystem.
        path: String,
        /// Block size used — must match the value in the preceding `FileDeltaQuery`.
        #[serde(default = "default_block_size")]
        block_size: u32,
        /// Changed blocks.  Each patch replaces exactly one block at `offset`.
        patches: Vec<DeltaPatch>,
        /// Total size of the final file in bytes (needed to truncate the last block).
        total_size: u64,
        /// SHA-256 hex of the fully-reconstructed file (verified after assembly).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checksum: Option<String>,
        /// File mode (permissions) to set on the final file (Unix octal, e.g. 0o644).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<u32>,
    },
    /// Register this agent with an ingress server as a handler for HTTP requests.
    ///
    /// The agent sends this action to the ingress server after connecting.
    /// The ingress server stores the routing rule and will send `ReverseProxy` actions
    /// when matching HTTP requests arrive from external callers.
    IngressRegister {
        /// Agent identifier (used in audit logs and load-balancing decisions).
        agent_id: String,
        /// Upstream URL on the agent host (e.g. "http://127.0.0.1:8080").
        upstream_url: String,
        /// Optional subdomain that triggers routing to this agent (e.g. "api").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subdomain: Option<String>,
        /// Optional path prefix that triggers routing to this agent (e.g. "/api/v1").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path_prefix: Option<String>,
    },
    /// Forward an HTTP request through the agent to a local upstream service.
    ///
    /// Sent by the ingress server to a registered agent when a matching external
    /// HTTP request arrives.  The agent applies its HTTP policy, connects to the
    /// upstream, and returns a `ReverseProxyResponse`.
    ReverseProxy {
        /// HTTP method (e.g. "GET", "POST").
        method: String,
        /// Request path (e.g. "/api/users").
        path: String,
        /// Query string (without leading `?`), if present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        /// Request headers as key-value pairs.
        #[serde(default)]
        headers: Vec<(String, String)>,
        /// Request body (raw bytes).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<Vec<u8>>,
        /// Full upstream URL (e.g. "http://127.0.0.1:8080").
        upstream_url: String,
        /// Per-request timeout in milliseconds (overrides agent policy default).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
        /// Maximum response body size in bytes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_response_bytes: Option<u64>,
    },
    /// Check whether a newer agent version is available.
    ///
    /// Responds with `UpdateAvailable` (download URL + SHA-256) if an update
    /// exists, or `UpdateNotAvailable` if the agent is already current.
    CheckUpdate {
        /// Semver string of the currently-running agent binary.
        current_version: String,
    },
    /// Apply an agent self-update: download the binary at `url`, verify
    /// SHA-256, optionally verify Ed25519 signature, and exec() the new binary.
    UpdateAgent {
        /// Target version string (semver).
        version: String,
        /// HTTPS URL to download the new binary from.
        url: String,
        /// Expected SHA-256 hex digest of the downloaded binary.
        sha256: String,
        /// Optional Ed25519 signature (hex-encoded) over the SHA-256 hex digest.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ed25519_sig: Option<String>,
        /// Allow installing an older version than current (rollback).
        #[serde(default)]
        allow_downgrade: bool,
    },
    /// Pin this agent to a specific version — future `UpdateAgent` requests
    /// for a different version are rejected until the pin is cleared.
    PinVersion {
        /// Semver string of the version to pin this agent to.
        version: String,
    },
    /// Clear any active version pin, resuming normal auto-update behaviour.
    UnpinVersion,
    /// Query this agent's current version, pin status, and update window.
    GetVersionInfo,
    /// Set or clear the maintenance window for auto-updates.
    ///
    /// Format: `"HH:MM-HH:MM"` (24-hour daily window, e.g. `"02:00-04:00"`).
    /// `None` means updates are allowed at any time.
    SetUpdateWindow {
        /// Daily time window, or `None` to allow updates at any time.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window: Option<String>,
    },
}

fn default_block_size() -> u32 {
    262144
}

/// One block of a remote file described by its position and checksums.
///
/// Adler-32 is the fast rolling weak checksum; SHA-256 is the strong confirmation.
/// The client skips sending a patch for any block whose Adler-32 AND SHA-256 both
/// match the local file — matching the rsync two-stage comparison strategy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockInfo {
    /// Byte offset of the block within the file.
    pub offset: u64,
    /// Actual size of this block (last block may be smaller than block_size).
    pub size: u32,
    /// Adler-32 weak rolling checksum of the block data.
    pub adler32: u32,
    /// SHA-256 hex string of the block data.
    pub sha256_hex: String,
}

/// A single block replacement in a delta patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeltaPatch {
    /// Byte offset of the block to replace.
    pub offset: u64,
    /// New block data (must equal `block_size` except for the final block).
    pub data: Vec<u8>,
}

/// Identifies which output stream a chunk belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamType {
    Stdout,
    Stderr,
}

/// RPC response sent from agent back to client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub result: RpcResult,
}

/// The result of an RPC action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RpcResult {
    Success {
        stdout: String,
        stderr: String,
        exit_code: i32,
        duration_ms: u64,
    },
    Denied {
        reason: String,
        rule: String,
    },
    Error {
        message: String,
    },
    /// Response to a Status action.
    StatusInfo {
        agent_id: String,
        version: String,
        uptime_seconds: u64,
    },
    /// Incremental output chunk from a streaming execution.
    StreamChunk {
        stream: StreamType,
        data: Vec<u8>,
    },
    /// Final message from a streaming execution indicating process completion.
    StreamEnd {
        exit_code: i32,
        duration_ms: u64,
    },
    /// Background job was started successfully.
    JobStarted {
        job_id: String,
        pid: u32,
    },
    /// Status of a background job.
    JobStatus {
        job_id: String,
        running: bool,
        exit_code: Option<i32>,
        stdout: Option<String>,
        stderr: Option<String>,
    },
    /// Response to a Ping action — heartbeat acknowledgment.
    Pong {
        timestamp_ms: u64,
    },
    /// Shell session opened successfully.
    ShellOpened {
        session_id: String,
    },
    /// Output data from a shell session.
    ShellOutput {
        session_id: String,
        data: Vec<u8>,
    },
    /// Shell session has exited.
    ShellExited {
        session_id: String,
        exit_code: i32,
    },
    /// Port forward started successfully.
    ForwardStarted {
        forward_id: String,
        bind_addr: String,
    },
    /// Port forward stopped.
    ForwardStopped {
        forward_id: String,
    },
    /// Health check result.
    HealthCheckResult {
        success: bool,
        latency_ms: u64,
        error: Option<String>,
    },
    /// Log tail output.
    TailOutput {
        lines: Vec<String>,
        path: String,
    },
    /// Response to a FilePush action — chunk accepted.
    FileChunkAck {
        /// Byte offset after this chunk (next expected offset)
        offset: u64,
        /// True if file is finalized (all chunks received, checksum verified)
        finalized: bool,
    },
    /// Response to a FilePull action — file data chunk.
    FileChunk {
        /// Byte offset this chunk starts at
        offset: u64,
        /// File data
        data: Vec<u8>,
        /// Total file size in bytes
        total_size: u64,
        /// SHA-256 checksum of the entire file (sent with last chunk)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checksum: Option<String>,
        /// True if data is zstd-compressed (matches compress flag in FilePull request).
        #[serde(default)]
        compressed: bool,
    },
    /// Response to a Proxy action — connection established.
    ProxyConnected {
        /// Unique proxy session ID for this connection
        proxy_id: String,
        /// Effective idle timeout in seconds (applied by client)
        idle_timeout_secs: u32,
        /// Effective max duration in seconds (applied by client)
        max_duration_secs: u32,
    },
    /// Response to a ProxyOpen action — tunnel established, switching to raw forwarding mode.
    ///
    /// After this response the connection carries raw plaintext bytes (still encrypted by
    /// the Noise channel) rather than RPC frames. Both sides call `chan.send` / `chan.recv`
    /// with arbitrary byte chunks until one side closes.
    ProxyReady {
        /// Unique proxy tunnel ID (for audit logging)
        proxy_id: String,
        /// Effective idle timeout in seconds
        idle_timeout_secs: u32,
        /// Effective max duration in seconds
        max_duration_secs: u32,
    },
    /// Response to an HttpForward action — upstream HTTP response.
    HttpResponse {
        /// HTTP status code (e.g., 200, 404, 500)
        status_code: u16,
        /// Response headers
        headers: HashMap<String, String>,
        /// Response body
        body: Vec<u8>,
        /// Latency in milliseconds (time to receive full response from upstream)
        latency_ms: u64,
    },
    /// Response to a RotateSecret action — rotation completed.
    Rotated {
        /// Name of the rotated secret.
        name: String,
        /// SHA-256 hex of the new value (for audit — never the plaintext).
        new_value_hash: String,
        /// TTL seconds remaining on the new value (equals the configured TTL).
        ttl_secs: u64,
        /// Grace period seconds in effect.
        grace_period_secs: u64,
    },
    /// Response to a SetSecretRotation action — rotation config applied.
    RotationConfigured {
        /// Name of the secret whose rotation was configured.
        name: String,
        /// TTL in seconds.
        ttl_secs: u64,
    },
    /// Agent is ready for streaming file I/O (response to `FilePushStream` / `FilePullStream`).
    ///
    /// For uploads (`FilePushStream`): `total_size` is 0, `checksum` is `None`.
    /// Client should now send the raw file data frames.
    ///
    /// For downloads (`FilePullStream`): `total_size` is the file size in bytes,
    /// `checksum` is the SHA-256 hex of the complete file.
    /// Agent will immediately start sending data frames after this response.
    FileStreamReady {
        /// File size in bytes (downloads: actual size; uploads: always 0)
        total_size: u64,
        /// SHA-256 hex checksum of the complete file (downloads only)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checksum: Option<String>,
    },
    /// Streaming file upload completed.
    ///
    /// Sent by the agent after all bytes have been received and the file finalized.
    /// Not sent for downloads — the client knows the transfer is done when it has
    /// received exactly `total_size` bytes.
    FileStreamDone {
        /// Total bytes received (upload) or sent (download)
        bytes_transferred: u64,
        /// True if the SHA-256 checksum was verified successfully (uploads with checksum only)
        checksum_verified: bool,
    },
    /// Response to a `ConfigureSecretBackend` action — backend registered.
    SecretBackendConfigured {
        /// Name of the registered backend.
        name: String,
        /// Backend type identifier.
        backend_type: String,
    },
    /// Response to a `FetchFromBackend` action — secret retrieved.
    SecretFetched {
        /// The backend name queried.
        backend: String,
        /// The path that was queried.
        path: String,
        /// The secret value.
        ///
        /// **Sensitive** — callers should handle this as a secret and avoid logging.
        value: String,
    },
    /// Response to a `SealSecret` action — secret sealed successfully.
    SecretSealed {
        /// Name of the secret that was sealed.
        name: String,
        /// SHA-256 hex digest of the sealed value (for audit — never log the value itself).
        value_hash: String,
        /// True if a previously-existing secret was rotated into a grace period.
        rotated: bool,
    },
    /// Response to a `ListSecrets` action — names of all sealed secrets.
    SecretsList {
        /// Sorted list of secret names currently held in the store.
        names: Vec<String>,
    },
    /// Response to a `FileDeltaQuery` action — block-level checksums of the remote file.
    FileDeltaIndex {
        /// Block checksums; empty when `file_missing` is true.
        blocks: Vec<BlockInfo>,
        /// Total size of the remote file in bytes.
        total_size: u64,
        /// True if the file does not exist on the agent (full transfer required).
        #[serde(default)]
        file_missing: bool,
    },
    /// Response to a `FileDeltaPatch` action — patch applied successfully.
    FileDeltaApplied {
        /// Number of bytes in changed blocks that were transferred.
        bytes_transferred: u64,
        /// Number of blocks that were updated by the patch.
        blocks_changed: u32,
        /// Total number of blocks in the file.
        total_blocks: u32,
        /// True if the final SHA-256 was checked and matched.
        checksum_verified: bool,
    },
    /// Response to an `IngressRegister` action — registration confirmed.
    IngressRegistered {
        /// Agent ID as echoed back by the ingress server.
        agent_id: String,
        /// Upstream URL that was registered.
        upstream_url: String,
    },
    /// Response to a `ReverseProxy` action — HTTP response from the upstream service.
    ReverseProxyResponse {
        /// HTTP status code (e.g. 200, 404).
        status: u16,
        /// Response headers as key-value pairs.
        #[serde(default)]
        headers: Vec<(String, String)>,
        /// Response body (raw bytes).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<Vec<u8>>,
        /// Round-trip latency from agent to upstream in milliseconds.
        latency_ms: u64,
    },
    /// A newer agent version is available for download.
    UpdateAvailable {
        /// Semver string of the available version.
        version: String,
        /// HTTPS URL to download the binary.
        url: String,
        /// SHA-256 hex digest of the binary at `url`.
        sha256: String,
    },
    /// The agent is already running the latest available version.
    UpdateNotAvailable,
    /// Update was downloaded, verified, and applied — agent is restarting.
    UpdateApplied {
        /// Version that was just installed.
        version: String,
        /// True if the agent is exec()-ing the new binary (Unix).
        restarting: bool,
    },
    /// Update failed (download error, hash mismatch, or exec failure).
    UpdateFailed {
        /// Human-readable failure reason.
        reason: String,
    },
    /// Version info for this agent (response to `GetVersionInfo`).
    VersionInfo {
        /// Agent identifier.
        agent_id: String,
        /// Semver string of the currently-running binary.
        current_version: String,
        /// Version pin, if set.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pinned_version: Option<String>,
        /// Maintenance window string, if configured.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        update_window: Option<String>,
    },
    /// Version pin has been set.
    VersionPinned {
        /// The version that is now pinned.
        version: String,
    },
    /// Version pin has been cleared.
    VersionUnpinned,
    /// Update window has been configured (or cleared).
    UpdateWindowSet {
        /// New window value, or `None` if cleared.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec;

    #[test]
    fn roundtrip_execute_request() {
        let req = Request {
            id: "req-42".into(),
            action: Action::Execute {
                command: "ls -la /tmp".into(),
                env: [("FOO".into(), "bar".into())].into_iter().collect(),
                workdir: Some("/home".into()),
            },
            timeout_ms: Some(30000),
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_request_with_reason() {
        let req = Request {
            id: "ai-req-1".into(),
            action: Action::Execute {
                command: "cargo test".into(),
                env: Default::default(),
                workdir: Some("/project".into()),
            },
            timeout_ms: Some(60000),
            reason: Some("Running tests to verify the refactoring didn't break anything".into()),
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
        assert_eq!(
            decoded.reason.unwrap(),
            "Running tests to verify the refactoring didn't break anything"
        );
    }

    #[test]
    fn roundtrip_stream_execute_request() {
        let req = Request {
            id: "stream-1".into(),
            action: Action::StreamExecute {
                command: "tail -f /var/log/syslog".into(),
                env: HashMap::new(),
                workdir: None,
            },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_success_response() {
        let resp = Response {
            id: "resp-1".into(),
            result: RpcResult::Success {
                stdout: "hello world\n".into(),
                stderr: String::new(),
                exit_code: 0,
                duration_ms: 42,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_denied_response() {
        let resp = Response {
            id: "resp-2".into(),
            result: RpcResult::Denied {
                reason: "command not allowed".into(),
                rule: "deny: .*rm.*".into(),
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_stream_chunk() {
        let resp = Response {
            id: "stream-2".into(),
            result: RpcResult::StreamChunk {
                stream: StreamType::Stderr,
                data: b"error output\n".to_vec(),
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_stream_end() {
        let resp = Response {
            id: "stream-3".into(),
            result: RpcResult::StreamEnd {
                exit_code: 1,
                duration_ms: 5000,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_status_info() {
        let resp = Response {
            id: "status-1".into(),
            result: RpcResult::StatusInfo {
                agent_id: "web-01".into(),
                version: "0.1.0".into(),
                uptime_seconds: 86400,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_all_actions() {
        let actions = vec![
            Action::Read {
                path: "/etc/hosts".into(),
            },
            Action::Write {
                path: "/tmp/test".into(),
                data: vec![1, 2, 3],
                mode: Some(0o644),
            },
            Action::List {
                path: "/var/log".into(),
            },
            Action::Metrics,
            Action::Signal {
                pid: 1234,
                signal: 15,
            },
            Action::Status,
            Action::BackgroundExec {
                command: "sleep 10".into(),
                env: HashMap::new(),
                workdir: Some("/tmp".into()),
            },
            Action::JobQuery {
                job_id: "job-123".into(),
            },
            Action::JobWait {
                job_id: "job-456".into(),
            },
            Action::Ping,
            Action::Shell {
                shell: Some("/bin/bash".into()),
                rows: 24,
                cols: 80,
                env: HashMap::new(),
            },
            Action::ShellInput {
                session_id: "sess-1".into(),
                data: b"ls\n".to_vec(),
            },
            Action::ShellResize {
                session_id: "sess-1".into(),
                rows: 50,
                cols: 120,
            },
            Action::ShellClose {
                session_id: "sess-1".into(),
            },
            Action::PortForward {
                bind_addr: "127.0.0.1:8080".into(),
                target_addr: "db:5432".into(),
            },
            Action::PortForwardClose {
                forward_id: "fwd-1".into(),
            },
            Action::RemoteForward {
                bind_addr: "0.0.0.0:2222".into(),
                target_addr: "127.0.0.1:22".into(),
            },
            Action::Socks5Forward {
                bind_addr: "127.0.0.1:1080".into(),
            },
            Action::Socks5Close {
                forward_id: "socks-1".into(),
            },
            Action::HealthCheck {
                probe_type: "tcp".into(),
                target: "127.0.0.1:80".into(),
                timeout_ms: 5000,
            },
            Action::TailLog {
                path: "/var/log/app.log".into(),
                lines: Some(100),
            },
        ];
        for action in actions {
            let req = Request {
                id: "test".into(),
                action,
                timeout_ms: None,
                reason: None,
            };
            let bytes = codec::encode(&req).unwrap();
            let decoded: Request = codec::decode(&bytes).unwrap();
            assert_eq!(req, decoded);
        }
    }

    #[test]
    fn roundtrip_job_started() {
        let resp = Response {
            id: "r-1".into(),
            result: RpcResult::JobStarted {
                job_id: "job-abc".into(),
                pid: 42,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_job_status() {
        let resp = Response {
            id: "r-2".into(),
            result: RpcResult::JobStatus {
                job_id: "job-xyz".into(),
                running: false,
                exit_code: Some(0),
                stdout: Some("output".into()),
                stderr: Some(String::new()),
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_pong() {
        let resp = Response {
            id: "ping-1".into(),
            result: RpcResult::Pong {
                timestamp_ms: 1714900000000,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_file_push() {
        let req = Request {
            id: "fp-1".into(),
            action: Action::FilePush {
                path: "/tmp/test.bin".into(),
                offset: 0,
                data: vec![0xDE, 0xAD, 0xBE, 0xEF],
                done: false,
                checksum: None,
                mode: Some(0o644),
                compress: false,
            },
            timeout_ms: Some(30000),
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_file_pull() {
        let req = Request {
            id: "fp-2".into(),
            action: Action::FilePull {
                path: "/var/log/syslog".into(),
                offset: 1024,
                max_chunk: 65536,
                compress: false,
            },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_file_chunk_ack() {
        let resp = Response {
            id: "fp-1".into(),
            result: RpcResult::FileChunkAck {
                offset: 4,
                finalized: true,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_file_chunk() {
        let resp = Response {
            id: "fp-2".into(),
            result: RpcResult::FileChunk {
                offset: 0,
                data: vec![1, 2, 3, 4, 5],
                total_size: 100,
                checksum: Some("abc123".into()),
                compressed: false,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_proxy() {
        let req = Request {
            id: "px-1".into(),
            action: Action::Proxy {
                target: "10.0.0.5:5432".into(),
                idle_timeout_secs: Some(60),
                max_duration_secs: None,
            },
            timeout_ms: Some(10000),
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);

        let resp = Response {
            id: "px-1".into(),
            result: RpcResult::ProxyConnected {
                proxy_id: "proxy-px-1".into(),
                idle_timeout_secs: 60,
                max_duration_secs: 3600,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_proxy_open_and_ready() {
        let req = Request {
            id: "po-1".into(),
            action: Action::ProxyOpen {
                target: "10.0.0.5:5432".into(),
                idle_timeout_secs: Some(30),
                max_duration_secs: Some(1800),
            },
            timeout_ms: Some(10000),
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);

        let resp = Response {
            id: "po-1".into(),
            result: RpcResult::ProxyReady {
                proxy_id: "tunnel-abc".into(),
                idle_timeout_secs: 30,
                max_duration_secs: 1800,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_file_push_stream() {
        let req = Request {
            id: "fps-1".into(),
            action: Action::FilePushStream {
                path: "/opt/app/binary".into(),
                total_size: 1048576,
                checksum: Some("deadbeef01234567".into()),
                mode: Some(0o755),
                compress: false,
            },
            timeout_ms: Some(120000),
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_file_pull_stream() {
        let req = Request {
            id: "fpl-1".into(),
            action: Action::FilePullStream {
                path: "/var/log/app.log".into(),
                compress: false,
            },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_file_stream_ready() {
        // Upload variant (total_size=0, no checksum)
        let resp_upload = Response {
            id: "fps-1".into(),
            result: RpcResult::FileStreamReady {
                total_size: 0,
                checksum: None,
            },
        };
        let bytes = codec::encode(&resp_upload).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp_upload, decoded);

        // Download variant (total_size + checksum)
        let resp_download = Response {
            id: "fpl-1".into(),
            result: RpcResult::FileStreamReady {
                total_size: 2048,
                checksum: Some("abc123def456".into()),
            },
        };
        let bytes = codec::encode(&resp_download).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp_download, decoded);
    }

    #[test]
    fn roundtrip_file_stream_done() {
        let resp = Response {
            id: "fps-1".into(),
            result: RpcResult::FileStreamDone {
                bytes_transferred: 1048576,
                checksum_verified: true,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_seal_secret_action() {
        let req = Request {
            id: "ss-1".into(),
            action: Action::SealSecret {
                name: "db_password".into(),
                value: "s3cr3t!".into(),
                grace_period_secs: 30,
            },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_seal_secret_result() {
        let resp = Response {
            id: "ss-1".into(),
            result: RpcResult::SecretSealed {
                name: "db_password".into(),
                value_hash: "a1b2c3d4e5f6a7b8".into(),
                rotated: true,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_list_secrets_action() {
        let req = Request {
            id: "ls-1".into(),
            action: Action::ListSecrets,
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_secrets_list_result() {
        let resp = Response {
            id: "ls-1".into(),
            result: RpcResult::SecretsList {
                names: vec!["api_key".into(), "db_password".into(), "jwt_secret".into()],
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn seal_secret_zero_grace_period_roundtrip() {
        // Default grace_period_secs = 0 should be omitted in serialized form but round-trip correctly.
        let req = Request {
            id: "ss-2".into(),
            action: Action::SealSecret {
                name: "token".into(),
                value: "my-token".into(),
                grace_period_secs: 0,
            },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
        // Verify the default field is handled correctly.
        if let Action::SealSecret {
            grace_period_secs, ..
        } = decoded.action
        {
            assert_eq!(grace_period_secs, 0);
        } else {
            panic!("expected SealSecret action");
        }
    }

    #[test]
    fn roundtrip_file_delta_query_action() {
        let req = Request {
            id: "dq-1".into(),
            action: Action::FileDeltaQuery {
                path: "/etc/config.toml".into(),
                block_size: 262144,
            },
            timeout_ms: Some(30000),
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_file_delta_patch_action() {
        let req = Request {
            id: "dp-1".into(),
            action: Action::FileDeltaPatch {
                path: "/etc/config.toml".into(),
                block_size: 262144,
                patches: vec![DeltaPatch {
                    offset: 262144,
                    data: vec![0u8; 512],
                }],
                total_size: 786432,
                checksum: Some("abc123def456".into()),
                mode: Some(0o644),
            },
            timeout_ms: Some(60000),
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_file_delta_index_result() {
        let resp = Response {
            id: "dq-1".into(),
            result: RpcResult::FileDeltaIndex {
                blocks: vec![
                    BlockInfo {
                        offset: 0,
                        size: 262144,
                        adler32: 0xdeadbeef,
                        sha256_hex: "a".repeat(64),
                    },
                    BlockInfo {
                        offset: 262144,
                        size: 12345,
                        adler32: 0x12345678,
                        sha256_hex: "b".repeat(64),
                    },
                ],
                total_size: 274489,
                file_missing: false,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_file_delta_index_missing() {
        let resp = Response {
            id: "dq-2".into(),
            result: RpcResult::FileDeltaIndex {
                blocks: vec![],
                total_size: 0,
                file_missing: true,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
        if let RpcResult::FileDeltaIndex { file_missing, .. } = decoded.result {
            assert!(file_missing);
        } else {
            panic!("expected FileDeltaIndex");
        }
    }

    #[test]
    fn roundtrip_file_delta_applied_result() {
        let resp = Response {
            id: "dp-1".into(),
            result: RpcResult::FileDeltaApplied {
                bytes_transferred: 262144,
                blocks_changed: 1,
                total_blocks: 3,
                checksum_verified: true,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_ingress_register_action() {
        let req = Request {
            id: "ir-1".into(),
            action: Action::IngressRegister {
                agent_id: "web-01".into(),
                upstream_url: "http://127.0.0.1:8080".into(),
                subdomain: Some("api".into()),
                path_prefix: None,
            },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_reverse_proxy_action() {
        let req = Request {
            id: "rp-1".into(),
            action: Action::ReverseProxy {
                method: "POST".into(),
                path: "/api/users".into(),
                query: Some("page=1".into()),
                headers: vec![
                    ("content-type".into(), "application/json".into()),
                    ("authorization".into(), "Bearer token123".into()),
                ],
                body: Some(b"{\"name\":\"Alice\"}".to_vec()),
                upstream_url: "http://127.0.0.1:8080".into(),
                timeout_ms: Some(5000),
                max_response_bytes: Some(1048576),
            },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_ingress_registered_result() {
        let resp = Response {
            id: "ir-1".into(),
            result: RpcResult::IngressRegistered {
                agent_id: "web-01".into(),
                upstream_url: "http://127.0.0.1:8080".into(),
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_reverse_proxy_response_result() {
        let resp = Response {
            id: "rp-1".into(),
            result: RpcResult::ReverseProxyResponse {
                status: 200,
                headers: vec![("content-type".into(), "application/json".into())],
                body: Some(b"{\"ok\":true}".to_vec()),
                latency_ms: 12,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_check_update_action() {
        let req = Request {
            id: "cu-1".into(),
            action: Action::CheckUpdate {
                current_version: "0.19.0".into(),
            },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_update_agent_action() {
        let req = Request {
            id: "ua-1".into(),
            action: Action::UpdateAgent {
                version: "0.20.0".into(),
                url: "https://releases.ravenfabric.io/v0.20.0/rf-agent".into(),
                sha256: "abc123".into(),
                ed25519_sig: Some("sigbytes".into()),
                allow_downgrade: false,
            },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_update_available_result() {
        let resp = Response {
            id: "cu-1".into(),
            result: RpcResult::UpdateAvailable {
                version: "0.20.0".into(),
                url: "https://releases.ravenfabric.io/v0.20.0/rf-agent".into(),
                sha256: "abc123def456".into(),
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_update_not_available_result() {
        let resp = Response {
            id: "cu-2".into(),
            result: RpcResult::UpdateNotAvailable,
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_update_applied_result() {
        let resp = Response {
            id: "ua-1".into(),
            result: RpcResult::UpdateApplied {
                version: "0.20.0".into(),
                restarting: true,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_update_failed_result() {
        let resp = Response {
            id: "ua-2".into(),
            result: RpcResult::UpdateFailed {
                reason: "SHA-256 mismatch".into(),
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_pin_version_action() {
        let req = Request {
            id: "pv-1".into(),
            action: Action::PinVersion { version: "0.20.0".into() },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_unpin_version_action() {
        let req = Request {
            id: "pv-2".into(),
            action: Action::UnpinVersion,
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_get_version_info_action() {
        let req = Request {
            id: "vi-1".into(),
            action: Action::GetVersionInfo,
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_set_update_window_action() {
        let req = Request {
            id: "uw-1".into(),
            action: Action::SetUpdateWindow { window: Some("02:00-04:00".into()) },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_version_info_result() {
        let resp = Response {
            id: "vi-1".into(),
            result: RpcResult::VersionInfo {
                agent_id: "web-01".into(),
                current_version: "0.20.0".into(),
                pinned_version: Some("0.20.0".into()),
                update_window: Some("02:00-04:00".into()),
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_version_pinned_result() {
        let resp = Response {
            id: "pv-1".into(),
            result: RpcResult::VersionPinned { version: "0.20.0".into() },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_update_window_set_result() {
        let resp = Response {
            id: "uw-1".into(),
            result: RpcResult::UpdateWindowSet { window: Some("02:00-04:00".into()) },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }
}
