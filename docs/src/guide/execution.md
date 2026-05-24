# Remote Execution

RavenFabric supports multiple execution modes — from simple one-shot commands to interactive shell sessions and multi-agent orchestration.

## Execution Modes

| Mode | CLI Command | Response | Use Case |
|------|-------------|----------|----------|
| Standard | `rf exec` | stdout + stderr + exit code | One-off commands |
| Streaming | (automatic for long-running) | Real-time output chunks | Live monitoring |
| Background | `BackgroundExec` action | Job ID immediately | Long-running tasks |
| Interactive | `rf shell` | Full PTY session | Interactive debugging |
| Orchestrated | `rf playbook` | Multi-agent coordination | Deployments |

## Standard Execution

```bash
rf exec --token abc123 "hostname"
```

The flow:

1. CLI connects to relay and pairs with agent via meet token
2. Noise XX handshake establishes E2E encrypted channel
3. `Execute` action sent with command string
4. Agent checks policy → allowed → runs via `sh -c`
5. Response contains stdout, stderr, exit code, and duration

## Streaming Execution

For long-running commands, the agent streams output incrementally:

```bash
rf exec --token abc123 "journalctl -f"
```

Internally uses `StreamExecute` action. Output arrives as `StreamChunk` messages (tagged stdout/stderr), followed by a `StreamEnd` with the final exit code.

## Background Execution

Via the RPC `BackgroundExec` action, commands run detached:

- Agent returns a `job_id` and `pid` immediately
- Use `JobQuery` to check status
- Use `JobWait` to block until completion

## Interactive Shell

```bash
rf shell --token abc123 --cols 120 --rows 40
```

Opens a full PTY on the agent:

- Terminal enters raw mode (local echo disabled)
- Bidirectional stdin/stdout via `ShellInput`/`ShellOutput` messages
- Window resize propagated via `ShellResize`
- Session recording in asciicast v2 format

## Port Forwarding

### Local Forward (ssh -L equivalent)

```bash
rf forward --token abc123 -L 127.0.0.1:5432 -R db.internal:5432
```

Listen locally, connect through the agent to a remote target.

### Remote Forward (ssh -R equivalent)

Via `RemoteForward` RPC action — agent listens on a port and tunnels back to the client.

### SOCKS5 Dynamic Forward

Via `Socks5Forward` RPC action — agent runs a SOCKS5 proxy for arbitrary destination forwarding.

## Multi-Agent Orchestration

```bash
rf playbook deploy.yaml --token abc123
```

Playbooks define ordered steps across multiple agents with rollback:

```yaml
name: Deploy update
agents: ["web-01", "web-02", "web-03"]
steps:
  - command: "systemctl stop myapp"
    rollback: "systemctl start myapp"
  - command: "apt-get update && apt-get install -y myapp"
    rollback: "apt-get install -y myapp=1.0.0"
  - command: "systemctl start myapp"
```

If any step fails, preceding steps are automatically rolled back on agents that succeeded.

## Resource Limits

Execution is bounded by policy:

| Resource | Policy Field | Default |
|----------|-------------|---------|
| Output size | `maxOutputBytes` | 10 MB |
| Timeout | `timeoutSeconds` | 300s |
| Working directory | `workdir` restriction | Any allowed path |
| Environment | `env` passthrough | Filtered by policy |

## Security Flow

Every execution follows this path:

```
CLI → Relay (encrypted, opaque) → Agent
                                    ├── Policy check (deny-by-default)
                                    ├── Execute (within resource limits)
                                    ├── Audit log entry written
                                    └── Response encrypted back
```

The agent is always the final authority — a compromised CLI or relay cannot override agent-side policy.
