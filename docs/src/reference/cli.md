# CLI Reference

The `rf` binary is the user-facing CLI for RavenFabric. All commands communicate with agents via the relay.

## Global Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--relay <URL>` | `-r` | Relay WebSocket URL | `ws://127.0.0.1:9090` |
| `--key-path <PATH>` | `-k` | Client key file | `client.key` |
| `--help` | `-h` | Print help | — |
| `--version` | `-V` | Print version | — |

**Environment:** `RF_RELAY` overrides the default relay URL.

---

## `rf exec`

Execute a command on a remote agent.

```
rf exec --token <TOKEN> <COMMAND>
```

| Option | Short | Description | Required |
|--------|-------|-------------|----------|
| `--token <TOKEN>` | `-t` | Meet token for relay pairing | Yes |

### Examples

```bash
# Simple command
rf exec --token abc123 "hostname"

# With custom relay
rf exec --relay wss://relay.example.com:9090 --token abc123 "uname -a"

# Using environment variable
export RF_RELAY=wss://relay.example.com:9090
rf exec --token abc123 "systemctl status nginx"
```

---

## `rf shell`

Open an interactive shell session via PTY on the remote agent.

```
rf shell --token <TOKEN> [OPTIONS]
```

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--token <TOKEN>` | `-t` | Meet token for relay pairing | Required |
| `--cols <N>` | — | Terminal columns | 80 |
| `--rows <N>` | — | Terminal rows | 24 |

### Examples

```bash
# Open a shell with default terminal size
rf shell --token abc123

# Specify terminal dimensions
rf shell --token abc123 --cols 120 --rows 40
```

---

## `rf forward`

Port forward: listen locally, connect through the agent to a remote target.

```
rf forward --token <TOKEN> -L <LOCAL> -R <REMOTE>
```

| Option | Short | Description | Required |
|--------|-------|-------------|----------|
| `--token <TOKEN>` | `-t` | Meet token for relay pairing | Yes |
| `--local <ADDR>` | `-L` | Local bind address (e.g., `127.0.0.1:8080`) | Yes |
| `--remote <ADDR>` | `-R` | Remote target address (e.g., `db.internal:5432`) | Yes |

### Examples

```bash
# Forward local port 5432 to agent's PostgreSQL
rf forward --token abc123 -L 127.0.0.1:5432 -R localhost:5432

# Access a web service behind the agent
rf forward --token abc123 -L 0.0.0.0:8080 -R internal-api:3000
```

---

## `rf playbook`

Execute a multi-agent orchestration playbook.

```
rf playbook <FILE> --token <TOKEN>
```

| Option | Short | Description | Required |
|--------|-------|-------------|----------|
| `--token <TOKEN>` | `-t` | Meet token for relay pairing | Yes |
| `<FILE>` | — | Path to playbook YAML file | Yes |

### Examples

```bash
# Run a deployment playbook
rf playbook deploy.yaml --token abc123
```

---

## `rf dev`

Start a local development environment — relay + agent in a single process with a permissive policy. No authentication required.

```
rf dev [OPTIONS]
```

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--port <PORT>` | `-p` | Local relay listen port | 9090 |

### Examples

```bash
# Start dev mode on default port
rf dev

# Start on custom port
rf dev --port 8443

# Then in another terminal:
rf exec --relay ws://127.0.0.1:9090 --token dev "whoami"
```

---

## `rf status`

Query the status of a remote agent.

```
rf status --token <TOKEN>
```

| Option | Short | Description | Required |
|--------|-------|-------------|----------|
| `--token <TOKEN>` | `-t` | Meet token for relay pairing | Yes |

Returns agent ID, version, and uptime.

---

## `rf completions`

Generate shell completions.

```
rf completions <SHELL>
```

Supported shells: `bash`, `zsh`, `fish`, `powershell`, `elvish`.

```bash
# Zsh
rf completions zsh > ~/.zfunc/_rf

# Bash
rf completions bash > /etc/bash_completion.d/rf

# Fish
rf completions fish > ~/.config/fish/completions/rf.fish
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error (connection, RPC failure) |
| 2 | Policy denied — command not allowed by agent policy |
| 126 | Command not executable (on agent) |
| 127 | Command not found (on agent) |
| 130 | Interrupted (Ctrl+C) |

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RF_RELAY` | Default relay URL (avoids `--relay` flag) |
| `RUST_LOG` | Log level filter (e.g., `rf=info`, `rf=debug,rf_relay=trace`) |
