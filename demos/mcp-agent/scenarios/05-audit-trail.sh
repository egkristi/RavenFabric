#!/usr/bin/env bash
# Scenario 5: Audit Trail
#
# Demonstrates that every AI action produces a structured audit entry.
# The audit log is append-only and provides a complete record of
# all AI agent activity for compliance and forensics.

set -euo pipefail
cd "$(dirname "$0")/.."

CONTAINER="rf-mcp-demo"

echo "=== Scenario 5: Audit Trail ==="
echo ""
echo "Every MCP tool invocation produces a JSON-lines audit entry."
echo "The audit log is append-only — entries cannot be deleted or modified."
echo ""

sleep 2

echo "--- Generate Some Activity ---"
echo ""
echo "Running a few commands to produce audit entries..."
docker exec "$CONTAINER" hostname > /dev/null
docker exec "$CONTAINER" uname -a > /dev/null
docker exec "$CONTAINER" cat /etc/os-release > /dev/null 2>&1 || true
echo "  3 commands executed"
echo ""

sleep 2

echo "--- Audit Entry Format ---"
echo ""
echo "Each audit entry is a JSON object with:"
echo ""
echo '  {'
echo '    "timestamp": "'$(date -u +%Y-%m-%dT%H:%M:%S.000Z)'",'
echo '    "session_id": "ses_a1b2c3d4",'
echo '    "action": "exec",'
echo '    "command": "hostname",'
echo '    "policy_result": "ALLOWED",'
echo '    "exit_code": 0,'
echo '    "output_bytes": 14,'
echo '    "duration_ms": 3,'
echo '    "approval_id": null,'
echo '    "client_identity": "ai-agent-claude"'
echo '  }'
echo ""

sleep 2

echo "--- Audit Query Examples ---"
echo ""
echo '$ rf_audit_query(limit=5)'
echo "  Returns the last 5 audit entries in reverse chronological order"
echo ""
echo '$ rf_audit_query(action_filter="denied")'
echo "  Returns only DENIED actions — useful for security review"
echo ""
echo '$ rf_audit_query(action_filter="approval")'
echo "  Returns approval requests and decisions"
echo ""

sleep 2

echo "--- What Gets Logged ---"
echo ""
echo "  ┌────────────────────┬───────────────────────────────────┐"
echo "  │ Action             │ Logged Fields                     │"
echo "  ├────────────────────┼───────────────────────────────────┤"
echo "  │ rf_exec            │ command, result, exit_code, time  │"
echo "  │ rf_query_policy    │ command, policy_result            │"
echo "  │ rf_file_read       │ path, bytes_read, policy_result   │"
echo "  │ rf_file_write      │ path, bytes_written, approval_id  │"
echo "  │ rf_request_approval│ operation, reason, approval_id    │"
echo "  │ rf_check_approval  │ approval_id, status               │"
echo "  │ rf_list_capabilities│ (discovery logged)               │"
echo "  │ rf_audit_query     │ query parameters, results_count   │"
echo "  └────────────────────┴───────────────────────────────────┘"
echo ""

sleep 2

echo "=== Key Takeaway ==="
echo ""
echo "The audit trail provides:"
echo "  - Complete record of all AI agent activity"
echo "  - Immutable append-only log (no delete/truncate)"
echo "  - Structured JSON-lines format for easy parsing"
echo "  - Queryable through the MCP audit_query tool"
echo "  - Compliance evidence for SOC 2, HIPAA, GDPR"
echo ""
echo "Scenario 5 complete."
