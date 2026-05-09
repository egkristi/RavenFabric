#!/usr/bin/env bash
# Scenario: Port Forwarding (Kubernetes + CloudNativePG)
#
# Demonstrates local port forwarding to access the PostgreSQL cluster
# directly from your local machine through an encrypted RavenFabric tunnel.
# This is an alternative to kubectl port-forward that works through NAT
# and firewalls without direct cluster access.
#
# Architecture:
#   psql → localhost:5432 → [Noise XX] → rf-agent pod → pg-cluster-rw:5432
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9093}"
RF="${RF_CLI:-rf}"
NAMESPACE="ravenfabric"

echo "=== Port Forwarding: Kubernetes + CloudNativePG Demo ==="
echo ""

# 1. Show current PostgreSQL connectivity via exec
echo "[1] Current access: commands tunneled via rf exec"
$RF --relay "$RELAY" exec --token cnpg 'psql -c "SELECT current_database(), current_user;"'
echo ""
sleep 6

# 2. Show the agent's view of PostgreSQL
echo "[2] Agent pod's PostgreSQL environment:"
$RF --relay "$RELAY" exec --token cnpg 'echo "PGHOST=$PGHOST PGPORT=$PGPORT PGUSER=$PGUSER PGDATABASE=$PGDATABASE"'
echo ""
sleep 6

# 3. Verify the pg-cluster-rw service
echo "[3] PostgreSQL service endpoint inside K8s:"
$RF --relay "$RELAY" exec --token cnpg 'getent hosts pg-cluster-rw 2>/dev/null || nslookup pg-cluster-rw 2>/dev/null | tail -2'
echo ""
sleep 6

# 4. Show the forward command
echo "[4] Port forward command (for direct psql access):"
echo ""
echo "  # Forward localhost:5432 to PostgreSQL through the encrypted tunnel"
echo "  rf --relay $RELAY forward --token cnpg -L 127.0.0.1:5432 -R pg-cluster-rw:5432"
echo ""
echo "  # Then connect directly with psql from your Mac:"
echo "  psql -h 127.0.0.1 -p 5432 -U <user> -d app"
echo ""

# 5. Also show forwarding to the read-only replica endpoint
echo "[5] Forward to read-only replica (for reporting queries):"
echo ""
echo "  # Forward to pg-cluster-ro (read-only replicas)"
echo "  rf --relay $RELAY forward --token cnpg -L 127.0.0.1:5433 -R pg-cluster-ro:5432"
echo ""
echo "  # Read-only replica access:"
echo "  psql -h 127.0.0.1 -p 5433 -U <user> -d app"
echo ""

# 6. Compare with kubectl port-forward
echo "[6] Comparison with kubectl port-forward:"
echo ""
echo "  Method                  Works through NAT?  E2E encrypted?  Audited?"
echo "  ─────────────────────────────────────────────────────────────────────"
echo "  kubectl port-forward    No (needs kubeconfig) No            No"
echo "  rf forward              Yes                   Yes (Noise XX) Yes"
echo ""

# 7. Show that the forward would be audited
echo "[7] Port forward sessions are audited:"
$RF --relay "$RELAY" exec --token cnpg 'tail -2 /tmp/rf-audit.jsonl'
echo ""
sleep 6

echo "=== Port Forwarding Demo Complete ==="
echo ""
echo "Key takeaways:"
echo "  - Forward PostgreSQL ports through Noise XX encrypted tunnels"
echo "  - Works through NAT/firewalls — no direct cluster access needed"
echo "  - Separate forwards for read-write (pg-cluster-rw) and read-only (pg-cluster-ro)"
echo "  - All forwarding sessions are audited"
echo "  - Alternative to kubectl port-forward that works remotely"
