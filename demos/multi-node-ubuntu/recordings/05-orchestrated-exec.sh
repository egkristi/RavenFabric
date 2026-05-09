#!/usr/bin/env bash
# Recording: Orchestrated Multi-Agent Execution
# Run inside: asciinema rec --command "bash recordings/05-orchestrated-exec.sh"
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Multi-Agent Orchestration"

comment "Execute commands across multiple agents with playbooks"
comment "Supports parallel, sequential, rolling, and canary strategies"
sleep 1

comment "Show a playbook"
run_cmd "cat scenarios/playbooks/parallel-update.yaml"
sleep 1

comment "Run parallel update across both agents"
run_cmd "rf --relay $RELAY playbook --token agent1 scenarios/playbooks/parallel-update.yaml"
sleep 5

comment "Show canary deploy playbook (with rollback)"
run_cmd "cat scenarios/playbooks/canary-deploy.yaml"
sleep 1

comment "Run canary deploy — tests on one agent before rolling to all"
run_cmd "rf --relay $RELAY playbook --token agent1 scenarios/playbooks/canary-deploy.yaml"

section "Orchestrate fleets — parallel, sequential, canary, with rollback"
sleep 2
