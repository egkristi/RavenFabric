# Claude Desktop Integration

This guide shows how to connect [Claude Desktop](https://claude.ai/download) to RavenFabric via the MCP server.

## Prerequisites

- RavenFabric installed (`rf-mcp-server` binary available)
- Claude Desktop installed (macOS or Windows)
- A policy file for the AI agent

## Configuration

Claude Desktop uses `claude_desktop_config.json` to configure MCP servers.

### macOS

Edit `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "ravenfabric": {
      "command": "rf-mcp-server",
      "args": [
        "--policy", "/Users/you/.config/ravenfabric/ai-policy.yaml",
        "--api-token", "your-secret-token",
        "--rate-limit", "60"
      ]
    }
  }
}
```

### Windows

Edit `%APPDATA%\Claude\claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "ravenfabric": {
      "command": "rf-mcp-server.exe",
      "args": [
        "--policy", "C:\\Users\\you\\.config\\ravenfabric\\ai-policy.yaml",
        "--api-token", "your-secret-token",
        "--rate-limit", "60"
      ]
    }
  }
}
```

## Environment Variables

You can use environment variables instead of inline tokens:

```json
{
  "mcpServers": {
    "ravenfabric": {
      "command": "rf-mcp-server",
      "args": ["--policy", "~/.config/ravenfabric/ai-policy.yaml"],
      "env": {
        "RF_API_TOKEN": "your-secret-token",
        "RF_RATE_LIMIT": "60"
      }
    }
  }
}
```

## Policy Configuration

Use a `coding-assistant` style policy for development work:

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
      - path: /Users/you/projects
    deny:
      - path: /etc/shadow
      - path: /root
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

## Available Tools

Once configured, Claude Desktop can use these MCP tools:

| Tool | Description |
|------|-------------|
| `rf_exec` | Execute commands (policy-validated) |
| `rf_query_policy` | Pre-flight policy check without execution |
| `rf_request_approval` | Request human approval for sensitive ops |
| `rf_list_my_capabilities` | Discover allowed operations |
| `rf_audit_query` | Query session audit log |
| `rf_file_read` | Read files (path-policy enforced) |
| `rf_file_write` | Write files (path-policy enforced) |

## Verifying the Connection

1. Restart Claude Desktop after editing the config
2. Look for the hammer icon in the chat input area — it indicates MCP tools are available
3. Ask Claude: "Use rf_list_my_capabilities to show what operations are available"
4. You should see a JSON response with policy limits and session ID

## Security Features

- **Session isolation** — each Claude Desktop session spawns its own `rf-mcp-server` process
- **Rate limiting** — prevents runaway tool-call loops (default: 60/min)
- **Anomaly detection** — behavioral patterns tracked per session, high deviation scores logged
- **Audit trail** — every tool call recorded with timestamp, command, decision, and session ID
- **Fail-closed** — if policy is missing or invalid, all commands are denied

## Troubleshooting

**No hammer icon in chat:**
- Restart Claude Desktop completely (Cmd+Q, reopen)
- Check JSON syntax in config file: `cat ~/Library/Application\ Support/Claude/claude_desktop_config.json | jq .`
- Verify `rf-mcp-server` is in PATH: `which rf-mcp-server`

**"Authentication required" error:**
- Ensure the `apiToken` is sent during initialization
- Claude Desktop passes MCP server args as configured — check spelling

**Commands blocked:**
- Use `rf_query_policy` tool to test specific commands
- Update policy YAML to match your regex patterns (anchored with `^`)

**Server crashes on start:**
- Check policy YAML syntax: `rf-mcp-server --policy ./policy.yaml` manually
- Review stderr output for error messages
