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
| 12 | Policy Denial | `scenarios/12-policy-denial.sh` | Full deny-by-default demo — allowed vs denied commands + audit log |
| 13 | Audit Trail | `scenarios/13-audit-trail.sh` | Structured JSON audit logging — every action recorded, append-only |
| 14 | Port Forwarding | `scenarios/14-port-forwarding.sh` | Local, reverse, and SOCKS5 port forwarding through encrypted tunnels |
| 15 | Dev Mode | `scenarios/15-dev-mode.sh` | Zero-setup dev environment — relay + agent in one process, no config |
| 16 | Fleet Orchestration | `scenarios/16-fleet-orchestration.sh` | Multi-agent playbooks — parallel, sequential, rolling, canary strategies |
| 17 | Human Approval | `scenarios/17-human-approval.sh` | Human-in-the-loop approval gate for AI-controlled agents via MCP |

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

### 12 — Policy Denial

Full deny-by-default demonstration. Applies a restrictive policy that only allows safe read-only commands, then tests both allowed and denied commands. Every denial is recorded in the audit log.

```bash
# Run the full scenario
./scenarios/12-policy-denial.sh

# What it does:
# 1. Shows the current permissive policy
# 2. Applies a restrictive policy (only hostname, uname, uptime, cat /etc/*)
# 3. Tests allowed commands (hostname, uname, uptime) — all succeed
# 4. Tests denied commands (rm -rf, curl, apt, shutdown, chmod) — all blocked
# 5. Inspects audit log showing denial entries
# 6. Restores permissive policy
```

Denied command categories:

- **Destructive**: `rm -rf`, `mkfs`, `dd`
- **Network**: `curl`, `wget`
- **System control**: `shutdown`, `reboot`
- **Package management**: `apt`, `pip`
- **Permission changes**: `chmod`, `chown`

---

### 13 — Audit Trail

Every action — allowed or denied — produces a structured JSON audit entry. Each agent maintains its own independent, append-only log.

```bash
# Run the full scenario
./scenarios/13-audit-trail.sh

# View raw audit log (JSON-lines format)
docker exec rf-agent-1 tail -3 /var/log/rf-audit.jsonl

# Count entries per agent
docker exec rf-agent-1 wc -l < /var/log/rf-audit.jsonl
docker exec rf-agent-2 wc -l < /var/log/rf-audit.jsonl
```

Audit entry fields:

- **timestamp**: ISO 8601 when the action occurred
- **command**: the command string that was executed (or attempted)
- **decision**: `allowed` or `denied`
- **caller**: identity of the requesting client
- **duration**: execution time in milliseconds
- **exit_code**: process exit code (for allowed commands)

---

### 14 — Port Forwarding

SSH-style local port forwarding through Noise XX encrypted tunnels. Access remote services on agents without firewall changes.

```bash
# Run the full scenario
./scenarios/14-port-forwarding.sh

# Start a web server on the agent
rf --relay ws://127.0.0.1:9091 exec --token agent1 \
    'python3 -m http.server 8000 --directory /tmp/www &'

# Forward local port through encrypted tunnel
rf --relay ws://127.0.0.1:9091 forward --token agent1 \
    -L 127.0.0.1:8080 -R 127.0.0.1:8000

# Access the service locally
curl http://localhost:8080
```

Forwarding types:

- **Local** (`-L`): `localhost:8080 → agent:8000` (SSH -L equivalent)
- **Reverse** (`--reverse`): `agent:9000 → localhost:3000` (SSH -R equivalent)
- **SOCKS5** (`--socks5`): `localhost:1080 → agent → destination` (SSH -D equivalent)

---

### 15 — Dev Mode (Zero-Setup)

One command starts a complete RavenFabric environment — relay + agent in a single process. No Docker, no config files, no key exchange.

```bash
# Run the full scenario
./scenarios/15-dev-mode.sh

# Start dev mode
rf dev

# In another terminal:
rf exec --token dev 'hostname'
rf exec --token dev --stream 'for i in 1 2 3; do echo $i; sleep 1; done'

# Custom port and bind
rf dev --port 8080
rf dev --bind 0.0.0.0 --port 8080
```

Dev mode features:

- **Instant**: < 1 second startup, zero configuration
- **Ephemeral**: In-memory keys, no files written to disk
- **Permissive**: All commands allowed (development only)
- **Same syntax**: `rf exec` and `rf forward` work identically

