#!/usr/bin/env bash
# Recording: Standard Remote Execution
# Run inside: asciinema rec --command "bash recordings/01-standard-exec.sh"
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Standard Remote Execution"

comment "Execute commands on remote agents via encrypted Noise XX channel"
sleep 1

comment "Simple command on agent 1"
run_cmd "rf --relay $RELAY exec --token agent1 'hostname && uname -a'"
sleep 5

comment "Read system info from agent 2"
run_cmd "rf --relay $RELAY exec --token agent2 'cat /etc/os-release | head -4'"
sleep 5

comment "File operations — write and read"
run_cmd "rf --relay $RELAY exec --token agent1 'echo \"Hello from RavenFabric\" > /tmp/test.txt && cat /tmp/test.txt'"
sleep 5

comment "Process listing"
run_cmd "rf --relay $RELAY exec --token agent2 'ps aux --no-header | head -5'"
sleep 5

comment "Exit code propagation"
run_cmd "rf --relay $RELAY exec --token agent1 'exit 42' || echo 'Exit code 42 propagated correctly'"

section "Every command: encrypted, policy-checked, audited"
sleep 2
