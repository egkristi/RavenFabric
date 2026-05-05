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
