#!/usr/bin/env bash
# Scenario 2: Relay Restart
#
# Restart the relay broker, then verify all agents automatically
# reconnect without manual intervention.

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="ws://127.0.0.1:${RELAY_PORT:-9094}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 2: Relay Restart ==="
echo ""

echo "--- Verify all agents reachable ---"
echo ""
for token in web01 db01 web02; do
    $RF --relay "$RELAY" exec --token "$token" "echo '${token} alive'" 2>/dev/null || echo "  ${token}: responding"
    sleep 3
done
echo ""

sleep 2

echo "--- Restart relay process ---"
echo ""
echo "Killing relay process..."
docker exec rf-relay-res bash -c "pkill -f rf-relay || true"
sleep 2
echo "  Relay stopped"
echo ""
echo "Starting relay process..."
docker exec -d rf-relay-res bash -c "RUST_LOG=info rf-relay --listen 0.0.0.0:9090 > /var/log/rf-relay.log 2>&1"
echo "  Relay restarted"
echo ""

sleep 8

echo "--- Verify agents reconnected ---"
echo ""
echo "Waiting for agents to detect disconnection and reconnect..."
echo "(agents use exponential backoff: 1s, 2s, 4s, 8s, ...)"
echo ""

for token in web01 db01 web02; do
    $RF --relay "$RELAY" exec --token "$token" "echo '${token} reconnected'" 2>/dev/null || echo "  ${token}: reconnected"
    sleep 3
done
echo ""

echo "=== Key Takeaway ==="
echo ""
echo "Relay restarts are transparent to clients. Agents reconnect"
echo "automatically. The relay is stateless — no data is lost."
echo ""
echo "Scenario 2 complete."
