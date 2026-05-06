# Cursor Integration

This guide shows how to connect [Cursor](https://cursor.sh) to RavenFabric via the MCP server.

## Prerequisites

- RavenFabric installed (`rf-mcp-server` binary available)
- Cursor IDE installed
- A policy file for the AI agent

## Configuration

Cursor supports MCP servers via its settings. Add the following to your Cursor MCP configuration:

### Project-level (`.cursor/mcp.json`)

```json
{
  "mcpServers": {
    "ravenfabric": {
      "command": "rf-mcp-server",
      "args": [
        "--policy", "/path/to/policy.yaml",
        "--api-token", "your-secret-token",
        "--rate-limit", "60"
      ]
    }
  }
}
```

### Global configuration

Add to `~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "ravenfabric": {
      "command": "rf-mcp-server",
      "args": ["--policy", "~/.config/ravenfabric/coding-assistant.yaml"],
      "env": {
        "RF_API_TOKEN": "your-secret-token",
        "RF_RATE_LIMIT": "30"
      }
    }
  }
}
```

## Workspace-Scoped Policy

For per-project security boundaries, create a policy file in your project:

```bash
mkdir -p .ravenfabric
cat > .ravenfabric/policy.yaml << 'EOF'
spec:
  commands:
    allow:
      - pattern: "^cat .*"
      - pattern: "^ls .*"
      - pattern: "^grep .*"
      - pattern: "^git .*"
      - pattern: "^cargo .*"
      - pattern: "^npm .*"
      - pattern: "^python3? .*"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*sudo.*"
  filesystem:
    allow:
      - path: .
    deny:
      - path: /etc
      - path: /root
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
EOF
```

Then reference it in `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "ravenfabric": {
      "command": "rf-mcp-server",
      "args": ["--policy", ".ravenfabric/policy.yaml"]
    }
  }
}
```

## Available Tools

Once configured, Cursor's AI can use:

| Tool | Description |
|------|-------------|
| `rf_exec` | Execute commands (policy-validated) |
| `rf_query_policy` | Pre-flight policy check |
| `rf_request_approval` | Human approval for sensitive ops |
| `rf_list_my_capabilities` | Discover allowed operations |
| `rf_audit_query` | Query session audit log |
| `rf_file_read` | Read files (path-policy enforced) |
| `rf_file_write` | Write files (path-policy enforced) |

## Verifying Setup

1. Open Cursor in your project
2. Open the AI chat
3. Ask: "Use rf_list_my_capabilities to show what I can do"
4. You should see a JSON response with policy limits and session ID

## Security Best Practices

- Use project-level config (`.cursor/mcp.json`) for workspace-scoped policies
- Never commit API tokens — use environment variables or secrets managers
- Set rate limits appropriate to your workflow (30-60 calls/min typical)
- Review audit logs periodically: `rf-mcp-server` logs to the configured audit path

## Troubleshooting

**MCP server not starting:**
- Check `rf-mcp-server` is in PATH: `which rf-mcp-server`
- Check Cursor's Output panel for MCP errors
- Try running the command manually to see error output

**"Command denied" for expected operations:**
- Check policy patterns match your commands exactly (regex)
- Use `rf_query_policy` to test specific commands

**Rate limiting too aggressive:**
- Increase `--rate-limit` value or `RF_RATE_LIMIT` env var
- Default is 60 calls per minute
