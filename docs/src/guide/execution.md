# Remote Execution

RavenFabric supports multiple execution modes, from fire-and-forget to fully orchestrated playbooks.

## Fire and Forget

Execute a command without waiting for results:

```bash
rf exec --mode fire-and-forget --target web-01 "touch /tmp/heartbeat"
```

## Standard Execution

Execute and wait for results:

```bash
rf exec --target web-01 "systemctl status nginx"
```

Output is streamed in real-time via yamux multiplexing.

## Multi-Agent Execution

Execute across multiple agents:

```bash
rf exec --target "web-*" "apt update && apt upgrade -y"
```

## Interactive Shell

Open an interactive PTY session:

```bash
rf shell --target web-01 --cols 120 --rows 40
```

## Execution Modes

| Mode | Description |
|------|-------------|
| `fire-and-forget` | No response expected |
| `fire-and-verify` | Verify exit code only |
| `streaming` | Real-time stdout/stderr |
| `orchestrated` | Multi-step with rollback |

## Resource Limits

Every execution is bounded by policy:

- **Output size**: Maximum bytes of stdout/stderr (default: 10 MB)
- **Timeout**: Maximum execution time (default: 300s)
- **Concurrent**: Maximum parallel executions per agent

## Security

- Every command is checked against the policy engine before execution
- Commands run via `sh -c` with the full command string policy-checked
- Symlinks are resolved before path checks (prevents traversal)
- Output is bounded to prevent memory exhaustion
- Agent is always the final authority — controller cannot override

## See Also

- [CLI Reference](../reference/cli.md) — All command options and flags
- [Policy Configuration](policy-config.md) — Define what commands are allowed
- [Audit Log Format](../reference/audit-log-format.md) — Every execution is logged
- [Troubleshooting](troubleshooting.md) — Common execution issues
