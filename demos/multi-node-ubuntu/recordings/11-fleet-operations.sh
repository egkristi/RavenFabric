#!/usr/bin/env bash
# Recording: Multi-Agent Fleet Operations
# Run inside: asciinema rec --command "bash recordings/11-fleet-operations.sh"
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Fleet Operations"

comment "Manage multiple agents from a single CLI"
sleep 1

comment "Collect hostnames from all agents"
for token in agent1 agent2; do
    run_cmd "rf --relay $RELAY exec --token $token 'hostname'" 1
    sleep 5
done
sleep 1

comment "Deploy a config file to all agents"
for token in agent1 agent2; do
    run_cmd "rf --relay $RELAY exec --token $token 'echo version: 2.0 > /opt/app/config.yaml && echo Deployed to \$(hostname)'" 1
    sleep 5
done
sleep 1

comment "Verify deployment across fleet"
for token in agent1 agent2; do
    run_cmd "rf --relay $RELAY exec --token $token 'cat /opt/app/config.yaml'" 1
    sleep 5
done

section "One CLI, many agents — fleet management made simple"
sleep 2
