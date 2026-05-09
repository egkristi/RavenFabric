#!/usr/bin/env bash
# Recording: Audit Log Inspection
# Run inside: asciinema rec --command "bash recordings/10-audit-inspection.sh"
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Audit Logging"

comment "Every action is logged — allowed, denied, with timing and caller"
sleep 1

comment "Execute a command (generates an audit entry)"
run_cmd "rf --relay $RELAY exec --token agent1 'echo audit-test'" 2
sleep 5

comment "View the audit log (structured JSON-lines)"
run_cmd "docker exec rf-agent-1 tail -3 /var/log/rf-audit.jsonl"
sleep 1

comment "Count total audit entries"
run_cmd "docker exec rf-agent-1 wc -l < /var/log/rf-audit.jsonl"

section "Complete audit trail — every action, every decision, every agent"
sleep 2
