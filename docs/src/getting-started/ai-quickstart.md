# AI Agent Quick Start

Get an AI agent connected to RavenFabric in under 5 minutes.

## Overview

RavenFabric provides an MCP (Model Context Protocol) server that lets AI agents execute commands, read/write files, and query policies — all within a security sandbox with behavioral anomaly detection.

```text
AI Agent (Claude, Cursor, Aider)
    ↓ MCP (stdio or HTTP+SSE)
rf-mcp-server
    ↓ policy check + anomaly detection
Executor → shell command / filesystem
    ↓
Audit log (every action recorded)
```

**Available tools:** `rf_exec`, `rf_query_policy`, `rf_file_read`, `rf_file_write`, `rf_list_my_capabilities`, `rf_audit_query`, `rf_request_approval`, `rf_check_approval`

## Step 1: Build the MCP Server

```bash
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric
cargo build --release -p rf-mcp-server
```

The binary is at `target/release/rf-mcp-server`. Add it to your PATH:

```bash
cp target/release/rf-mcp-server ~/.local/bin/
```

## Step 2: Create a Policy

Create `~/.config/ravenfabric/ai-policy.yaml`:

```yaml
spec:
  commands:
    allow:
      - pattern: "^echo .*"
      - pattern: "^cat .*"
      - pattern: "^ls .*"
      - pattern: "^find .*"
      - pattern: "^grep .*"
      - pattern: "^git .*"
      - pattern: "^cargo .*"
      - pattern: "^npm .*"
      - pattern: "^python3? .*"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*sudo.*"
      - pattern: ".*curl.*|.*"
  filesystem:
    allow:
      - path: /home
    deny:
      - path: /etc/shadow
      - path: /root
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

## Step 3: Generate an API Token

```bash
# Generate a random token
export RF_API_TOKEN=$(openssl rand -hex 32)
echo "Your token: $RF_API_TOKEN"
```

## Step 4: Connect Your AI Agent

### Claude Code

```bash
claude mcp add ravenfabric -- rf-mcp-server \
  --policy ~/.config/ravenfabric/ai-policy.yaml \
  --api-token "$RF_API_TOKEN" \
  --rate-limit 60
```

### Cursor

Create `.cursor/mcp.json` in your project:

```json
{
  "mcpServers": {
    "ravenfabric": {
      "command": "rf-mcp-server",
      "args": [
        "--policy", "~/.config/ravenfabric/ai-policy.yaml",
        "--rate-limit", "60"
      ],
      "env": {
        "RF_API_TOKEN": "your-token-here"
      }
    }
  }
}
```

### Aider

```bash
aider --mcp-server "rf-mcp-server --policy ~/.config/ravenfabric/ai-policy.yaml"
```

## Step 5: Test It

Ask your AI agent:

> "Use rf_list_my_capabilities to show what operations are available."

You should see a JSON response with policy limits and session information.

Then try:

> "Use rf_exec to run 'echo Hello from RavenFabric'"

## What You Get

- **Deny-by-default security** — only explicitly allowed commands execute
- **Full audit trail** — every action logged with timestamps, session ID, and decision
- **Rate limiting** — prevents runaway AI loops (default: 60 calls/minute)
- **Per-session crypto identity** — Curve25519 keypair per session for tamper-proof audit correlation
- **Behavioral anomaly detection** — velocity, novelty, timing, and escalation patterns tracked
- **Alert webhook** — real-time notifications on suspicious activity
- **API token auth** — constant-time validated, supports token rotation
- **RBAC per caller** — different tokens get different policy profiles
- **Human-in-loop approvals** — sensitive operations require explicit human confirmation

## HTTP+SSE Mode (Multi-User)

For web-based deployments or multi-user setups:

```bash
cargo build --release -p rf-mcp-server --features http-sse

rf-mcp-server \
  --policy ~/.config/ravenfabric/ai-policy.yaml \
  --api-token "$RF_API_TOKEN" \
  --http-listen 0.0.0.0:8080 \
  --alert-webhook http://alerts.internal/webhook
```

See the [MCP Server Reference](../reference/mcp-server.md) for full endpoint documentation.

## RBAC: Per-Caller Policies

Give different AI agents different permissions via a callers config:

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
```

```bash
rf-mcp-server --callers callers.toml
```

Each caller authenticates with their token and automatically gets their assigned policy.

## Next Steps

- [MCP Server Reference](../reference/mcp-server.md) — full CLI flags, tools, and configuration
- [Claude Code integration guide](../integrations/claude-code.md) — full configuration reference
- [Cursor integration guide](../integrations/cursor.md) — workspace-scoped setup
- [Aider integration guide](../integrations/aider.md) — `.aider.conf.yml` reference
- [Policy YAML format](../reference/policy-yaml.md) — full policy syntax reference
- [Audit log format](../reference/audit-log-format.md) — understanding audit entries
