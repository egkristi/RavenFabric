#!/usr/bin/env bash
# Scenario 3: Policy Denial
#
# Demonstrates the deny-by-default policy engine blocking
# commands that are not explicitly allowed. The AI agent
# cannot bypass policy — it's enforced at the execution layer.

set -euo pipefail
cd "$(dirname "$0")/.."

CONTAINER="rf-mcp-demo"

echo "=== Scenario 3: Policy Denial ==="
echo ""
echo "The AI agent attempts commands outside the allow list."
echo "The policy engine denies them before execution begins."
echo ""

sleep 2

echo "--- Attempt: Install a package ---"
echo ""
echo '$ rf_query_policy(command="apt-get install nginx")'
echo "  Result: DENIED"
echo "  Reason: No allow pattern matches 'apt-get install nginx'"
echo "  The command never executes — policy blocks it at evaluation time."
echo ""

sleep 2

echo "--- Attempt: Delete files ---"
echo ""
echo '$ rf_query_policy(command="rm -rf /tmp/*")'
echo "  Result: DENIED"
echo "  Reason: Matches explicit deny pattern '.*rm .*-rf.*'"
echo "  Even if the AI hallucinated a delete command, the policy catches it."
echo ""

sleep 2

echo "--- Attempt: System shutdown ---"
echo ""
echo '$ rf_query_policy(command="shutdown -h now")'
echo "  Result: DENIED"
echo "  Reason: Matches explicit deny pattern '.*shutdown.*'"
echo ""

sleep 2

echo "--- Attempt: Read sensitive file ---"
echo ""
echo '$ rf_file_read(path="/etc/shadow")'
echo "  Result: DENIED"
echo "  Reason: Path /etc/shadow is in the filesystem deny list"
echo ""

sleep 2

echo "--- Verify: Allowed commands still work ---"
echo ""
echo '$ rf_exec(command="hostname")'
docker exec "$CONTAINER" hostname
echo "  Result: ALLOWED (matches ^hostname$ pattern)"
echo ""

sleep 2

echo "=== Defense-in-Depth Layers ==="
echo ""
echo "┌─────────────────────────────────────────────────────┐"
echo "│  Layer 1: Policy Engine    → Deny-by-default rules  │"
echo "│  Layer 2: Human Approval   → Operator gate          │"
echo "│  Layer 3: Output Limits    → Bounded response size  │"
echo "│  Layer 4: Timeout          → Execution time cap     │"
echo "│  Layer 5: Audit Trail      → Every action logged    │"
echo "└─────────────────────────────────────────────────────┘"
echo ""
echo "Scenario 3 complete."
