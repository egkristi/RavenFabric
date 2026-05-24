# MCP Server Reference

`rf-mcp-server` implements the [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) to provide policy-controlled system access to AI agents like Claude, Cursor, and Aider.

## Overview

The MCP server translates JSON-RPC 2.0 tool calls from AI agents into RavenFabric operations, enforcing deny-by-default policy on every action.

```text
AI Agent (Claude, Cursor, Aider)
    ↓ JSON-RPC 2.0 (stdio or HTTP+SSE)
rf-mcp-server
    ↓ policy check + anomaly detection
Executor → shell / filesystem
    ↓
Audit log (every action recorded)
```

## Installation

```bash
cargo build --release -p rf-mcp-server
cp target/release/rf-mcp-server ~/.local/bin/
```

## CLI Flags

| Flag | Env Variable | Description |
|------|-------------|-------------|
| `--policy <path>` | `RF_POLICY_PATH` | Path to the RPC policy YAML file |
| `--audit <path>` | `RF_AUDIT_PATH` | Path to the audit log file (JSON-lines) |
| `--caller-key <key>` | — | Caller identity key (default: `mcp-session`) |
| `--api-token <token>` | `RF_API_TOKEN` | API token for authentication |
| `--api-token-file <path>` | `RF_API_TOKEN_FILE` | Path to token file (re-read per connection, for rotation) |
| `--callers <path>` | `RF_CALLERS` | RBAC callers config (TOML) for per-caller policy profiles |
| `--alert-webhook <url>` | `RF_ALERT_WEBHOOK` | Webhook URL for anomaly/security alert notifications |
| `--rate-limit <n>` | `RF_RATE_LIMIT` | Max tool calls per minute (default: 60) |
| `--http-listen <addr>` | `RF_HTTP_LISTEN` | Enable HTTP+SSE mode (e.g., `0.0.0.0:8080`) |
| `--log-level <level>` | `RF_LOG_LEVEL` | Log level: `trace`, `debug`, `info`, `warn`, `error` (default: `info`) |

## Transport Modes

### stdio (default)

Single-user, single-session. Used by Claude Desktop, Cursor, and other MCP clients that spawn child processes.

```bash
rf-mcp-server --policy policy.yaml --api-token "$TOKEN"
```

### HTTP+SSE (multi-user)

Multi-user server deployment for web-based AI applications. Requires the `http-sse` feature.

```bash
rf-mcp-server --policy policy.yaml --api-token "$TOKEN" --http-listen 0.0.0.0:8080
```

Build with the feature enabled:

```bash
cargo build --release -p rf-mcp-server --features http-sse
```

**Endpoints:**

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/message` | Send JSON-RPC requests |
| `GET` | `/sse` | Server-Sent Events stream for responses |
| `GET` | `/health` | Health check (returns `ok`) |

**Headers:**

| Header | Required | Purpose |
|--------|----------|---------|
| `Authorization: Bearer <token>` | On `initialize` | API token for authentication |
| `X-Session-Id: <id>` | After `initialize` | Route requests to correct session |

## Tools

The MCP server exposes 8 tools:

| Tool | Purpose |
|------|---------|
| `rf_exec` | Execute a command (policy-checked, audited) |
| `rf_query_policy` | Pre-flight policy check without execution |
| `rf_file_read` | Read a file (subject to path policy) |
| `rf_file_write` | Write a file (subject to path policy) |
| `rf_list_my_capabilities` | Dynamic capability discovery |
| `rf_audit_query` | Query recent audit entries from this session |
| `rf_request_approval` | Request human-in-loop approval for sensitive ops |
| `rf_check_approval` | Poll status of a pending approval request |

### rf_exec

Execute a shell command through the policy engine.

**Arguments:**

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `command` | string | Yes | The command to execute |
| `workdir` | string | No | Working directory |
| `reason` | string | No | Justification (recorded in audit) |
| `timeout_ms` | integer | No | Execution timeout in milliseconds |

**Example:**

```json
{
  "name": "rf_exec",
  "arguments": {
    "command": "git status",
    "workdir": "/home/user/project",
    "reason": "Check repository state before commit"
  }
}
```

### rf_query_policy

Check if a command would be allowed without executing it.

**Arguments:**

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `command` | string | Yes | Command to check |

### rf_file_read / rf_file_write

Read or write files subject to path policy.

**rf_file_read arguments:**

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `path` | string | Yes | Absolute file path |

**rf_file_write arguments:**

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `path` | string | Yes | Absolute file path |
| `content` | string | Yes | File content to write |
| `mode` | integer | No | Unix file permissions (e.g., 644) |

### rf_audit_query

Query the session audit log.

**Arguments:**

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `limit` | integer | No | Max entries to return (default: 20) |
| `action_filter` | string | No | Filter by action substring |

### rf_request_approval / rf_check_approval

Human-in-loop approval workflow for sensitive operations.

**rf_request_approval arguments:**

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `operation` | string | Yes | Type of operation (e.g., "deploy") |
| `command` | string | Yes | The command requiring approval |
| `reason` | string | Yes | Why this needs approval |

**rf_check_approval arguments:**

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `approval_id` | string | Yes | ID returned by rf_request_approval |

## Authentication

### API Token

Set a shared secret that clients must provide during initialization:

```bash
rf-mcp-server --api-token "your-secret-token"
```

Clients include it in the `initialize` request:

```json
{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"apiToken": "your-secret-token"}}
```

Token validation uses constant-time comparison to prevent timing attacks.

### Token Rotation

For zero-downtime token rotation:

1. **Comma-separated tokens** — Both old and new tokens are accepted during the grace period:

   ```bash
   rf-mcp-server --api-token "new-token,old-token"
   ```

2. **Token file** — External rotation via file re-read:

   ```bash
   rf-mcp-server --api-token-file /etc/ravenfabric/mcp-token
   ```

   Update the file contents to rotate. The server re-reads on each new connection.

## RBAC: Per-Caller Policies

Map different API tokens to different policy profiles using a TOML config:

```toml
# callers.toml
[[callers]]
name = "ci-agent"
token = "ci-token-secret"
policy = "/etc/ravenfabric/ci-policy.yaml"

