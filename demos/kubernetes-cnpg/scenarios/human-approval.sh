#!/usr/bin/env bash
# Human Approval for AI-Controlled Agents — Kubernetes + CloudNativePG
#
# Demonstrates the human approval workflow for AI agents operating on
# a Kubernetes database cluster. AI can query data freely, but schema
# changes and destructive operations require human approval.
#
# Use case: AI DBA assistant that can SELECT freely but must get
# approval for ALTER TABLE, DROP, VACUUM, or backup operations.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RF="${RF_CLI:-rf}"
RELAY="${RF_RELAY:-ws://127.0.0.1:9093}"

echo "=== Human Approval — Kubernetes + CloudNativePG ==="
echo ""

# --- Part 1: AI DBA Use Case ---

echo "--- Part 1: AI Database Assistant ---"
echo ""
echo "  An AI agent acts as a DBA assistant for the CNPG cluster."
echo "  It connects via MCP server → RavenFabric → PostgreSQL."
echo ""
echo "  Allowed without approval (policy permits):"
echo "    SELECT * FROM users"
echo "    SELECT version()"
echo "    \\dt, \\d+ table_name"
echo ""
echo "  Requires human approval:"
echo "    ALTER TABLE users ADD COLUMN role TEXT"
echo "    DROP TABLE old_logs"
echo "    VACUUM FULL"
echo "    pg_dump app > backup.sql"
echo ""
sleep 6

# --- Part 2: Approval Workflow for K8s ---

echo "--- Part 2: Approval Workflow in Kubernetes ---"
echo ""
echo "  AI Agent → MCP Server → rf_request_approval"
echo "                              ↓"
echo "                    Operator dashboard / webhook"
echo "                    Slack notification / PagerDuty"
echo "                              ↓"
echo "                    approve() or deny()"
echo "                              ↓"
echo "                    rf_exec → agent pod → psql"
echo ""
echo "  The approval webhook can integrate with:"
echo "    - Slack (approve/deny buttons)"
echo "    - PagerDuty (escalation)"
echo "    - Custom dashboard"
echo "    - GitOps approval (PR merge = approve)"
echo ""
sleep 6

# --- Part 3: Example Flow ---

echo "--- Part 3: Example — Schema Migration ---"
echo ""
echo "  AI requests:"
echo "    rf_request_approval("
echo "      operation: \"schema_migration\","
echo "      command: \"psql -h pg-cluster-rw -c 'ALTER TABLE users ADD COLUMN role TEXT'\","
echo "      reason: \"Adding role column for new RBAC feature (ticket DB-1234)\""
echo "    )"
echo ""
echo "  Operator reviews:"
echo "    - Command: ALTER TABLE users ADD COLUMN role TEXT"
echo "    - Target: pg-cluster-rw (production primary)"
echo "    - Reason: RBAC feature, ticket DB-1234"
echo "    → APPROVED"
echo ""
echo "  AI executes via approved path:"
echo "    rf exec --token cnpg 'PGPASSWORD=\$POSTGRES_PASSWORD \\"
echo "      psql -h pg-cluster-rw -U postgres -d app \\"
echo "      -c \"ALTER TABLE users ADD COLUMN role TEXT\"'"
echo ""
echo "  Audit log records:"
echo "    {\"action\":\"approval_approved\",\"command\":\"ALTER TABLE...\","
echo "     \"operator\":\"human\",\"ticket\":\"DB-1234\"}"
echo ""
sleep 6

# --- Part 4: vs kubectl exec ---

echo "--- Part 4: vs Unrestricted kubectl exec ---"
echo ""
echo "  ┌──────────────────────┬──────────────────────┬───────────────────────┐"
echo "  │                      │ kubectl exec         │ rf + MCP + approval   │"
echo "  ├──────────────────────┼──────────────────────┼───────────────────────┤"
echo "  │ AI can run anything? │ Yes (if RBAC allows) │ No (policy + approval)│"
echo "  │ Human gate           │ None                 │ rf_request_approval   │"
echo "  │ Audit per-command    │ K8s audit (coarse)   │ JSON per-command      │"
echo "  │ Rate limiting        │ None                 │ 60/min (configurable) │"
echo "  │ Anomaly detection    │ None                 │ Behavioral baseline   │"
echo "  │ Works through NAT    │ Needs kubeconfig     │ Yes (relay-based)     │"
echo "  └──────────────────────┴──────────────────────┴───────────────────────┘"
echo ""
sleep 6

# --- Key Takeaways ---

echo "=== Key Takeaways ==="
echo ""
echo "  1. AI DBA can query freely, but schema changes need approval"
echo "  2. Approval webhook integrates with Slack, PagerDuty, GitOps"
echo "  3. Every approval (granted or denied) is audited"
echo "  4. Defense in depth: policy → approval → rate limit → anomaly → audit"
echo ""
echo "=== Human Approval Scenario Complete ==="
