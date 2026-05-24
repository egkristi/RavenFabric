# RavenFabric Demo — Secure Fleet Data Collection

Collect system inventory, resource metrics, logs, configurations, and security posture
from a heterogeneous fleet — all through encrypted channels with a read-only policy.

## Architecture

```text
┌─────────────────────────────────────────────────────────┐
│  Your Machine                                           │
│  ┌─────────────────────────────────────────────────┐    │
│  │  rf (CLI)                                       │    │
│  │  Scenarios: inventory, logs, configs, security  │    │
│  └──────────────────────┬──────────────────────────┘    │
│                         │ ws://127.0.0.1:9092           │
├─────────────────────────┼───────────────────────────────┤
│  Docker Network         │                               │
│  ┌──────────────────────┴──────────────────────────┐    │
│  │  rf-relay (stateless broker, port 9092)         │    │
│  └──┬──────────────────┬──────────────────┬────────┘    │
│     │                  │                  │             │
│  ┌──┴───────────┐  ┌──┴───────────┐  ┌──┴───────────┐  │
│  │ rf-collector  │  │ rf-webserver │  │ rf-database  │  │
│  │ (aggregator)  │  │ (web role)   │  │ (db role)    │  │
│  │ token:        │  │ token:       │  │ token:       │  │
│  │  collector    │  │  webserver   │  │  database    │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└─────────────────────────────────────────────────────────┘
```

**Key point:** The relay never decrypts payload. All data flows end-to-end encrypted
between the CLI and each agent via Noise XX mutual authentication.

## Prerequisites

- Docker
- Rust toolchain (to build `rf` CLI)
- ~500 MB disk (Docker images + binaries)

## Quick Start

```bash
# Build the CLI
cargo build --release -p rf-cli

# Start the fleet (4 containers)
./setup.sh

# Run all scenarios
./scenarios/01-system-inventory.sh
./scenarios/02-resource-monitoring.sh
./scenarios/03-log-collection.sh
./scenarios/04-config-audit.sh
./scenarios/05-network-topology.sh
./scenarios/06-security-scan.sh
./scenarios/07-fleet-snapshot.sh
./scenarios/08-policy-boundary.sh

# Tear down
./setup.sh teardown
```

## Scenarios

| # | Scenario | What It Collects |
|---|----------|------------------|
| 01 | System Inventory | Hostname, OS, kernel, CPU, memory, disk, uptime |
| 02 | Resource Monitoring | Load averages, memory utilization, disk I/O, top processes |
| 03 | Log Collection | Access logs, query logs, audit trails, error patterns |
| 04 | Configuration Audit | App configs, policy checksums, OS settings, drift detection |
| 05 | Network Topology | IP addresses, routes, listening ports, interface stats |
| 06 | Security Scan | Users, SUID binaries, world-writable files, open ports |
| 07 | Fleet Snapshot | Full point-in-time collection of all metrics per agent |
| 08 | Policy Boundary | Demonstrates allowed reads vs denied writes/destructive ops |

## Policy

The demo uses a **read-only data collection policy** — the strictest useful policy for
monitoring and compliance use cases:

**Allowed:**

- System info: `hostname`, `uname`, `uptime`, `date`, `whoami`, `id`
- Resource data: `free`, `df`, `ps`, `top`, `/proc/*` reads
- Log reads: `tail`, `head`, `grep`, `wc`, `find` on `/var/log`
- Config reads: `cat`, `ls`, `stat`, `sha256sum`
- Network info: `ip addr`, `ip route`, `ss`, `netstat`

**Denied (explicit denylist + deny-by-default):**

- Destructive: `rm`, `shutdown`, `reboot`, `mkfs`, `dd`
- Downloads: `curl`, `wget`
- Package management: `apt`, `pip`
- Process control: `kill`, `pkill`, `systemctl`, `service`
- Permission changes: `chmod`, `chown`
- Mount operations: `mount`, `umount`
- Firewall: `iptables`

**Filesystem restrictions:**

- Allowed paths: `/proc`, `/sys`, `/etc`, `/var/log`, `/opt/app`
- Denied paths: `/etc/shadow`, `/etc/gshadow`, `/root`

## Playbooks

Pre-built orchestration playbooks in `scenarios/playbooks/`:

| Playbook | Strategy | Purpose |
|----------|----------|---------|
| `fleet-inventory.yaml` | parallel | System inventory from all agents simultaneously |
| `log-sweep.yaml` | serial | Collect recent logs from each agent sequentially |
| `security-audit.yaml` | parallel | Security posture check across the fleet |

## Simulated Data

Each agent is seeded with role-appropriate data:

- **rf-webserver**: 50 HTTP access log entries, nginx config, app config
- **rf-database**: 30 query log entries (with varying latencies), PostgreSQL-style config
- **rf-collector**: Aggregation config with collection interval and retention settings

## Customization

| Variable | Default | Description |
|----------|---------|-------------|
| `RELAY_PORT` | `9092` | Host port for relay |
| `RF_RELAY` | `ws://127.0.0.1:9092` | Relay URL (for scenarios) |
| `RF_CLI` | `rf` | Path to `rf` binary |

```bash
# Example: custom port
RELAY_PORT=9095 ./setup.sh
RF_RELAY=ws://127.0.0.1:9095 ./scenarios/01-system-inventory.sh
```

## Security Notes

- All communication is end-to-end encrypted (Noise XX + ChaCha20-Poly1305)
- The relay is stateless and never sees plaintext
- Policy is enforced at the agent — the CLI cannot bypass it
- Audit logs record every command executed on each agent
- The read-only policy prevents any modification to the target systems
