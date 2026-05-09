#!/usr/bin/env bash
# Recording: Background Execution
# Run inside: asciinema rec --command "bash recordings/03-background-exec.sh"
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Background Execution"

comment "Start long-running tasks without blocking — get a job ID back immediately"
sleep 1

comment "Fire and forget — start a background job"
run_cmd "rf --relay $RELAY exec --background --token agent1 'sleep 3 && echo done > /tmp/bg-result.txt'" 2
sleep 5

comment "Check the result after completion"
run_cmd "rf --relay $RELAY exec --token agent1 'cat /tmp/bg-result.txt 2>/dev/null || echo Still running...'"

section "Non-blocking execution for long-running operations"
sleep 2
