#!/usr/bin/env bash
# Recording: Controller / Web UI Demo
# Run: asciinema rec --command "bash demos/recordings/record-controller.sh" demos/recordings/controller.cast --cols 100 --rows 28 --overwrite
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$ROOT/target/release:$PATH"
export RUST_LOG=warn

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
section "RavenFabric — Controller / Web UI"

comment "HTTP API server with embedded fleet dashboard"
comment "REST endpoints: agents, health, exec, policy"
sleep 1

comment "List connected agents via REST API"
run_cmd "cd demos/controller && ./scenarios/01-agent-list.sh" 6

comment "Health check — controller and all agents"
run_cmd "./scenarios/02-health-check.sh" 6

comment "Remote execution via HTTP API"
run_cmd "./scenarios/03-remote-execution.sh" 6

comment "Fleet dashboard (embedded web UI)"
run_cmd "./scenarios/04-fleet-dashboard.sh" 6

comment "Policy inspection via API"
run_cmd "./scenarios/05-policy-view.sh" 6

section "REST API · Web dashboard · Fleet management"
sleep 2
