# Multi-Node Ubuntu Demo

Manage multiple Ubuntu systems remotely using RavenFabric. This demo sets up three Docker containers — one relay and two agents — and runs 11 scenarios covering remote execution, port forwarding, orchestration, policy enforcement, and audit logging.

## Architecture

```
┌─────────────────┐     ┌─────────────────┐
│  rf-agent-1     │     │  rf-agent-2     │
│  Ubuntu 24.04   │     │  Ubuntu 24.04   │
│  token: agent1  │     │  token: agent2  │
└────────┬────────┘     └────────┬────────┘
         │ WebSocket              │ WebSocket
         │                        │
    ┌────┴────────────────────────┴────┐
    │         rf-relay                 │
    │         Ubuntu 24.04             │
    │         :9091 (host-mapped)      │
    └────────────────┬─────────────────┘
                     │ port 9091
                     │
              ┌──────┴──────┐
              │  rf CLI     │
              │  (your Mac) │
              └─────────────┘
```

All traffic is end-to-end encrypted (Noise XX). The relay pairs connections by meet token but never sees plaintext commands or output.

## Prerequisites

- Docker
- The `rf` CLI binary (build with `cargo build --release -p rf-cli` or install via `brew install egkristi/tap/ravenfabric`)
- For recordings: (`pipx install asciinema`)

## Quick Start

```bash
cd demos/multi-node-ubuntu
chmod +x setup.sh
./setup.sh
```

This creates 3 Ubuntu 24.04 containers with pre-built RavenFabric binaries (no compilation needed).

## Scenarios

### Remote Execution

| # | Scenario | Script | Description |
|---|----------|--------|-------------|
| 01 | Standard Exec | `scenarios/01-standard-exec.sh` | Basic command execution — hostname, file ops, exit codes |
| 02 | Streaming Exec | `scenarios/02-streaming-exec.sh` | Real-time incremental output with `--stream` flag |
| 03 | Background Exec | `scenarios/03-background-exec.sh` | Fire-and-forget with `--background` — returns job ID immediately |
| 04 | Interactive Shell | `scenarios/04-interactive-shell.sh` | Full PTY shell session (like SSH) |
| 05 | Orchestrated Exec | `scenarios/05-orchestrated-exec.sh` | Multi-agent playbooks — parallel, sequential, canary strategies |

### Port Forwarding

| # | Scenario | Script | Description |
|---|----------|--------|-------------|
| 06 | Local Forward | `scenarios/06-local-forward.sh` | SSH -L style: localhost:8080 → agent:8000 through tunnel |
| 07 | Remote Forward | `scenarios/07-remote-forward.sh` | SSH -R style: agent listens, tunnels back to client (RPC-level) |
| 08 | SOCKS5 Forward | `scenarios/08-socks5-forward.sh` | SSH -D style: dynamic SOCKS5 proxy through agent (RPC-level) |

### Operations & Security

| # | Scenario | Script | Description |
|---|----------|--------|-------------|
| 09 | Policy Enforcement | `scenarios/09-policy-enforcement.sh` | Apply restrictive policy, test allow/deny, hot-reload |
| 10 | Audit Inspection | `scenarios/10-audit-inspection.sh` | View structured JSON audit logs, filter by decision |
| 11 | Fleet Operations | `scenarios/11-fleet-operations.sh` | Fleet management — inventory, deploy, verify across agents |

### Running a Scenario

```bash
# Run any scenario individually
./scenarios/01-standard-exec.sh

# Or run commands manually
rf --relay ws://127.0.0.1:9091 exec --token agent1 'hostname'
```

> **Note:** After each command, the agent reconnects with a brief delay (~5s). Wait between consecutive commands to the same agent.

---

## Scenario Details

### 01 — Standard Remote Execution

Basic command execution over an encrypted channel. Every command goes through:
1. Noise XX handshake (mutual authentication)
2. Policy check (deny-by-default)
3. Execution with timeout and output limits
4. Audit log entry

```bash
# Simple command
rf --relay ws://127.0.0.1:9091 exec --token agent1 'hostname && uname -a'

# File operations
rf --relay ws://127.0.0.1:9091 exec --token agent2 'echo "test" > /tmp/file.txt && cat /tmp/file.txt'

# Exit code propagation
rf --relay ws://127.0.0.1:9091 exec --token agent1 'exit 42'
# process exits with code 42
```

### 02 — Streaming Execution

Real-time output streaming with `--stream`. Output arrives as it's produced — no buffering until command completion.

```bash
# Watch a countdown in real-time
rf --relay ws://127.0.0.1:9091 exec --stream --token agent1 \
    'for i in 5 4 3 2 1; do echo "Countdown: $i"; sleep 1; done'

# Stream log output
rf --relay ws://127.0.0.1:9091 exec --stream --token agent2 \
    'tail -f /var/log/syslog'
```

The `--stream` flag switches from `Action::Execute` (batch) to `Action::StreamExecute` (incremental). Each chunk is delivered as a separate `StreamChunk` response.

### 03 — Background Execution

