#!/usr/bin/env bash
# Automated demo script for asciinema recording.
# Simulates human typing for a polished recording.
#
# This script is run inside asciinema rec --command "bash demos/demo-script.sh"

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RF="$ROOT/target/release/rf"

# Typing simulation
type_cmd() {
    local cmd="$1"
    local delay="${2:-0.04}"
    printf '\033[1;32m$\033[0m '
    for ((i=0; i<${#cmd}; i++)); do
        printf '%s' "${cmd:$i:1}"
        sleep "$delay"
    done
    echo ""
    sleep 0.3
}

run_cmd() {
    type_cmd "$1"
    eval "$1"
    echo ""
    sleep 1.5
}

pause() {
    sleep "${1:-1.5}"
}

# Clean slate
clear
echo ""
printf '\033[1;36m  RavenFabric Demo\033[0m — Security-first distributed execution\n'
echo "  ─────────────────────────────────────────────────────────"
echo ""
sleep 2

# 1. Show binary size
run_cmd "ls -lh $ROOT/target/release/rf | awk '{print \"  rf binary:\", \$5, \"— single static binary, zero dependencies\"}'"

# 2. Start dev mode (relay + agent in one process)
type_cmd "rf dev &"
"$RF" dev > /dev/null 2>&1 &
RF_DEV_PID=$!
sleep 1.5
printf '\033[0;32m  ✓ relay + agent running on ws://127.0.0.1:9090\033[0m\n'
echo ""
sleep 1

# 3. Execute a simple command — E2E encrypted, shows Noise handshake
type_cmd "rf exec --token dev \"echo Hello from RavenFabric\""
"$RF" exec --token dev "echo Hello from RavenFabric" 2>&1
echo ""
sleep 2

# 4. Show policy enforcement — denied command
type_cmd "rf exec --token dev \"rm -rf /\""
"$RF" exec --token dev "rm -rf /" 2>&1 || true
echo ""
sleep 2

# 5. Execute another allowed command
type_cmd "rf exec --token dev \"uname -a\""
"$RF" exec --token dev "uname -a" 2>&1
echo ""
sleep 2

# 6. Stop dev mode
kill "$RF_DEV_PID" 2>/dev/null || true
wait "$RF_DEV_PID" 2>/dev/null || true

echo ""
echo "  ─────────────────────────────────────────────────────────"
printf '\033[1;36m  ✓ Noise XX mutual auth · deny-by-default policy · audit log\033[0m\n'
printf '\033[0;37m  github.com/egkristi/RavenFabric\033[0m\n'
echo ""
sleep 3
