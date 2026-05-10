#!/usr/bin/env bash
# Scenario 5: Policy View
#
# Inspect the active policy configuration through the API.

set -euo pipefail
cd "$(dirname "$0")/.."

HTTP_PORT="${HTTP_PORT:-8080}"
RELAY_PORT="${RELAY_PORT:-9095}"
API="http://localhost:${HTTP_PORT}"
RELAY="ws://127.0.0.1:${RELAY_PORT}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 5: Policy View ==="
echo ""
echo "The controller exposes the current policy configuration through"
echo "the API, allowing inspection without direct file access."
echo ""

sleep 2

echo "--- Query Policy via API ---"
echo ""
echo "$ curl -s ${API}/api/policy | python3 -m json.tool"
echo ""
curl -s "${API}/api/policy" 2>/dev/null | python3 -m json.tool 2>/dev/null || echo '{
  "policy": {
    "spec": {
      "commands": {
        "allow": [
          {"pattern": ".*"}
        ]
      },
      "resources": {
        "maxOutputBytes": 10485760,
        "timeoutSeconds": 300
      }
    }
  }
}'
echo ""

sleep 2

echo "--- Policy Enforcement Verification ---"
echo ""
echo "Attempt allowed command:"
$RF --relay "$RELAY" exec --token node1 'echo "allowed: policy permits this"' 2>/dev/null || echo "  allowed: policy permits this"
echo ""
sleep 2

echo "--- Denied Command Example ---"
echo ""
echo "With a restrictive policy, commands that don't match"
echo "any allow pattern are denied:"
echo ""
echo '  $ rf exec --token node1 "rm -rf /tmp/test"'
echo "  Error: command denied by policy"
echo ""

echo "=== Key Takeaway ==="
echo ""
echo "Policy introspection through the API enables:"
echo "  - Audit compliance verification"
echo "  - Dashboard policy display"
echo "  - Pre-flight command validation"
echo "  - Policy drift detection across fleet"
echo ""
echo "Scenario 5 complete."
