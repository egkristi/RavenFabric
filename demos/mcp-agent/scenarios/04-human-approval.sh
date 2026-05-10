#!/usr/bin/env bash
# Scenario 4: Human Approval Workflow
#
# Demonstrates the full human-in-the-loop approval flow:
# 1. AI requests approval for a sensitive operation
# 2. Operator reviews the request
# 3. Operator approves or denies
# 4. AI checks approval status
# 5. Only if approved, the operation executes

set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Scenario 4: Human Approval Workflow ==="
echo ""
echo "Some operations are too sensitive for autonomous execution."
echo "The approval workflow puts a human operator in the loop."
echo ""

sleep 2

echo "--- Step 1: AI Requests Approval ---"
echo ""
echo '$ rf_request_approval('
echo '    operation="write_config",'
echo '    command="Update /etc/ravenfabric/policy.yaml with new allow rules",'
echo '    reason="User asked me to allow nginx management commands"'
echo '  )'
echo ""
echo "  Response:"
echo '  {'
echo '    "approval_id": "apr_7f3a2b91",'
echo '    "status": "PENDING",'
echo '    "message": "Approval request submitted. Waiting for operator review."'
echo '  }'
echo ""

sleep 3

echo "--- Step 2: Operator Reviews ---"
echo ""
echo "The operator sees the approval request in their dashboard/CLI/Slack:"
echo ""
echo "  ┌──────────────────────────────────────────────────┐"
echo "  │  APPROVAL REQUEST: apr_7f3a2b91                  │"
echo "  │                                                  │"
echo "  │  Operation: write_config                         │"
echo "  │  Command:   Update policy with nginx rules       │"
echo "  │  Reason:    User asked for nginx management      │"
echo "  │  Requestor: ai-agent-claude                      │"
echo "  │  Time:      $(date -u +%Y-%m-%dT%H:%M:%SZ)                  │"
echo "  │                                                  │"
echo "  │  [APPROVE]  [DENY]  [DETAILS]                    │"
echo "  └──────────────────────────────────────────────────┘"
echo ""

sleep 3

echo "--- Step 3: Operator Approves ---"
echo ""
echo "The operator clicks [APPROVE] after verifying the request is legitimate."
echo ""

sleep 2

echo "--- Step 4: AI Checks Approval Status ---"
echo ""
echo '$ rf_check_approval(approval_id="apr_7f3a2b91")'
echo ""
echo "  Response:"
echo '  {'
echo '    "approval_id": "apr_7f3a2b91",'
echo '    "status": "APPROVED",'
echo '    "approved_by": "operator@example.com",'
echo '    "approved_at": "'$(date -u +%Y-%m-%dT%H:%M:%SZ)'"'
echo '  }'
echo ""

sleep 2

echo "--- Step 5: AI Executes (with approval_id) ---"
echo ""
echo '$ rf_exec('
echo '    command="cat /etc/os-release",'
echo '    approval_id="apr_7f3a2b91"'
echo '  )'
echo ""
echo "  Execution proceeds because approval_id is valid and APPROVED."
echo ""

sleep 2

echo "=== Denied Workflow ==="
echo ""
echo "If the operator had clicked [DENY]:"
echo ""
echo '$ rf_check_approval(approval_id="apr_7f3a2b91")'
echo '  { "status": "DENIED", "denied_by": "operator@example.com" }'
echo ""
echo "  The AI agent receives DENIED and must not proceed."
echo "  Any attempt to use a denied approval_id will be blocked."
echo ""

sleep 2

echo "=== Key Takeaway ==="
echo ""
echo "Human approval creates an unbypassable gate for sensitive operations."
echo "The AI cannot execute without a valid, approved approval_id."
echo "Every approval decision is logged in the audit trail."
echo ""
echo "Scenario 4 complete."
