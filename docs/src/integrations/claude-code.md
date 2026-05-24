# Claude Code Integration

This guide shows how to connect [Claude Code](https://docs.anthropic.com/en/docs/claude-code) to RavenFabric via the MCP server.

## Prerequisites

- RavenFabric installed (`rf-mcp-server` binary available)
- Claude Code installed (`claude` CLI)
- A policy file for the AI agent (recommended: `coding-assistant` template)

## Quick Setup

Add the MCP server to Claude Code:

```bash
claude mcp add ravenfabric -- rf-mcp-server --policy /path/to/policy.yaml
```

With API token authentication:

```bash
claude mcp add ravenfabric -- rf-mcp-server \
  --policy /path/to/policy.yaml \
  --api-token "$(cat /path/to/token)"
```

With rate limiting (30 tool calls per minute):

```bash
claude mcp add ravenfabric -- rf-mcp-server \
  --policy /path/to/policy.yaml \
  --api-token "$(cat /path/to/token)" \
  --rate-limit 30
```

## Policy Configuration

Use the `coding-assistant` policy template for development work:

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
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*sudo.*"
      - pattern: ".*curl.*|.*"
  filesystem:
    allow:
      - path: /home/user/project
    deny:
      - path: /etc
      - path: /root
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

Save as `~/.config/ravenfabric/coding-assistant.yaml`.

## Environment Variables

Instead of CLI flags, you can use environment variables:

```bash
export RF_API_TOKEN="your-secret-token"
export RF_RATE_LIMIT=60
```

## Available Tools

Once connected, Claude Code can use these tools:

| Tool | Description |
|------|-------------|
| `rf_exec` | Execute commands (policy-validated) |
| `rf_query_policy` | Check if a command would be allowed |
| `rf_request_approval` | Request human approval for sensitive ops |
| `rf_list_my_capabilities` | Discover allowed operations |
| `rf_audit_query` | Query session audit log |
| `rf_file_read` | Read files (path-policy enforced) |
| `rf_file_write` | Write files (path-policy enforced) |

## Verifying the Connection

After setup, ask Claude Code to run:

```text
Use the rf_list_my_capabilities tool to show what operations are available.
```

You should see a JSON response listing policy limits and session info.

## Security Notes

- Each Claude Code session gets its own isolated process
- Sessions have unique IDs tracked in the audit log
- Rate limiting prevents runaway loops
- API tokens are validated with constant-time comparison
- All commands pass through the deny-by-default policy engine

## Troubleshooting

**"Authentication required" error:**

- Ensure `--api-token` matches what Claude Code sends in the initialize request
- Check that `RF_API_TOKEN` env variable is set if using env-based auth

**"Rate limited" error:**

- Increase `--rate-limit` or wait for the window to reset (1 minute)

**Command denied:**

- Use `rf_query_policy` to check why a command is blocked
- Update your policy YAML to allow the pattern

**Server not found:**

- Verify `rf-mcp-server` is in your PATH: `which rf-mcp-server`
- Build with: `cargo build --release -p rf-mcp-server`
