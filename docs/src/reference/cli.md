# CLI Reference

## `rf exec`

Execute a command on a remote agent.

```
rf exec [OPTIONS] <COMMAND>
```

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `--relay <URL>` | Relay WebSocket URL | Required |
| `--token <TOKEN>` | Authentication token | Required |
| `--target <ID>` | Target agent ID or pattern | Required |
| `--timeout <SECS>` | Execution timeout | 30 |
| `--mode <MODE>` | Execution mode (see below) | `streaming` |

### Modes

- `fire-and-forget` — No response
- `fire-and-verify` — Exit code only
- `streaming` — Real-time output
- `orchestrated` — Multi-step

### Examples

```bash
# Simple command
rf exec --relay wss://relay.example.com/meet \
  --token abc123 "hostname"

# Multi-agent
rf exec --target "web-*" "systemctl status nginx"

# With timeout
rf exec --timeout 60 "apt update"
```

---

## `rf shell`

Open an interactive shell session.

```
rf shell [OPTIONS]
```

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `--relay <URL>` | Relay WebSocket URL | Required |
| `--token <TOKEN>` | Authentication token | Required |
| `--target <ID>` | Target agent ID | Required |
| `--cols <N>` | Terminal columns | 80 |
| `--rows <N>` | Terminal rows | 24 |

---

## `rf status`

Query agent status.

```
rf status [OPTIONS]
```

---

## `rf completions`

Generate shell completions.

```
rf completions <SHELL>
```

Supported shells: `bash`, `zsh`, `fish`, `powershell`, `elvish`.

```bash
# Generate and install zsh completions
rf completions zsh > ~/.zfunc/_rf
```

---

## `rf dev`

Start a local development environment (relay + agent in one process).

```
rf dev [OPTIONS]
```

| Option | Description | Default |
|--------|-------------|---------|
| `--port <PORT>` | Local relay port | 9090 |
| `--policy <PATH>` | Policy file | permissive default |

---

## `rf forward`

Port forwarding through the agent.

```
rf forward [OPTIONS] -L <LOCAL:REMOTE>
```

| Option | Description | Default |
|--------|-------------|---------|
| `--relay <URL>` | Relay WebSocket URL | Required |
| `--token <TOKEN>` | Authentication token | Required |
| `--target <ID>` | Target agent ID | Required |
| `-L <bind:host:port>` | Local forward (listen locally, connect on agent) | Required |

### Examples

```bash
# Forward local port 5432 to agent's PostgreSQL
rf forward --target db-01 -L 5432:localhost:5432

# Forward with custom bind address
rf forward --target web-01 -L 0.0.0.0:8080:localhost:80
```

---

## `rf playbook`

Execute multi-agent orchestrated operations.

```
rf playbook <FILE> [OPTIONS]
```

| Option | Description | Default |
|--------|-------------|---------|
| `--target <PATTERN>` | Target agent filter (labels or glob) | From playbook |
| `--rollback-on-failure` | Auto-rollback on any step failure | false |
| `--dry-run` | Show what would execute without running | false |
| `--parallel <N>` | Max parallel agents | 5 |

### Examples

```bash
# Run a playbook across production
rf playbook deploy.yaml --target 'env=production'

# Dry run first
rf playbook patch.yaml --dry-run

# With auto-rollback
rf playbook upgrade.yaml --rollback-on-failure
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Connection failed |
| 3 | Authentication failed |
| 4 | Policy denied |
| 5 | Command timeout |
| 6 | Agent not found |
| 126 | Command not executable (on agent) |
| 127 | Command not found (on agent) |
| 130 | Interrupted (Ctrl+C) |

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RF_RELAY` | Default relay URL (avoids `--relay` flag) |
| `RF_KEY_PATH` | Default key file path |
| `RF_TOKEN` | Default authentication token |
| `RF_POLICY` | Default policy file path |
| `RUST_LOG` | Log level (`error`, `warn`, `info`, `debug`, `trace`) |

---

## Global Options

These options apply to all subcommands:

| Option | Description |
|--------|-------------|
| `--version` | Print version and exit |
| `--help` | Print help and exit |
| `--config <PATH>` | Configuration file path |
| `--key <PATH>` | Private key file path |
