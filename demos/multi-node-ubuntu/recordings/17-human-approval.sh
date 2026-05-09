#!/usr/bin/env bash
# Recording: Human Approval for AI-Controlled Agents
# Run inside: asciinema rec --command "bash recordings/17-human-approval.sh"
source "$(dirname "$0")/helpers.sh"

clear
section "RavenFabric — Human Approval for AI Agents"

comment "AI agents execute commands via MCP — but high-risk operations"
comment "require human approval before execution"
sleep 1

comment "The MCP server exposes approval tools:"
comment "  rf_request_approval  — AI requests permission"
comment "  rf_check_approval    — AI polls for decision"
sleep 2

comment "Step 1: AI requests approval for a database migration"
type_cmd 'rf_request_approval(command="psql -c ALTER TABLE...", reason="Adding role column")'
sleep 3

comment "Output: approval_id = a1b2c3d4-..., status = PENDING"
sleep 2

comment "Step 2: Human operator reviews and approves"
type_cmd 'approve("a1b2c3d4-...")'
sleep 2

comment "Step 3: AI polls and sees APPROVED — proceeds with execution"
type_cmd 'rf_check_approval(id="a1b2c3d4-...") → APPROVED'
sleep 2

comment "Step 4: AI executes the approved command"
type_cmd 'rf_exec(command="psql -c ALTER TABLE users ADD COLUMN role TEXT")'
sleep 3

comment "If denied: AI sees DENIED, does NOT execute, denial is audited"
sleep 2

comment "Defense in depth: policy → approval → rate limit → audit"
sleep 2

section "AI operates within bounds — humans stay in control"
sleep 2
