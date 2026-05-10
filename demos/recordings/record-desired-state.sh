#!/usr/bin/env bash
# Recording: Desired-State Convergence Demo
# Run: asciinema rec --command "bash demos/recordings/record-desired-state.sh" demos/recordings/desired-state.cast --cols 100 --rows 28 --overwrite
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
section "RavenFabric — Desired-State Convergence"

comment "Declarative desired-state with drift detection and auto-remediation"
comment "Define what should be true — RavenFabric makes it so"
sleep 1

comment "Scenario 1: Detect drift without making changes"
run_cmd "cd demos/desired-state && ./scenarios/01-drift-detection.sh" 6

comment "Scenario 2: Auto-remediate drifted resources"
run_cmd "./scenarios/02-auto-remediation.sh" 6

comment "Scenario 3: Report-only mode (monitoring, no changes)"
run_cmd "./scenarios/03-report-mode.sh" 6

comment "Scenario 4: Target agents by grains (OS, arch, role)"
run_cmd "./scenarios/05-grains-targeting.sh" 6

comment "Scenario 5: Event-triggered convergence (file watch, timer)"
run_cmd "./scenarios/05-event-triggers.sh" 6

section "Declare · Detect · Remediate · Verify"
sleep 2
