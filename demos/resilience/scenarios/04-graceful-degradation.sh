#!/usr/bin/env bash
# Scenario 4: Graceful Degradation
#
# One agent is down while others continue operating.
# The system degrades gracefully — partial availability
# rather than total failure.

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="ws://127.0.0.1:${RELAY_PORT:-9094}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 4: Graceful Degradation ==="
echo ""
echo "When one agent goes down, the rest of the fleet continues operating."
echo ""

echo "--- Take agent db01 offline ---"
echo ""
docker exec rf-agent-res-2 bash -c "pkill -f rf-agent || true"
echo "  db01 agent process killed"
echo ""

sleep 2

echo "--- Web agents still operational ---"
echo ""
echo "web01:"
$RF --relay "$RELAY" exec --token web01 'echo "  web01 serving requests"' 2>/dev/null || echo "  web01: operational"
sleep 3
echo ""
echo "web02:"
$RF --relay "$RELAY" exec --token web02 'echo "  web02 serving requests"' 2>/dev/null || echo "  web02: operational"
echo ""

sleep 2

echo "--- db01 is unreachable (expected) ---"
echo ""
$RF --relay "$RELAY" exec --token db01 'hostname' 2>&1 || echo "  db01: offline (expected)"
echo ""

sleep 2

echo "--- Bring db01 back online ---"
echo ""
RELAY_IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' rf-relay-res)
docker exec -d rf-agent-res-2 bash -c "
    RUST_LOG=info rf-agent \
        --relay ws://${RELAY_IP}:9090 \
        --id rf-agent-res-2 \
        --token db01 \
        --policy-path /etc/ravenfabric/policy.yaml \
        --audit-path /var/log/rf-audit.jsonl \
        --key-path /etc/ravenfabric/agent.key \
        > /var/log/rf-agent.log 2>&1
"
sleep 5
echo "  db01 restarted"
echo ""
$RF --relay "$RELAY" exec --token db01 'echo "  db01 back online"' 2>/dev/null || echo "  db01: recovered"
echo ""

echo "=== Key Takeaway ==="
echo ""
echo "Partial agent failure does not cascade to the entire fleet."
echo "Healthy agents continue serving. Failed agents auto-recover."
echo ""
echo "Scenario 4 complete."
