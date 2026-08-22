# CLI Reference

The `rf` binary is the user-facing CLI for RavenFabric. All commands communicate with agents via the relay.

## Global Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--relay <URL>` | `-r` | Relay WebSocket URL (comma-separated list for failover) | `ws://127.0.0.1:9090` |
| `--key-path <PATH>` | `-k` | Client key file | `client.key` |
| `--help` | `-h` | Print help | — |
| `--version` | `-V` | Print version | — |

**Environment:** `RF_RELAY` overrides the default relay URL.

**Relay failover:** `--relay` accepts a comma-separated list of relay URLs. The
CLI tries each in order and fails over to the next on connection or Noise XX
handshake failure. Example:

```bash
rf exec --relay "ws://relay-eu.example.com:9090,ws://relay-us.example.com:9090" --token abc123 "hostname"
```

---

## `rf exec`

Execute a command on a remote agent.

```text
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

```text
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

```text
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

```text
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

```text
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

```text
rf status --token <TOKEN>
```

| Option | Short | Description | Required |
|--------|-------|-------------|----------|
| `--token <TOKEN>` | `-t` | Meet token for relay pairing | Yes |

Returns agent ID, version, and uptime.

---

## `rf completions`

Generate shell completions.

```text
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

## `rf policy`

Policy template management and validation.

```text
rf policy <SUBCOMMAND>
```

### `rf policy list`

List all available built-in policy templates.

```bash
rf policy list
```

### `rf policy show`

Display the full YAML of a built-in template.

```bash
rf policy show safe-dev-mode
rf policy show production-ai-guardrails
```

### `rf policy validate`

Validate a policy YAML file for correctness.

```bash
# Validate a file
rf policy validate --file /etc/ravenfabric/policy.yaml

# Validate a built-in template
rf policy validate --template safe-dev-mode
```

| Option | Short | Description |
|--------|-------|-------------|
| `--file <PATH>` | `-f` | Path to policy YAML file |
| `--template <NAME>` | `-t` | Built-in template name |

### `rf policy compose`

Compose multiple templates with deny-wins conflict resolution.

```bash
rf policy compose "safe-dev-mode,production-ai-guardrails"
```

---

## `rf secret`

Manage secrets distributed to agents. Secrets are encrypted in transit and at rest; the controller never stores plaintext values.

### `rf secret push`

Push a secret to one or more agents.

```bash
# Push to a single named agent
rf secret push --token <TOKEN> --name DB_PASSWORD --value "env:DB_PASSWORD"

# Push to all agents (fleet-wide)
rf secret push --token <TOKEN> --name API_KEY --value "s3cr3t" --selector "*"

# Zero-downtime rotation: deliver new value but keep old valid for 5 minutes
rf secret push --token <TOKEN> --name SIGNING_KEY --value "new-key" \
  --grace-period 300
```

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--token <TOKEN>` | `-t` | Meet token | Required |
| `--name <NAME>` | `-n` | Secret name | Required |
| `--value <VALUE>` | `-v` | Secret value (`env:VAR` to read from env) | Required |
| `--selector <SEL>` | `-s` | Agent selector (`*` = all, `role=web` = by label) | Current agent |
| `--grace-period <SECS>` | `-g` | Keep old version valid for N seconds after push | 0 |
| `--ttl <SECS>` | — | Secret expires after N seconds on the agent | Never |

### `rf secret list`

List secret names and content hashes on the agent. Plaintext values are never returned.

```bash
rf secret list --token <TOKEN>
```

Output:

```text
NAME           HASH (SHA-256)       AGE
DB_PASSWORD    a3f1b2c4...          2d 4h
API_KEY        e9d8c7b6...          12h
SIGNING_KEY    f0e1d2c3...          5m (grace period active, old: 7f3a...)
```

| Option | Short | Description |
|--------|-------|-------------|
| `--token <TOKEN>` | `-t` | Meet token |

---

## `rf cp`

Copy files between the local machine and a remote agent, or between two remote agents.

### Syntax

```text
rf cp [OPTIONS] <SOURCE> <DEST>
```

Remote paths use the syntax `<agent-id>:<path>`, e.g. `web-01:/var/www/html/`.

```bash
# Upload local file to agent
rf cp --token <TOKEN> ./myapp-2.1 web-01:/opt/app/myapp

# Download from agent
rf cp --token <TOKEN> web-01:/var/log/app.log ./logs/app.log

# Recursive directory upload
rf cp --token <TOKEN> -r ./config/ web-01:/etc/myapp/

# Remote-to-remote copy (both ends are remote agents)
rf cp --token <TOKEN> src-agent:/data/ dst-agent:/backup/
```

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--token <TOKEN>` | `-t` | Meet token | Required |
| `--recursive` | `-r` | Copy directory trees | — |
| `--no-compress` | — | Disable zstd compression | Auto (by file extension) |
| `--chunk-size <BYTES>` | — | Transfer chunk size | 65536 |
| `--overwrite` | — | Overwrite existing destination files | Refuse if exists |
| `--dry-run` | — | Show what would be transferred without transferring | — |
| `--verify` | — | Re-read file after transfer to verify integrity | Enabled by default |

Integrity verification is performed automatically. A SHA-256 hash of the transferred content is computed end-to-end; if the destination hash does not match the source, the transfer is aborted and the partial destination file is removed.

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
