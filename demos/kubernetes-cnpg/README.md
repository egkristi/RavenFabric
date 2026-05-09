# Kubernetes + CloudNativePG Demo

Deploy a CloudNativePG PostgreSQL cluster with a RavenFabric agent for remote database access and command execution — all in a local Kubernetes cluster.

## Architecture

```
                    ┌─── Kubernetes (Rancher Desktop) ───────────────┐
                    │  namespace: ravenfabric                        │
                    │                                                │
                    │  ┌──────────────────┐   ┌───────────────────┐  │
                    │  │   CNPG Cluster    │   │    rf-agent       │  │
                    │  │                  │   │    Deployment     │  │
                    │  │  pg-cluster-1    │◄──│                   │  │
                    │  │  (primary)       │   │  postgres:17      │  │
                    │  │                  │   │  + rf-agent binary │  │
                    │  │  pg-cluster-2    │   │                   │  │
                    │  │  (replica)       │   │  Token: cnpg      │  │
                    │  └──────────────────┘   └─────────┬─────────┘  │
                    │       ▲ pg-cluster-rw              │            │
                    │       │ (ClusterIP)                │ ws://      │
                    └───────┼────────────────────────────┼────────────┘
                            │                           │
                            │              ┌────────────▼────────────┐
                            │              │  rf-relay-k8s (Docker)  │
                            │              │  Ubuntu 24.04           │
                            │              │  :9093 (host)           │
                            │              └────────────┬────────────┘
                            │                           │ port 9093
                            │                           │
                            │              ┌────────────▼────────────┐
                            │              │     rf CLI (your Mac)   │
                            │              └─────────────────────────┘
```

The rf-agent pod has PostgreSQL environment variables (PGHOST, PGUSER, PGPASSWORD) pre-configured from the CNPG superuser secret, so `psql` commands work without extra arguments.

## Prerequisites

- Docker (Colima or Docker Desktop)
- Kubernetes (Rancher Desktop or similar)
- CloudNativePG operator installed in the cluster
- The `rf` CLI binary

### Installing CNPG Operator

```bash
helm repo add cnpg https://cloudnative-pg.github.io/charts
helm upgrade --install cnpg-operator cnpg/cloudnative-pg \
    --namespace cnpg-system --create-namespace --wait
```

## Quick Start

```bash
cd demos/kubernetes-cnpg

# Deploy everything (relay + CNPG cluster + rf-agent)
./setup.sh

# Query PostgreSQL via RavenFabric
rf --relay ws://127.0.0.1:9093 exec --token cnpg 'psql -c "SELECT version();"'

# Teardown
./setup.sh teardown
```

## What Gets Deployed

| Component | Type | Description |
|-----------|------|-------------|
| `rf-relay-k8s` | Docker container | Relay broker (Ubuntu 24.04, port 9093) |
| `ravenfabric` | K8s namespace | Isolated namespace for all demo resources |
| `pg-cluster` | CNPG Cluster | 2-instance PostgreSQL (primary + replica) |
| `pg-cluster-rw` | K8s Service | Read-write endpoint (connects to primary) |
| `pg-cluster-ro` | K8s Service | Read-only endpoint (connects to replicas) |
| `pg-cluster-r` | K8s Service | Any endpoint (connects to any instance) |
| `rf-agent` | K8s Deployment | RavenFabric agent with psql client |
| `rf-agent-policy` | K8s ConfigMap | Policy allowing all commands |

## Usage Examples

```bash
RELAY="ws://127.0.0.1:9093"

# Check the agent pod's OS
rf --relay $RELAY exec --token cnpg 'cat /etc/os-release | head -2'

# PostgreSQL version
rf --relay $RELAY exec --token cnpg 'psql -c "SELECT version();"'

# List databases
rf --relay $RELAY exec --token cnpg 'psql -c "\l"'

# Check replication status
rf --relay $RELAY exec --token cnpg 'psql -c "SELECT client_addr, state, sync_state FROM pg_stat_replication;"'

# Create a table and query it
rf --relay $RELAY exec --token cnpg 'psql -c "CREATE TABLE demo(id serial PRIMARY KEY, name text); INSERT INTO demo(name) VALUES ('"'"'hello'"'"'), ('"'"'ravenfabric'"'"'); SELECT * FROM demo;"'

# Database size
rf --relay $RELAY exec --token cnpg 'psql -c "SELECT pg_database.datname, pg_size_pretty(pg_database_size(pg_database.datname)) FROM pg_database ORDER BY pg_database_size(pg_database.datname) DESC;"'

# Active connections
rf --relay $RELAY exec --token cnpg 'psql -c "SELECT datname, usename, client_addr, state FROM pg_stat_activity WHERE state IS NOT NULL;"'

# Backup with pg_dump
rf --relay $RELAY exec --token cnpg 'pg_dump --schema-only app'
```

## How It Works

1. **Relay** runs on Docker, exposed on host port 9093
2. **CNPG operator** provisions a 2-instance PostgreSQL cluster in the `ravenfabric` namespace
3. **rf-agent Deployment** runs alongside the cluster:
   - InitContainer downloads the static `rf-agent` binary from GitHub releases
   - Main container (`postgres:17`) runs the agent with PostgreSQL client tools
   - Environment variables (`PGHOST`, `PGUSER`, `PGPASSWORD`) are sourced from the CNPG superuser secret
   - Agent connects to the relay via the host's network IP
4. **rf CLI** on your Mac connects to the relay on `localhost:9093` and executes commands on the agent pod

## Gatekeeper Policy Handling

The setup script automatically exempts the `ravenfabric` namespace from Gatekeeper constraints (image whitelist, readOnlyRootFilesystem, etc.) to allow standard Docker Hub images. The teardown script reverts these exemptions.

## Port Assignment

This demo uses port **9093** (configurable via `RELAY_PORT` env var) to avoid conflicts with other demos.

```bash
RELAY_PORT=9099 ./setup.sh
```

## Relay Host Detection

The setup script auto-detects the host IP that Kubernetes pods can use to reach the Docker relay. Override with:

```bash
RELAY_HOST=192.168.1.100 ./setup.sh
```

## Commands

```bash
./setup.sh              # Deploy everything
./setup.sh teardown     # Remove everything
./setup.sh status       # Show component status
./setup.sh verify       # Test connectivity and queries
```

## Troubleshooting

### rf-agent pod is CrashLoopBackOff
Check the agent logs:
```bash
kubectl logs -n ravenfabric -l app=rf-agent -c rf-agent
```
Common causes: relay not reachable (wrong host IP), binary download failed (check init container logs).

### CNPG cluster pods not starting
```bash
kubectl describe cluster pg-cluster -n ravenfabric
kubectl get events -n ravenfabric --sort-by='.lastTimestamp'
```

### Init container failing to download binary
```bash
kubectl logs -n ravenfabric -l app=rf-agent -c download-rf-agent
```

### Relay not reachable from k8s
Check the detected host IP and verify connectivity:
```bash
kubectl run --rm -it test-curl --image=curlimages/curl --restart=Never -n ravenfabric -- curl -v ws://HOST_IP:9093
```

### Agent not responding after command
The agent reconnects with exponential backoff. Wait ~6 seconds between commands.
