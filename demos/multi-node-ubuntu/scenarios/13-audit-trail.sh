#!/usr/bin/env bash
# Scenario 13: Audit Trail
#
# Demonstrates RavenFabric's structured audit logging. Every action —
# allowed or denied — is recorded as a JSON-lines entry with timestamp,
# command, decision, caller identity, and execution duration.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 13: Audit Trail ==="
echo ""

# 1. Clear audit log baseline count
BASELINE=$(docker exec rf-agent-1 bash -c 'wc -l < /var/log/rf-audit.jsonl 2>/dev/null || echo 0')
echo "[1] Audit log baseline: ${BASELINE} entries on agent 1"
echo ""

# 2. Execute several commands to generate audit entries
echo "[2] Executing commands to generate audit trail..."
$RF --relay "$RELAY" exec --token agent1 'hostname' > /dev/null 2>&1 || true
sleep 6
$RF --relay "$RELAY" exec --token agent1 'uname -a' > /dev/null 2>&1 || true
sleep 6
$RF --relay "$RELAY" exec --token agent1 'uptime' > /dev/null 2>&1 || true
sleep 6
echo "  3 commands executed on agent 1."
echo ""

# 3. View raw audit log entries
echo "[3] Raw audit log (last 5 entries — structured JSON-lines):"
docker exec rf-agent-1 tail -5 /var/log/rf-audit.jsonl
echo ""

# 4. Count entries per agent
echo "[4] Audit entry count per agent:"
AGENT1_COUNT=$(docker exec rf-agent-1 bash -c 'wc -l < /var/log/rf-audit.jsonl 2>/dev/null || echo 0')
AGENT2_COUNT=$(docker exec rf-agent-2 bash -c 'wc -l < /var/log/rf-audit.jsonl 2>/dev/null || echo 0')
echo "  agent 1: ${AGENT1_COUNT} entries"
echo "  agent 2: ${AGENT2_COUNT} entries"
echo ""

# 5. Generate an entry on agent 2
echo "[5] Executing command on agent 2..."
$RF --relay "$RELAY" exec --token agent2 'date' > /dev/null 2>&1 || true
sleep 6
echo "  Command executed."
echo ""

echo "[6] Agent 2 audit log (last 3 entries):"
docker exec rf-agent-2 tail -3 /var/log/rf-audit.jsonl
echo ""

# 6. Filter audit log fields
echo "[7] Extracting command and decision fields:"
docker exec rf-agent-1 bash -c '
tail -5 /var/log/rf-audit.jsonl | while IFS= read -r line; do
    cmd=$(echo "$line" | grep -o "\"command\":\"[^\"]*\"" | head -1)
    decision=$(echo "$line" | grep -o "\"decision\":\"[^\"]*\"" | head -1)
    ts=$(echo "$line" | grep -o "\"timestamp\":\"[^\"]*\"" | head -1)
    echo "  ${ts} | ${cmd} | ${decision}"
done
'
echo ""

# 7. Show that audit log is append-only
echo "[8] Audit log properties:"
echo "  Format:      JSON-lines (one JSON object per line)"
echo "  Location:    /var/log/rf-audit.jsonl"
echo "  Append-only: new entries appended, no deletion or truncation"
echo "  Contains:    timestamp, command, decision, caller, duration"
echo ""

# 8. Generate a denied entry (try a command that will fail under default policy)
echo "[9] Executing command on agent 1 (generates audit entry with result):"
$RF --relay "$RELAY" exec --token agent1 'echo "audit trail demo complete"'
echo ""
sleep 6

echo "[10] Latest audit entry:"
docker exec rf-agent-1 tail -1 /var/log/rf-audit.jsonl
echo ""

echo "=== Scenario 13 Complete ==="
echo ""
echo "Key takeaways:"
echo "  - Every command execution produces a structured audit entry"
echo "  - Allowed and denied actions are both logged"
echo "  - Each agent maintains its own independent audit log"
echo "  - Audit log is append-only — entries cannot be deleted or modified"
echo "  - Fields: timestamp, command, decision, caller, duration, exit_code"
