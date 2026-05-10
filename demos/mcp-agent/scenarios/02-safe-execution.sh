#!/usr/bin/env bash
# Scenario 2: Safe Execution
#
# Demonstrates AI agent executing allowed read-only commands.
# All commands match the policy's allow patterns and execute
# without requiring human approval.

set -euo pipefail
cd "$(dirname "$0")/.."

CONTAINER="rf-mcp-demo"

echo "=== Scenario 2: Safe Execution ==="
echo ""
echo "The AI executes commands that match the policy's allow list."
echo "These are read-only, informational commands — no approval needed."
echo ""

sleep 2

echo "--- System Identification ---"
echo ""
echo '$ rf_exec(command="hostname")'
docker exec "$CONTAINER" hostname
echo ""
echo '$ rf_exec(command="uname -a")'
docker exec "$CONTAINER" uname -a
echo ""

sleep 2

echo "--- User Context ---"
echo ""
echo '$ rf_exec(command="whoami")'
docker exec "$CONTAINER" whoami
echo ""
echo '$ rf_exec(command="id")'
docker exec "$CONTAINER" id
echo ""

sleep 2

echo "--- System Resources ---"
echo ""
echo '$ rf_exec(command="df -h")'
docker exec "$CONTAINER" df -h 2>/dev/null || true
echo ""
echo '$ rf_exec(command="uptime")'
docker exec "$CONTAINER" uptime
echo ""

sleep 2

echo "--- File Reading ---"
echo ""
echo '$ rf_exec(command="cat /etc/os-release")'
docker exec "$CONTAINER" cat /etc/os-release | head -4
echo ""

sleep 2

echo "=== Key Takeaway ==="
echo ""
echo "Read-only commands execute immediately within policy bounds."
echo "Every execution is logged to the audit trail (see scenario 5)."
echo ""
echo "Scenario 2 complete."
