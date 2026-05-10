#!/usr/bin/env bash
# Recording: Transport Showcase Demo
# Run: asciinema rec --command "bash demos/recordings/record-transport-showcase.sh" demos/recordings/transport-showcase.cast --cols 100 --rows 28 --overwrite
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
section "RavenFabric — Transport Showcase"

comment "Same Noise XX encryption over 5 fundamentally different transports"
comment "The transport is just a byte pipe — security lives above it"
sleep 1

comment "1. WebSocket (TCP) — default relay transport"
run_cmd "cd demos/transport-showcase && ./scenarios/01-websocket.sh" 6

comment "2. QUIC (UDP) — multiplexed streams, 0-RTT reconnect"
run_cmd "./scenarios/02-quic.sh" 6

comment "3. UNIX Socket — same-host IPC, zero network overhead"
run_cmd "./scenarios/03-unix-socket.sh" 6

comment "4. Stdio Pipe — parent/child process communication"
run_cmd "./scenarios/04-stdio-pipe.sh" 6

comment "5. Memory — in-process tokio::io::duplex channel"
run_cmd "./scenarios/05-memory.sh" 6

section "5 transports · 1 protocol · Identical encryption"
sleep 2
