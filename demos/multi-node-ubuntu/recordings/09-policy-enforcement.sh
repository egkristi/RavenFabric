#!/usr/bin/env bash
# Recording: Policy Enforcement
# Run inside: asciinema rec --command "bash recordings/09-policy-enforcement.sh"
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Policy Enforcement"

comment "Deny-by-default policy engine — every command is checked before execution"
sleep 1

comment "Show available policy templates"
run_cmd "rf policy list"
sleep 1

comment "Show the 'safe-dev-mode' policy template"
run_cmd "rf policy show safe-dev-mode | head -20"
sleep 1

comment "Try an allowed command (hostname)"
run_cmd "rf --relay $RELAY exec --token agent1 'hostname'"
sleep 5

comment "Try a dangerous command (rm -rf /)"
run_cmd "rf --relay $RELAY exec --token agent1 'rm -rf /' || echo 'BLOCKED by policy'"

section "Every command checked — deny by default, allow by policy"
sleep 2
