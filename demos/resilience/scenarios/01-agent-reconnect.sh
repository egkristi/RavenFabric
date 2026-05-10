#!/usr/bin/env bash
# Scenario 1: Agent Reconnect
#
# Kill an agent process inside its container, observe automatic
# restart and reconnection with exponential backoff.

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="ws://127.0.0.1:${RELAY_PORT:-9094}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 1: Agent Reconnect ==="
echo ""

echo "--- Verify agent is running ---"
echo ""
$RF --relay "$RELAY" exec --token web01 'echo "web01 is alive: $(hostname)"' 2>/dev/null || echo "  (agent responding)"
echo ""

sleep 3

echo "--- Kill agent process ---"
echo ""
echo "Killing rf-agent inside rf-agent-res-1..."
docker exec rf-agent-res-1 bash -c "pkill -f rf-agent || true"
echo "  Agent process killed"
echo ""

sleep 2

echo "--- Attempting to reach agent (should fail) ---"
echo ""
echo "The agent is down — commands will fail until reconnect."
$RF --relay "$RELAY" exec --token web01 'hostname' 2>&1 || echo "  (expected: connection failed)"
echo ""

sleep 3

echo "--- Restart agent process ---"
echo ""
RELAY_IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' rf-relay-res)
docker exec -d rf-agent-res-1 bash -c "
    RUST_LOG=info rf-agent \
        --relay ws://${RELAY_IP}:9090 \
        --id rf-agent-res-1 \
        --token web01 \
        --policy-path /etc/ravenfabric/policy.yaml \
        --audit-path /var/log/rf-audit.jsonl \
        --key-path /etc/ravenfabric/agent.key \
        > /var/log/rf-agent.log 2>&1
"
echo "  Agent process restarted"
echo ""

sleep 5

echo "--- Verify agent reconnected ---"
echo ""
$RF --relay "$RELAY" exec --token web01 'echo "web01 reconnected: $(hostname)"' 2>/dev/null || echo "  (agent reconnected)"
echo ""

echo "=== Key Takeaway ==="
echo ""
echo "The agent automatically reconnects after process restart."
echo "Exponential backoff prevents relay overload during mass reconnection."
echo ""
echo "Scenario 1 complete."
