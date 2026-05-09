#!/usr/bin/env bash
# Scenario 17: Human Approval for AI-Controlled Agents
#
# Demonstrates the human-in-the-loop approval workflow for AI agents
# using the MCP (Model Context Protocol) server. AI agents must request
# approval for high-risk operations — a human operator approves or denies.
#
# Architecture:
#   AI Agent → MCP Server → rf_request_approval → [PENDING]
#                                                    ↓
#                                            Human operator
#                                            approve / deny
#                                                    ↓
#                          rf_check_approval → [APPROVED/DENIED]
#                                                    ↓
#                          rf_exec (only if approved) → agent
#
# Prerequisites: rf and rf-mcp-server binaries in $PATH

set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Scenario 17: Human Approval for AI-Controlled Agents ==="
echo ""

# --- Part 1: The Problem ---

echo "--- Part 1: Why Human Approval? ---"
echo ""
echo "  AI agents (Claude, GPT, Copilot) can execute commands via MCP."
echo "  But should an AI be allowed to:"
echo "    - DROP TABLE production_users?"
echo "    - rm -rf /opt/app?"
echo "    - systemctl restart nginx?"
echo ""
echo "  RavenFabric's answer: deny-by-default policy + human approval gate."
echo ""
sleep 6

# --- Part 2: MCP Server Setup ---

echo "--- Part 2: MCP Server with Approval ---"
echo ""
echo "  The MCP server exposes 8 tools to AI agents, including:"
echo ""
echo "    rf_exec              — execute command (policy-enforced)"
echo "    rf_query_policy      — check if command is allowed (dry-run)"
echo "    rf_request_approval  — request human approval"
echo "    rf_check_approval    — poll approval status"
echo ""
echo "  Start the MCP server:"
echo "    $ rf-mcp-server --policy policy.yaml --audit audit.jsonl"
echo ""
sleep 6

# --- Part 3: Approval Workflow ---

echo "--- Part 3: The Approval Workflow ---"
echo ""
echo "  Step 1: AI requests approval"
echo "    Tool: rf_request_approval"
echo "    Input:"
echo "      operation: \"database_migration\""
echo "      command: \"psql -c 'ALTER TABLE users ADD COLUMN role TEXT'\""
echo "      reason: \"Adding role column for RBAC feature\""
echo ""
echo "    Output:"
echo "      approval_id: \"a1b2c3d4-e5f6-7890-abcd-ef1234567890\""
echo "      status: \"PENDING\""
echo "      message: \"Approval requested. Waiting for operator.\""
echo ""
sleep 6

echo "  Step 2: Operator sees the request (stderr / webhook)"
echo "    [APPROVAL REQUEST]"
echo "    ID:        a1b2c3d4-e5f6-7890-abcd-ef1234567890"
echo "    Operation: database_migration"
echo "    Command:   psql -c 'ALTER TABLE users ADD COLUMN role TEXT'"
echo "    Reason:    Adding role column for RBAC feature"
echo "    Time:      2026-05-09T14:32:00Z"
echo ""
sleep 6

echo "  Step 3: Operator approves or denies"
echo "    approve(\"a1b2c3d4-e5f6-7890-abcd-ef1234567890\")  → APPROVED"
echo "    deny(\"a1b2c3d4-e5f6-7890-abcd-ef1234567890\")     → DENIED"
echo ""
sleep 6

echo "  Step 4: AI polls for result"
echo "    Tool: rf_check_approval"
echo "    Input:  approval_id: \"a1b2c3d4-...\""
echo "    Output: status: \"APPROVED\""
echo ""
echo "  Step 5: AI proceeds (only if APPROVED)"
echo "    Tool: rf_exec"
echo "    Input: command: \"psql -c 'ALTER TABLE users ADD COLUMN role TEXT'\""
echo "    → Command executes successfully"
echo ""
sleep 6

# --- Part 4: Denied Scenario ---

echo "--- Part 4: When the Operator Denies ---"
echo ""
echo "  AI requests:"
echo "    rf_request_approval("
echo "      operation: \"cleanup\","
echo "      command: \"rm -rf /opt/app/data\","
echo "      reason: \"Removing old data files\""
echo "    )"
echo ""
echo "  Operator reviews → DENIED"
echo "  AI polls → status: \"DENIED\""
echo "  AI does NOT execute the command."
echo ""
echo "  Audit log records the denial:"
echo "    {\"action\":\"approval_denied\",\"command\":\"rm -rf /opt/app/data\","
echo "     \"reason\":\"Removing old data files\",\"operator\":\"human\"}"
echo ""
sleep 6

# --- Part 5: Defense in Depth ---

echo "--- Part 5: Defense in Depth ---"
echo ""
echo "  ┌──────────────────┬───────────────────────────────────────────────┐"
echo "  │ Layer            │ Protection                                   │"
echo "  ├──────────────────┼───────────────────────────────────────────────┤"
echo "  │ Policy engine    │ Deny-by-default — blocks disallowed commands │"
echo "  │ Human approval   │ Gate for high-risk operations                │"
echo "  │ Rate limiting    │ 60 requests/min per session (configurable)   │"
echo "  │ Anomaly detection│ Behavioral baseline, alerts on deviation     │"
echo "  │ Audit trail      │ Every action logged (allowed, denied, gated) │"
echo "  │ Session isolation│ Per-session keys, no cross-session access    │"
echo "  └──────────────────┴───────────────────────────────────────────────┘"
echo ""
sleep 6

# --- Key Takeaways ---

echo "=== Key Takeaways ==="
echo ""
echo "  1. AI agents MUST request approval for high-risk operations"
echo "  2. Human operator approves/denies via API or webhook"
echo "  3. Denied requests are audited — full accountability"
echo "  4. Policy engine is the first gate; approval is the second"
echo "  5. Even if the AI is compromised, it cannot bypass the approval flow"
echo ""
echo "=== Scenario 17 Complete ==="
