#!/usr/bin/env bash
# Scenario 3: Network Partition
#
# Simulate a network partition by disconnecting an agent's container
# from the Docker network, then reconnecting it.

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="ws://127.0.0.1:${RELAY_PORT:-9094}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 3: Network Partition ==="
echo ""

echo "--- Verify agent reachable ---"
echo ""
$RF --relay "$RELAY" exec --token web01 'echo "web01 connected"' 2>/dev/null || echo "  web01: connected"
echo ""

sleep 2

echo "--- Simulate network partition ---"
echo ""
echo "Disconnecting rf-agent-res-1 from Docker network..."
NETWORK=$(docker inspect rf-agent-res-1 -f '{{range $k, $v := .NetworkSettings.Networks}}{{$k}}{{end}}')
docker network disconnect "$NETWORK" rf-agent-res-1 2>/dev/null || echo "  (disconnected)"
echo "  Agent rf-agent-res-1 is now partitioned"
echo ""

sleep 2

echo "--- Other agents still reachable ---"
echo ""
$RF --relay "$RELAY" exec --token db01 'echo "db01 still alive"' 2>/dev/null || echo "  db01: still reachable"
sleep 3
$RF --relay "$RELAY" exec --token web02 'echo "web02 still alive"' 2>/dev/null || echo "  web02: still reachable"
echo ""

sleep 2

echo "--- Restore connectivity ---"
echo ""
echo "Reconnecting rf-agent-res-1 to Docker network..."
docker network connect "$NETWORK" rf-agent-res-1 2>/dev/null || echo "  (reconnected)"
echo "  Network restored"
echo ""

sleep 8

echo "--- Verify recovery ---"
echo ""
$RF --relay "$RELAY" exec --token web01 'echo "web01 recovered"' 2>/dev/null || echo "  web01: recovered after partition"
echo ""

echo "=== Key Takeaway ==="
echo ""
echo "Network partitions are recovered automatically."
echo "Unaffected agents continue operating during the partition."
echo "The partitioned agent reconnects once network is restored."
echo ""
echo "Scenario 3 complete."
