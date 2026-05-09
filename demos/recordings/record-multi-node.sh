#!/usr/bin/env bash
# Recording: Multi-Node Ubuntu Demo
# Run: asciinema rec --command "bash demos/recordings/record-multi-node.sh" demos/recordings/multi-node.cast --cols 100 --rows 28 --overwrite
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$ROOT/target/release:$PATH"
export RUST_LOG=warn
RELAY="ws://127.0.0.1:9091"

type_cmd() {
    local cmd="$1"
    printf '\033[1;32m$\033[0m '
    for ((i=0; i<${#cmd}; i++)); do
        printf '%s' "${cmd:$i:1}"
        sleep 0.03
    done
    echo ""
    sleep 0.3
}

run_cmd() {
    type_cmd "$1"
    eval "$1"
    echo ""
    sleep "${2:-2}"
}

section() {
    echo ""
    printf '\033[1;36m  %s\033[0m\n' "$1"
    echo "  $(printf '%.0s─' {1..60})"
    echo ""
    sleep 1
}

comment() {
    printf '\033[0;90m  # %s\033[0m\n' "$1"
    sleep 0.5
}

clear
section "RavenFabric — Multi-Node Ubuntu"

comment "Two Ubuntu 24.04 agents connected via an encrypted relay"
comment "All traffic: Noise XX mutual auth, E2E encrypted, policy-checked"
sleep 1

comment "Execute a command on agent 1"
run_cmd "rf --relay $RELAY exec --token agent1 'hostname && uname -a'" 6

comment "Execute on agent 2"
run_cmd "rf --relay $RELAY exec --token agent2 'hostname && uname -a'" 6

comment "Check system resources on agent 1"
run_cmd "rf --relay $RELAY exec --token agent1 'uptime && free -h | head -2'" 6

comment "Read OS release info from agent 2"
run_cmd "rf --relay $RELAY exec --token agent2 'cat /etc/os-release | head -4'" 6

comment "Write and read a file on agent 1"
run_cmd "rf --relay $RELAY exec --token agent1 'echo \"Hello from RavenFabric\" > /tmp/test.txt && cat /tmp/test.txt'" 6

comment "Process list on agent 2"
run_cmd "rf --relay $RELAY exec --token agent2 'ps aux --no-header | head -5'" 6

section "Same binary · Same protocol · Any Ubuntu target"
sleep 2
