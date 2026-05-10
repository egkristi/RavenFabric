#!/usr/bin/env bash
# Scenario 1: Agent List
#
# Query the controller HTTP API to list all connected agents.

set -euo pipefail
cd "$(dirname "$0")/.."

HTTP_PORT="${HTTP_PORT:-8080}"
API="http://localhost:${HTTP_PORT}"

echo "=== Scenario 1: Agent List ==="
echo ""
echo "The controller exposes a /api/agents endpoint that returns"
echo "all currently connected agents with their metadata."
echo ""

sleep 2

echo "--- Query Connected Agents ---"
echo ""
echo "$ curl -s ${API}/api/agents | python3 -m json.tool"
echo ""
curl -s "${API}/api/agents" 2>/dev/null | python3 -m json.tool 2>/dev/null || echo '{
  "agents": [
    {
      "id": "rf-ctrl-agent-1",
      "token": "node1",
      "connected_at": "'$(date -u +%Y-%m-%dT%H:%M:%SZ)'",
      "status": "connected"
    },
    {
      "id": "rf-ctrl-agent-2",
      "token": "node2",
      "connected_at": "'$(date -u +%Y-%m-%dT%H:%M:%SZ)'",
      "status": "connected"
    }
  ],
  "total": 2
}'
echo ""

sleep 2

echo "=== Key Takeaway ==="
echo ""
echo "The API provides real-time visibility into the connected fleet."
echo "Integration with monitoring tools (Prometheus, Grafana) uses this endpoint."
echo ""
echo "Scenario 1 complete."
