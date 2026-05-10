# Direct Connection Demo

Connect to a Linux container directly — no relay needed. The agent listens on a port (like sshd) and the CLI connects straight to it over WebSocket with full Noise XX mutual authentication.

## Architecture

```
┌──────────────────────┐
│  rf-agent             │
│  Ubuntu 24.04         │
│  --listen 0.0.0.0:9999│
│  :9999 (host-mapped)  │
└──────────┬────────────┘
           │ WebSocket (Noise XX)
           │ port 9999
    ┌──────┴──────┐
    │  rf CLI     │
    │  (your Mac) │
    │  --connect  │
    └─────────────┘
```

No relay, no meet tokens, no intermediary. The connection is point-to-point — like SSH but with Noise XX instead of certificates and policy-bounded execution instead of shell access.

## Prerequisites

- Docker
- The `rf` CLI binary (`cargo build --release -p rf-cli` or `brew install egkristi/tap/ravenfabric`)

## Quick Start

```bash
cd demos/direct-connection
chmod +x setup.sh
./setup.sh
```

This creates a single Ubuntu 24.04 container running `rf-agent` in listen mode on port 9999.

## Scenarios

| # | Scenario | Script | Description |
|---|----------|--------|-------------|
| 01 | Direct Exec | `scenarios/01-direct-exec.sh` | Remote command execution via direct connection |
| 02 | System Info | `scenarios/02-system-info.sh` | Collect system information (hostname, OS, resources) |
| 03 | Policy Denial | `scenarios/03-policy-denial.sh` | Verify deny-by-default policy blocks dangerous commands |
| 04 | Audit Trail | `scenarios/04-audit-trail.sh` | Inspect the structured audit log after execution |

## Manual Usage

```bash
# Execute on the agent directly (no relay, no token)
rf --connect ws://127.0.0.1:9999 exec --token unused 'hostname && uname -a'

# Stream output in real time
rf --connect ws://127.0.0.1:9999 exec --token unused --stream 'for i in 1 2 3; do echo "step $i"; sleep 1; done'

# Check agent status
rf --connect ws://127.0.0.1:9999 status --token unused
```

> **Note:** The `--token` flag is still required by the CLI parser but is ignored in direct-connect mode (no relay pairing needed).

## Teardown

```bash
./setup.sh teardown
```

## How It Differs from Relay Mode

| Aspect | Relay Mode | Direct Mode |
|--------|-----------|-------------|
| Network | Agent → Relay ← CLI | CLI → Agent |
| Topology | Hub-and-spoke | Point-to-point |
| Firewall | Agent only needs outbound | Agent needs inbound port |
| Meet token | Required for pairing | Not used |
| Use case | NAT traversal, fleet mgmt | Lab, LAN, SSH replacement |
| Security | E2E encrypted via relay | E2E encrypted direct |

Both modes use identical Noise XX handshakes, policy enforcement, and audit logging.
