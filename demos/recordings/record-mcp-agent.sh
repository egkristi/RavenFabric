#!/usr/bin/env bash
# Recording: MCP / AI Agent Demo
# Run: asciinema rec --command "bash demos/recordings/record-mcp-agent.sh" demos/recordings/mcp-agent.cast --cols 100 --rows 28 --overwrite
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
section "RavenFabric — MCP / AI Agent Integration"

comment "AI agents connect via MCP (Model Context Protocol)"
comment "Deny-by-default policy + human approval for sensitive operations"
sleep 1

comment "AI discovers what the policy allows"
run_cmd "cd demos/mcp-agent && ./scenarios/01-policy-discovery.sh" 6

comment "AI executes a safe, policy-allowed command"
run_cmd "./scenarios/02-safe-execution.sh" 6

comment "AI tries a dangerous command — DENIED by policy"
run_cmd "./scenarios/03-policy-denial.sh" 6

comment "AI requests human approval for a sensitive operation"
run_cmd "./scenarios/04-human-approval.sh" 6

comment "Full audit trail of all AI actions"
run_cmd "./scenarios/05-audit-trail.sh" 6

comment "AI reads and writes files (policy-enforced paths)"
run_cmd "./scenarios/06-file-operations.sh" 6

section "Policy gate · Human approval · SHA-256 bound · Audited"
sleep 2
