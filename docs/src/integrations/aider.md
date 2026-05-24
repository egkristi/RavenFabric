# Aider Integration

This guide shows how to connect [Aider](https://aider.chat) to RavenFabric via the MCP server.

## Prerequisites

- RavenFabric installed (`rf-mcp-server` binary available)
- Aider installed (`pip install aider-chat`)
- A policy file for the AI agent

## Configuration

Aider supports MCP servers via its configuration. Add to your `.aider.conf.yml` or `~/.aider.conf.yml`:

```yaml
mcp-servers:
  - name: ravenfabric
    command: rf-mcp-server
    args:
      - --policy
      - /path/to/policy.yaml
      - --api-token
      - ${RF_API_TOKEN}
      - --rate-limit
      - "60"
```

Or pass directly on the command line:

```bash
aider --mcp-server "rf-mcp-server --policy ./policy.yaml"
```

## Environment Setup

Set the API token via environment variable:

```bash
export RF_API_TOKEN="your-secret-token"
export RF_RATE_LIMIT=60
```

## Policy for Aider

Aider primarily reads and writes code files, and runs tests. A suitable policy:

```yaml
spec:
  commands:
    allow:
      - pattern: "^cat .*"
      - pattern: "^ls .*"
      - pattern: "^find .*"
      - pattern: "^grep .*"
      - pattern: "^git .*"
      - pattern: "^cargo .*"
      - pattern: "^npm .*"
      - pattern: "^python3? .*"
      - pattern: "^pytest.*"
      - pattern: "^make .*"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*sudo.*"
      - pattern: ".*curl.*|.*"
  filesystem:
    allow:
      - path: .
    deny:
      - path: /etc
      - path: /root
      - path: /var
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

Save as `~/.config/ravenfabric/aider-policy.yaml`.

## Available Tools

When connected, Aider's AI can use:

| Tool | Description |
|------|-------------|
| `rf_exec` | Execute commands (policy-validated) |
| `rf_query_policy` | Check if a command would be allowed |
| `rf_request_approval` | Request human approval |
| `rf_list_my_capabilities` | Discover allowed operations |
| `rf_audit_query` | Query session audit log |
| `rf_file_read` | Read files (path-policy enforced) |
| `rf_file_write` | Write files (path-policy enforced) |

## Usage Example

Once configured, you can ask Aider to execute validated commands:

```
> Run the test suite using rf_exec

Aider will call rf_exec with the appropriate test command,
subject to your policy rules.
```

## Security Notes

- Each Aider session spawns its own `rf-mcp-server` process (session isolation)
- Rate limiting prevents infinite loops (configurable per minute)
- All actions are logged to the audit trail
- Policy is read-only at runtime — the AI cannot modify its own permissions

## Troubleshooting

**Server not found:**

- Ensure `rf-mcp-server` is in your PATH
- Build: `cargo build --release -p rf-mcp-server && cargo install --path crates/rf-mcp-server`

**Authentication errors:**

- Check `RF_API_TOKEN` is exported in the shell where Aider runs
- Token must match between env var and what the MCP client sends

**Commands blocked unexpectedly:**

- Verify regex patterns in your policy (anchored with `^`)
- Test with: `rf-mcp-server --policy ./policy.yaml` then send a manual JSON-RPC request

**Rate limiting:**

- Default: 60 calls/minute. Increase with `--rate-limit 120` for heavy workloads
