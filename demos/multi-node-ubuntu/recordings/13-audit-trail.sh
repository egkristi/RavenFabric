#!/usr/bin/env bash
# Recording: Audit Trail
# Run inside: asciinema rec --command "bash recordings/13-audit-trail.sh"
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Audit Trail"

comment "Every action is logged — allowed or denied, with timestamp and caller"
sleep 1

comment "Execute a command (generates an audit entry)"
run_cmd "rf --relay $RELAY exec --token agent1 'hostname'"
sleep 5

comment "Execute another command on agent 2"
run_cmd "rf --relay $RELAY exec --token agent2 'uname -a'"
sleep 5

comment "View the structured audit log (JSON-lines format)"
run_cmd "docker exec rf-agent-1 tail -3 /var/log/rf-audit.jsonl"
sleep 2

comment "Count total audit entries per agent"
run_cmd "docker exec rf-agent-1 wc -l < /var/log/rf-audit.jsonl"
sleep 1
run_cmd "docker exec rf-agent-2 wc -l < /var/log/rf-audit.jsonl"
sleep 1

comment "Agent 2 has its own independent audit log"
run_cmd "docker exec rf-agent-2 tail -2 /var/log/rf-audit.jsonl"
sleep 2

section "Append-only audit trail — every action, every agent, every decision"
sleep 2
