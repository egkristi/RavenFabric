#!/usr/bin/env bash
# Recording: Resilience Demo
# Run: asciinema rec --command "bash demos/recordings/record-resilience.sh" demos/recordings/resilience.cast --cols 100 --rows 28 --overwrite
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$ROOT/target/release:$PATH"
export RUST_LOG=warn
RELAY="ws://127.0.0.1:9094"

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
section "RavenFabric — Resilience"

comment "4 containers: 1 relay + 3 agents (web-01, db-01, web-02)"
comment "Agents self-heal with exponential backoff + jitter"
sleep 1

comment "Kill an agent — it auto-restarts and reconnects"
run_cmd "cd demos/resilience && ./scenarios/01-agent-reconnect.sh" 8

comment "Stop the relay — agents queue, relay restarts — agents recover"
run_cmd "./scenarios/02-relay-restart.sh" 8

comment "Simulate network partition between agent and relay"
run_cmd "./scenarios/03-network-partition.sh" 8

comment "One agent down — others continue operating normally"
run_cmd "./scenarios/04-graceful-degradation.sh" 6

comment "Watch exponential backoff: 1s → 2s → 4s → 8s → 16s → 30s (cap)"
run_cmd "./scenarios/05-backoff-behavior.sh" 8

section "Self-healing · Backoff + jitter · Partition-tolerant"
sleep 2