[[callers]]
name = "dev-agent"
token = "dev-token-secret"
policy = "/etc/ravenfabric/dev-policy.yaml"

[[callers]]
name = "readonly-agent"
token = "readonly-token"
policy = "/etc/ravenfabric/readonly-policy.yaml"
```

```bash
rf-mcp-server --callers callers.toml
```

When a caller authenticates with their token, the server loads their assigned policy. Callers cannot access resources outside their profile.

## Session Security

### Per-Session Cryptographic Identity

Each MCP session generates a short-lived Curve25519 keypair. The public key is included in the `initialize` response:

```json
{
  "sessionId": "a1b2c3d4-...",
  "sessionPublicKey": "7f3a8b2c..."
}
```

This provides:

- Cryptographic session correlation in audit trails
- Proof of session identity for external verification
- Keys are ephemeral and zeroed from memory on session end

### Rate Limiting

Sliding window rate limiter prevents runaway AI loops:

```bash
rf-mcp-server --rate-limit 30  # 30 tool calls per minute
```

Default: 60 calls/minute. When exceeded, the server returns an error with retry-after information.

### Behavioral Anomaly Detection

The server tracks per-session behavioral baselines and detects anomalies:

- **Velocity** — sudden increase in request rate
- **Novelty** — commands not seen before in this session
- **Timing** — activity outside normal hours
- **Escalation** — progression from reads to writes to destructive commands

Anomaly events are written to the audit log and optionally sent to a webhook.

## Alert Webhook

Configure a webhook for real-time anomaly notifications:

```bash
rf-mcp-server --alert-webhook http://alerts.internal:9090/webhook
```

The server sends an HTTP POST with a JSON payload when anomaly events are detected:

```json
{
  "type": "anomaly_alert",
  "session_id": "a1b2c3d4-...",
  "command": "rm -rf /tmp/data",
  "anomaly_count": 1,
  "events": [
    {
      "type": "Escalation",
      "score": 0.85,
      "description": "Destructive command pattern detected"
    }
  ],
  "cumulative_score": 2.4,
  "timestamp": "2026-05-06T12:34:56Z"
}
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `full` | Yes | Includes sysinfo support in executor |
| `minimal` | No | Reduced dependency footprint |
| `http-sse` | No | Enables HTTP+SSE multi-user transport |

## Integration Examples

### Claude Desktop

```json
{
  "mcpServers": {
    "ravenfabric": {
      "command": "rf-mcp-server",
      "args": ["--policy", "policy.yaml", "--api-token", "your-token"]
    }
  }
}
```

### Claude Code

```bash
claude mcp add ravenfabric -- rf-mcp-server --policy policy.yaml --api-token "$TOKEN"
```

### Cursor

```json
{
  "mcpServers": {
    "ravenfabric": {
      "command": "rf-mcp-server",
      "args": ["--policy", "/path/to/policy.yaml", "--api-token", "your-token", "--rate-limit", "30"]
    }
  }
}
```

See [Integration Guides](../integrations/claude-desktop.md) for detailed setup instructions.
