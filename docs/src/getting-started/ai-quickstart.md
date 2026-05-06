# AI Agent Quick Start

Get an AI agent connected to RavenFabric in under 5 minutes.

## Overview

RavenFabric provides an MCP (Model Context Protocol) server that lets AI agents execute commands, read/write files, and query policies — all within a security sandbox.

```
AI Agent (Claude, Cursor, Aider)
    ↓ MCP stdio
rf-mcp-server
    ↓ policy check
Executor → shell command / filesystem
    ↓
Audit log (every action recorded)
```

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
- **Session isolation** — each AI session is a separate sandboxed process
- **API token auth** — constant-time validated, reject unauthorized sessions

## Next Steps

- [Claude Code integration guide](../integrations/claude-code.md) — full configuration reference
- [Cursor integration guide](../integrations/cursor.md) — workspace-scoped setup
- [Aider integration guide](../integrations/aider.md) — `.aider.conf.yml` reference
- [Policy YAML format](../reference/policy-yaml.md) — full policy syntax reference
- [Audit log format](../reference/audit-log-format.md) — understanding audit entries
