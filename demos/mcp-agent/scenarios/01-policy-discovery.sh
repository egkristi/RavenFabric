#!/usr/bin/env bash
# Scenario 1: Policy Discovery
#
# Demonstrates how an AI agent discovers its capabilities through
# the rf_list_my_capabilities MCP tool. The agent learns what commands
# it can run, what paths it can access, and resource limits.

set -euo pipefail
cd "$(dirname "$0")/.."

CONTAINER="rf-mcp-demo"

echo "=== Scenario 1: Policy Discovery ==="
echo ""
echo "An AI agent's first action should be discovering what it's allowed to do."
echo "The rf_list_my_capabilities tool returns the full policy summary."
echo ""

sleep 2

echo "--- Allowed Commands ---"
echo ""
echo "The policy explicitly allows these command patterns:"
docker exec "$CONTAINER" bash -c "cat /etc/ravenfabric/policy.yaml | grep -A 20 'commands:'"
echo ""

sleep 2

echo "--- Allowed Filesystem Paths ---"
echo ""
echo "The AI can read/write only within these paths:"
docker exec "$CONTAINER" bash -c "cat /etc/ravenfabric/policy.yaml | grep -A 10 'filesystem:'"
echo ""

sleep 2

echo "--- Resource Limits ---"
echo ""
echo "Output and execution are bounded:"
docker exec "$CONTAINER" bash -c "cat /etc/ravenfabric/policy.yaml | grep -A 5 'resources:'"
echo ""

sleep 2

echo "=== Key Takeaway ==="
echo ""
echo "Before executing any command, a well-behaved AI agent queries"
echo "its capabilities. This enables self-limiting behavior — the AI"
echo "knows what it CAN do and stays within those bounds."
echo ""
echo "Scenario 1 complete."
