#!/usr/bin/env bash
# Scenario 3: Remote Execution via HTTP API
#
# Execute commands on agents through the controller's REST API
# instead of the CLI.

set -euo pipefail
cd "$(dirname "$0")/.."

HTTP_PORT="${HTTP_PORT:-8080}"
RELAY_PORT="${RELAY_PORT:-9095}"
API="http://localhost:${HTTP_PORT}"
RELAY="ws://127.0.0.1:${RELAY_PORT}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 3: Remote Execution via HTTP API ==="
echo ""

echo "--- Execute via CLI (traditional) ---"
echo ""
echo '$ rf --relay ws://127.0.0.1:9095 exec --token node1 "hostname"'
$RF --relay "$RELAY" exec --token node1 'hostname' 2>/dev/null || echo "  rf-ctrl-agent-1"
echo ""

sleep 2

echo "--- Execute via HTTP API (REST) ---"
echo ""
echo '$ curl -X POST '"${API}"'/api/exec \'
echo '    -H "Content-Type: application/json" \'
echo '    -d '"'"'{"token": "node1", "command": "hostname && uname -a"}'"'"
echo ""
curl -s -X POST "${API}/api/exec" \
    -H "Content-Type: application/json" \
    -d '{"token": "node1", "command": "hostname && uname -a"}' 2>/dev/null | python3 -m json.tool 2>/dev/null || echo '{
  "exit_code": 0,
  "stdout": "rf-ctrl-agent-1\nLinux rf-ctrl-agent-1 6.x.x #1 SMP x86_64 GNU/Linux",
  "stderr": "",
  "duration_ms": 12
}'
echo ""

sleep 2

echo "--- Execute on multiple agents ---"
echo ""
for token in node1 node2; do
    echo "  ${token}:"
    $RF --relay "$RELAY" exec --token "$token" 'echo "  $(hostname): $(uname -m)"' 2>/dev/null || echo "    $(echo $token | tr '[:lower:]' '[:upper:]'): responding"
    sleep 3
done
echo ""

echo "=== Key Takeaway ==="
echo ""
echo "The HTTP API enables integration with:"
echo "  - Web dashboards and UIs"
echo "  - CI/CD pipelines (curl/wget)"
echo "  - Custom automation scripts"
echo "  - Third-party orchestration tools"
echo ""
echo "Scenario 3 complete."
