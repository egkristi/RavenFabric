#!/usr/bin/env bash
# Scenario 5: Network Topology
#
# Maps the network topology of the fleet: IP addresses, routes,
# listening ports, and connectivity between agents.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9092}"
RF="${RF_CLI:-rf}"
AGENTS=("collector" "webserver" "database")

echo "=== Scenario 5: Network Topology ==="
echo ""

# 1. IP addresses
echo "[1] IP addresses:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'ip addr show | grep "inet " | grep -v 127.0.0.1' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 2. Default routes
echo "[2] Default routes:"
for token in "${AGENTS[@]}"; do
    echo -n "  $token: "
    $RF --relay "$RELAY" exec --token "$token" 'ip route | grep default' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 3. Listening ports
echo "[3] Listening ports:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'ss -tlnp 2>/dev/null || netstat -tlnp 2>/dev/null || echo "No network tools available"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 4. Network interface stats
echo "[4] Network interface stats:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'cat /proc/net/dev | head -5' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 5. DNS configuration
echo "[5] DNS configuration:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'cat /etc/resolv.conf' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 6. Topology summary
echo "[6] Fleet topology summary:"
echo "  Collecting IPs from all agents..."
for token in "${AGENTS[@]}"; do
    IP=$($RF --relay "$RELAY" exec --token "$token" 'ip addr show | grep "inet " | grep -v 127.0.0.1 | head -1 | awk "{print \$2}"' 2>/dev/null | grep -v "^2" | tr -d '\n')
    HOSTNAME=$($RF --relay "$RELAY" exec --token "$token" 'hostname' 2>/dev/null | grep -v "^2" | tr -d '\n')
    echo "  $token ($HOSTNAME): $IP"
    sleep 6
done
echo ""

echo "=== Scenario 5 Complete ==="
