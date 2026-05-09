#!/usr/bin/env bash
# Scenario 10: Audit Log Inspection
#
# Demonstrates the structured audit logging system. Every command execution —
# allowed or denied — is recorded as a JSON-lines entry with timestamp,
# action, decision, caller, and duration.
#
# Prerequisites: ./setup.sh has been run, and some commands have been executed

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 10: Audit Log Inspection ==="
echo ""

# 1. Execute some commands to generate audit entries
echo "[1] Generating audit entries..."
$RF --relay "$RELAY" exec --token agent1 'echo "audit test 1"' > /dev/null 2>&1 || true
sleep 6
$RF --relay "$RELAY" exec --token agent1 'hostname' > /dev/null 2>&1 || true
sleep 6
echo "  Commands executed."
echo ""

# 2. View raw audit log
echo "[2] Raw audit log (last 5 entries):"
docker exec rf-agent-1 tail -5 /var/log/rf-audit.jsonl 2>/dev/null || echo "  No audit entries yet."
echo ""

# 3. Pretty-print audit log with jq
echo "[3] Pretty-printed audit entries:"
docker exec rf-agent-1 bash -c 'tail -3 /var/log/rf-audit.jsonl | python3 -m json.tool 2>/dev/null || tail -3 /var/log/rf-audit.jsonl'
echo ""

# 4. Filter for denied actions
echo "[4] Denied actions (if any):"
docker exec rf-agent-1 bash -c 'grep -i denied /var/log/rf-audit.jsonl 2>/dev/null | tail -3 || echo "  No denied actions found."'
echo ""

# 5. Count total audit entries
echo "[5] Total audit entries:"
docker exec rf-agent-1 bash -c 'wc -l < /var/log/rf-audit.jsonl 2>/dev/null || echo "  0"'
echo ""

# 6. Show audit log from agent 2
echo "[6] Agent 2 audit log (last 3 entries):"
docker exec rf-agent-2 bash -c 'tail -3 /var/log/rf-audit.jsonl 2>/dev/null || echo "  No entries yet."'
echo ""

echo "=== Scenario 10 Complete ==="
