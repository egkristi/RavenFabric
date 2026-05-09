#!/usr/bin/env bash
# Fleet Orchestration — Kubernetes + CloudNativePG
#
# Demonstrates fleet orchestration in a Kubernetes context. Use playbooks
# to manage database operations across primary and replica instances,
# run health checks, and coordinate rolling maintenance.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RF="${RF_CLI:-rf}"
RELAY="${RF_RELAY:-ws://127.0.0.1:9093}"

echo "=== Fleet Orchestration — Kubernetes Context ==="
echo ""

# --- Part 1: Database Fleet Inventory ---

echo "--- Part 1: Database Fleet Inventory ---"
echo ""
echo "  Query PostgreSQL cluster status through the agent."
echo ""

echo "[1] Cluster node status:"
$RF --relay "$RELAY" exec --token cnpg \
    'echo "  Primary (rw): pg-cluster-rw" && echo "  Replica (ro): pg-cluster-ro"'
echo ""
sleep 6

echo "[2] Database health check:"
$RF --relay "$RELAY" exec --token cnpg \
    'PGPASSWORD=$POSTGRES_PASSWORD psql -h pg-cluster-rw -U postgres -d app -c "SELECT version();" -t 2>/dev/null | head -1 || echo "  (connection check)"'
echo ""
sleep 6

# --- Part 2: Coordinated Database Operations ---

echo "--- Part 2: Coordinated Database Operations ---"
echo ""
echo "  In production, use playbooks to coordinate across pods:"
echo ""
echo "  Playbook: rolling-db-maintenance.yaml"
echo "  ---"
echo "  command: \"PGPASSWORD=\$POSTGRES_PASSWORD psql -h pg-cluster-rw -U postgres -d app -c 'VACUUM ANALYZE;'\""
echo "  target:"
echo "    agents:"
echo "      - rf-agent-cnpg"
echo "  strategy: sequential"
echo "  on_failure: stop_only"
echo "  timeout_secs: 120"
echo "  ---"
echo ""
sleep 6

# --- Part 3: K8s vs Traditional Fleet Management ---

echo "--- Part 3: RavenFabric vs kubectl for Fleet Ops ---"
echo ""
echo "  ┌──────────────────────┬─────────────────────────┬──────────────────────────┐"
echo "  │                      │ kubectl                 │ rf playbook              │"
echo "  ├──────────────────────┼─────────────────────────┼──────────────────────────┤"
echo "  │ Multi-pod commands   │ for pod in ...; do      │ Single playbook YAML     │"
echo "  │ Rollback on failure  │ Manual                  │ Automatic                │"
echo "  │ Canary strategy      │ Not built-in            │ canary: { count: 1 }     │"
echo "  │ Works through NAT    │ Needs kubeconfig        │ Yes (relay-based)        │"
echo "  │ Audit trail          │ K8s audit log           │ Per-command JSON audit    │"
echo "  │ Cross-cluster        │ Context switching       │ Change relay URL          │"
echo "  └──────────────────────┴─────────────────────────┴──────────────────────────┘"
echo ""
sleep 6

# --- Key Takeaways ---

echo "=== Key Takeaways ==="
echo ""
echo "  1. Playbooks coordinate database operations across pods"
echo "  2. Canary strategy validates changes before full rollout"
echo "  3. Automatic rollback protects against failed migrations"
echo "  4. Works through NAT — no kubeconfig required on the client"
echo ""
echo "=== Fleet Orchestration Scenario Complete ==="