---

### 16 — Fleet Orchestration

Multi-agent orchestration with YAML playbooks. Execute commands across a fleet using parallel, sequential, rolling, and canary strategies with automatic rollback.

```bash
# Run the full scenario
./scenarios/16-fleet-orchestration.sh

# Fleet inventory
for token in agent1 agent2; do
    rf --relay ws://127.0.0.1:9091 exec --token $token 'hostname'
done

# Run a playbook with canary strategy
rf --relay ws://127.0.0.1:9091 playbook --token agent1 \
    scenarios/playbooks/canary-deploy.yaml
```

Playbook YAML format:

```yaml
command: "echo 'Deploying v2.0' && mkdir -p /opt/app && echo v2.0 > /opt/app/version.txt"
target:
  agents: [rf-agent-1, rf-agent-2]
strategy:
  canary: { canary_count: 1 }
on_failure:
  rollback:
    command: "echo v1.0 > /opt/app/version.txt"
timeout_secs: 30
```

Strategies:

- **parallel**: All agents simultaneously
- **sequential**: One at a time, stop on failure
- **rolling**: Batches (e.g. 25% at a time)
- **canary**: Test on N agents first, then the rest

---

### 17 — Human Approval for AI Agents

Human-in-the-loop approval gate for AI-controlled agents. AI agents connect via MCP server and must request approval for high-risk operations before execution. Approval enforcement is mandatory and cryptographically verified.

```bash
# Run the full scenario
./scenarios/17-human-approval.sh
```

Approval workflow:

1. AI calls `rf_request_approval(command, reason)` → gets `approval_id`
2. Operator sees the request (stderr / webhook / Slack)
3. Operator calls `approve(id)` or `deny(id)`
4. AI polls `rf_check_approval(id)` → `APPROVED` or `DENIED`
5. AI passes `approval_id` to `rf_exec(command, approval_id)` — only executes if approved

Enforcement guarantees:

- **Command hash binding**: Each approval is SHA-256 bound to the exact command — the AI cannot substitute a different command after approval
- **One-time-use**: Each approval can be consumed exactly once — reuse returns DENIED
- **TTL expiration**: Approvals expire after 30 minutes — stale approvals return DENIED
- **Pattern-based**: Operator configures which commands require approval via `--approval-pattern` regex

MCP tools involved:

- **`rf_request_approval`**: Submit operation + command + reason for review
- **`rf_check_approval`**: Poll approval status (`PENDING` / `APPROVED` / `DENIED`)
- **`rf_exec`**: Execute command, optionally with `approval_id` for approval-required commands
- **`rf_query_policy`**: Dry-run policy check before requesting approval

Defense in depth:

- **Policy engine**: Deny-by-default (first gate)
- **Human approval**: Operator gate for high-risk ops with hash verification (second gate)
- **Rate limiting**: 60 requests/min per session
- **Anomaly detection**: Behavioral baseline alerts
- **Audit trail**: Every action logged

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
| `recordings/12-policy-denial.sh` | Policy denial |
| `recordings/13-audit-trail.sh` | Audit trail |
| `recordings/14-port-forwarding.sh` | Port forwarding |
| `recordings/15-dev-mode.sh` | Dev mode (zero-setup) |
| `recordings/16-fleet-orchestration.sh` | Fleet orchestration |
| `recordings/17-human-approval.sh` | Human approval for AI agents |

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
│   ├── 12-policy-denial.sh                  # Policy denial demo
│   ├── 13-audit-trail.sh                    # Audit trail demo
│   ├── 14-port-forwarding.sh                # Port forwarding demo
│   ├── 15-dev-mode.sh                       # Dev mode (zero-setup) demo
│   ├── 16-fleet-orchestration.sh            # Fleet orchestration demo
│   ├── 17-human-approval.sh                 # Human approval for AI agents
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
    ├── 11-fleet-operations.sh               # Recording: fleet ops
    ├── 12-policy-denial.sh                  # Recording: policy denial
    ├── 13-audit-trail.sh                    # Recording: audit trail
    ├── 14-port-forwarding.sh                # Recording: port forwarding
    ├── 15-dev-mode.sh                       # Recording: dev mode
    ├── 16-fleet-orchestration.sh            # Recording: fleet orchestration
    └── 17-human-approval.sh                 # Recording: human approval
```
