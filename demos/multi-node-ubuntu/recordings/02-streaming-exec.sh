#!/usr/bin/env bash
# Recording: Streaming Execution
# Run inside: asciinema rec --command "bash recordings/02-streaming-exec.sh"
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Streaming Execution"

comment "Stream output in real-time as the command produces it"
sleep 1

comment "Streaming countdown — watch output arrive incrementally"
run_cmd "rf --relay $RELAY exec --stream --token agent1 'for i in 5 4 3 2 1; do echo \"Countdown: \$i\"; sleep 1; done; echo Done!'"
sleep 5

comment "Streaming log output"
run_cmd "rf --relay $RELAY exec --stream --token agent2 'for i in 1 2 3 4 5; do echo \"[\$(date +%H:%M:%S)] Event \$i: status=ok\"; sleep 0.5; done'"

section "Real-time output — no buffering, no waiting"
sleep 2