Fire-and-forget mode with `--background`. The command starts and a job ID is returned immediately without waiting for completion.

```bash
# Start a background job
rf --relay ws://127.0.0.1:9091 exec --background --token agent1 \
    'sleep 10 && echo done > /tmp/result.txt'
# "background job started: <uuid> (pid 12345)"

# Check results later
rf --relay ws://127.0.0.1:9091 exec --token agent1 'cat /tmp/result.txt'
```

### 04 — Interactive Shell

Full PTY shell session — like SSH but with Noise XX encryption. Supports terminal resize and all interactive programs (vim, top, htop, etc.).

```bash
rf --relay ws://127.0.0.1:9091 shell --token agent1
# Opens bash prompt on the remote agent
# Type 'exit' to close
```

Platform: Unix only (PTY allocation requires Unix).

### 05 — Orchestrated Multi-Agent Execution

Execute across multiple agents using YAML playbooks with rollout strategies.

**Parallel update** (`scenarios/playbooks/parallel-update.yaml`):
```yaml
command: "apt-get update -qq && echo 'Updated' $(hostname)"
target:
  agents: [rf-agent-1, rf-agent-2]
strategy: parallel
on_failure: stop_only
timeout_secs: 60
```

**Canary deploy with rollback** (`scenarios/playbooks/canary-deploy.yaml`):
```yaml
command: "echo 'v2.0' > /opt/app/version.txt"
target:
  agents: [rf-agent-1, rf-agent-2]
strategy:
  canary:
    canary_count: 1
on_failure:
  rollback:
    command: "echo 'v1.0' > /opt/app/version.txt"
timeout_secs: 30
```

Strategies: `parallel`, `sequential`, `rolling` (batch_percent), `canary` (canary_count).

```bash
rf --relay ws://127.0.0.1:9091 playbook --token agent1 scenarios/playbooks/canary-deploy.yaml
```

### 06 — Local Port Forwarding

SSH -L equivalent: bind a local port and forward connections through the agent.

```bash
# Start a web server on the agent
rf --relay ws://127.0.0.1:9091 exec --token agent1 \
    'python3 -m http.server 8000 --directory /tmp/www &'

# Forward local port through the tunnel
rf --relay ws://127.0.0.1:9091 forward --token agent1 \
    -L 127.0.0.1:8080 -R 127.0.0.1:8000

# In another terminal:
curl http://localhost:8080
# Served from inside the agent container
```

### 07 — Remote Port Forwarding

SSH -R equivalent: the agent listens on a port and tunnels connections back to the client. Available at the RPC protocol level (`Action::RemoteForward`).

```
remote-client → agent:9000 → [encrypted tunnel] → client:3000
```

A dedicated CLI command (`rf forward --reverse`) is planned.

### 08 — SOCKS5 Dynamic Forwarding

SSH -D equivalent: a local SOCKS5 proxy tunnels all traffic through the agent's network. Available at the RPC level (`Action::Socks5Forward`).

```
Browser (SOCKS5) → localhost:1080 → [encrypted] → agent → destination
```

A dedicated CLI command (`rf forward --socks5`) is planned.

### 09 — Policy Enforcement

Demonstrates deny-by-default policy with allow/deny regex patterns.

```bash
# Allowed: matches allow patterns
rf exec --token agent1 'hostname'        # OK
rf exec --token agent1 'uname -a'        # OK

# Denied: matches deny patterns or not in allow list
rf exec --token agent1 'rm -rf /'        # DENIED
rf exec --token agent1 'curl evil.com'   # DENIED
```

### 10 — Audit Log Inspection

Every action generates a structured JSON-lines audit entry.

```bash
# View raw audit log
docker exec rf-agent-1 tail -5 /var/log/rf-audit.jsonl

# Pretty-print
docker exec rf-agent-1 tail -1 /var/log/rf-audit.jsonl | python3 -m json.tool

# Filter denied actions
docker exec rf-agent-1 grep denied /var/log/rf-audit.jsonl
```

### 11 — Fleet Operations

Manage multiple agents as a fleet from a single CLI.

```bash
# Collect info from all agents
for token in agent1 agent2; do
    rf --relay ws://127.0.0.1:9091 exec --token "$token" 'hostname'
done

# Deploy to all agents
for token in agent1 agent2; do
    rf --relay ws://127.0.0.1:9091 exec --token "$token" \
        'echo "v2.0" > /opt/app/version.txt'
done
```

---

## Asciinema Recordings

Each scenario has a corresponding recording script in `recordings/` that produces polished terminal recordings with simulated typing.

### Record All Scenarios

```bash
./recordings/record-all.sh
```

### Record Individual Scenarios

```bash
asciinema rec recordings/01-standard-exec.cast \
    --title "RavenFabric — Standard Execution" \
    --cols 100 --rows 30 \
    --command "bash recordings/01-standard-exec.sh"
```

### Play / Upload

```bash
asciinema play recordings/01-standard-exec.cast
asciinema upload recordings/01-standard-exec.cast
```

### Recording Files

