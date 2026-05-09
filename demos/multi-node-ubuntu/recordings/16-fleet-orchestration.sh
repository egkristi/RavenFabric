#!/usr/bin/env bash
# Recording: Fleet Orchestration
# Run inside: asciinema rec --command "bash recordings/16-fleet-orchestration.sh"
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Fleet Orchestration"

comment "Execute commands across multiple agents with playbook strategies"
comment "Parallel, sequential, rolling, and canary — with automatic rollback"
sleep 1

comment "Collect inventory from all agents"
for token in agent1 agent2; do
    run_cmd "rf --relay $RELAY exec --token $token 'hostname'" 1
    sleep 5
done
sleep 1

comment "Show the parallel playbook"
run_cmd "cat scenarios/playbooks/parallel-update.yaml"
sleep 2

comment "Run parallel update across both agents"
run_cmd "rf --relay $RELAY playbook --token agent1 scenarios/playbooks/parallel-update.yaml"
sleep 5

comment "Show the canary deploy playbook (with rollback)"
run_cmd "cat scenarios/playbooks/canary-deploy.yaml"
sleep 2

comment "Run canary deploy — test on 1 agent first, then roll out"
run_cmd "rf --relay $RELAY playbook --token agent1 scenarios/playbooks/canary-deploy.yaml"
sleep 5

comment "Verify deployment across the fleet"
for token in agent1 agent2; do
    run_cmd "rf --relay $RELAY exec --token $token 'cat /opt/app/version.txt'" 1
    sleep 5
done

section "Playbooks: define once, deploy to any fleet size"
sleep 2
