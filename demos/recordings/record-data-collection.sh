#!/usr/bin/env bash
# Recording: Data Collection Demo
# Run: asciinema rec --command "bash demos/recordings/record-data-collection.sh" demos/recordings/data-collection.cast --cols 100 --rows 28 --overwrite
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$ROOT/target/release:$PATH"
export RUST_LOG=warn
RELAY="ws://127.0.0.1:9096"

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
section "RavenFabric — Secure Fleet Data Collection"

comment "3 role-based agents (collector, webserver, database)"
comment "Strict read-only policy — no writes, no mutations"
sleep 1

comment "System inventory from all agents"
run_cmd "cd demos/data-collection && ./scenarios/01-system-inventory.sh" 6

comment "Live resource monitoring (CPU, memory, disk)"
run_cmd "./scenarios/02-resource-monitoring.sh" 6

comment "Centralized log collection"
run_cmd "./scenarios/03-log-collection.sh" 6

comment "Configuration audit"
run_cmd "./scenarios/04-config-audit.sh" 6

comment "Security scan across the fleet"
run_cmd "./scenarios/06-security-scan.sh" 6

comment "Fleet-wide snapshot"
run_cmd "./scenarios/07-fleet-snapshot.sh" 6

section "Read-only · Encrypted · Audited · Fleet-wide"
sleep 2