| Recording | Scenario |
|-----------|----------|
| `recordings/01-standard-exec.sh` | Standard remote execution |
| `recordings/02-streaming-exec.sh` | Streaming output |
| `recordings/03-background-exec.sh` | Background execution |
| `recordings/04-interactive-shell.sh` | Interactive shell (concept) |
| `recordings/05-orchestrated-exec.sh` | Multi-agent orchestration |
| `recordings/06-local-forward.sh` | Local port forwarding |
| `recordings/09-policy-enforcement.sh` | Policy enforcement |
| `recordings/10-audit-inspection.sh` | Audit log inspection |
| `recordings/11-fleet-operations.sh` | Fleet operations |

---

## Playbooks

Playbook YAML files for orchestrated scenarios are in `scenarios/playbooks/`:

| File | Strategy | Description |
|------|----------|-------------|
| `parallel-update.yaml` | Parallel | Update all agents simultaneously |
| `sequential-healthcheck.yaml` | Sequential | Health check one agent at a time |
| `canary-deploy.yaml` | Canary | Deploy to 1 agent first, then remaining, with rollback |

---

## How It Works

1. **CLI connects** to the relay at `ws://127.0.0.1:9091` with a meet token
2. **Relay pairs** the CLI with the agent that registered with the same token
3. **Noise XX handshake** establishes a mutually authenticated encrypted channel
4. **CLI sends** an RPC request (msgpack, encrypted) containing the action
5. **Agent checks policy** before executing
6. **Agent executes** and returns the result (encrypted) through the relay
7. **Audit log** records the action, decision, caller, and duration

The relay never decrypts payload — it's a dumb pipe.

## Security Properties

| Property | Details |
|----------|---------|
| Encryption | Noise XX (ChaCha20-Poly1305 + BLAKE2s) |
| Forward secrecy | Per-session ephemeral Curve25519 keys |
| Authentication | Mutual — both CLI and agent verify peer public key |
| Replay protection | Monotonic nonce counter per session |
| Tamper detection | Poly1305 MAC on every frame |
| Policy enforcement | Every command checked against YAML policy |
| Audit logging | Every action logged to `/var/log/rf-audit.jsonl` |

## Troubleshooting

### Agent not responding
```bash
docker exec rf-agent-1 cat /var/log/rf-agent.log

RELAY_IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' rf-relay)
docker exec rf-agent-1 bash -c 'pkill rf-agent'
docker exec -d rf-agent-1 bash -c "RUST_LOG=info rf-agent \
    --relay ws://${RELAY_IP}:9090 --id rf-agent-1 --token agent1 \
    --policy-path /etc/ravenfabric/policy.yaml \
    --audit-path /var/log/rf-audit.jsonl \
    --key-path /etc/ravenfabric/agent.key \
    > /var/log/rf-agent.log 2>&1"
```

### Consecutive commands failing
After each exec, the agent session closes and reconnects with exponential backoff. Wait ~5 seconds between commands to the same agent.

### Custom relay port
```bash
RELAY_PORT=9999 ./setup.sh
rf --relay ws://127.0.0.1:9999 exec --token agent1 'hostname'
```

## Teardown

```bash
./setup.sh teardown
```

## File Structure

```
demos/multi-node-ubuntu/
├── setup.sh                                 # Setup and teardown script
├── README.md                                # This file
├── scenarios/
│   ├── 01-standard-exec.sh                  # Standard remote execution
│   ├── 02-streaming-exec.sh                 # Streaming execution
│   ├── 03-background-exec.sh               # Background execution
│   ├── 04-interactive-shell.sh              # Interactive shell
│   ├── 05-orchestrated-exec.sh             # Multi-agent orchestration
│   ├── 06-local-forward.sh                  # Local port forwarding
│   ├── 07-remote-forward.sh                 # Remote port forwarding (concept)
│   ├── 08-socks5-forward.sh                 # SOCKS5 forwarding (concept)
│   ├── 09-policy-enforcement.sh             # Policy enforcement
│   ├── 10-audit-inspection.sh               # Audit log inspection
│   ├── 11-fleet-operations.sh               # Fleet operations
│   └── playbooks/
│       ├── parallel-update.yaml             # Parallel execution playbook
│       ├── sequential-healthcheck.yaml      # Sequential health check
│       └── canary-deploy.yaml               # Canary deploy with rollback
└── recordings/
    ├── helpers.sh                           # Shared recording utilities
    ├── record-all.sh                        # Record all scenarios
    ├── 01-standard-exec.sh                  # Recording: standard exec
    ├── 02-streaming-exec.sh                 # Recording: streaming
    ├── 03-background-exec.sh               # Recording: background
    ├── 04-interactive-shell.sh              # Recording: shell (concept)
    ├── 05-orchestrated-exec.sh             # Recording: orchestration
    ├── 06-local-forward.sh                  # Recording: local forward
    ├── 09-policy-enforcement.sh             # Recording: policy
    ├── 10-audit-inspection.sh               # Recording: audit
    └── 11-fleet-operations.sh               # Recording: fleet ops
```
