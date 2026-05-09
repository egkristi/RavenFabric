#!/usr/bin/env bash
# Recording: Kubernetes + CloudNativePG Demo
# Run: asciinema rec --command "bash demos/recordings/record-k8s-cnpg.sh" demos/recordings/k8s-cnpg.cast --cols 100 --rows 28 --overwrite
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$ROOT/target/release:$PATH"
export RUST_LOG=warn
RELAY="ws://127.0.0.1:9093"

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
section "RavenFabric — Kubernetes + CloudNativePG"

comment "Access a CNPG PostgreSQL cluster through an encrypted RavenFabric tunnel"
comment "Agent runs as a K8s deployment alongside a 2-node CNPG cluster"
sleep 1

comment "Check the agent pod's OS and environment"
run_cmd "rf --relay $RELAY exec --token cnpg 'cat /etc/os-release | head -2'" 6

comment "Query PostgreSQL version via psql (credentials auto-injected from CNPG secret)"
run_cmd "rf --relay $RELAY exec --token cnpg 'psql -c \"SELECT version();\"'" 6

comment "List databases"
run_cmd "rf --relay $RELAY exec --token cnpg 'psql -c \"\\l\"'" 6

comment "Check replication status (primary → replica)"
run_cmd "rf --relay $RELAY exec --token cnpg 'psql -c \"SELECT client_addr, state, sync_state FROM pg_stat_replication;\"'" 6

comment "Create a table and query it"
run_cmd "rf --relay $RELAY exec --token cnpg 'psql -c \"CREATE TABLE IF NOT EXISTS demo(id serial PRIMARY KEY, name text); INSERT INTO demo(name) VALUES ('\\''ravenfabric'\\''),('\\''kubernetes'\\''),('\\''cnpg'\\''); SELECT * FROM demo;\"'" 6

comment "Clean up"
run_cmd "rf --relay $RELAY exec --token cnpg 'psql -c \"DROP TABLE IF EXISTS demo;\"'" 4

section "E2E encrypted · Policy-checked · Audited · Kubernetes-native"
sleep 2
