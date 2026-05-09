#!/usr/bin/env bash
# Recording: Interactive Shell
# Run inside: asciinema rec --command "bash recordings/04-interactive-shell.sh"
#
# NOTE: This recording uses scripted input to simulate an interactive session.
# A real interactive shell session would require human input.
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Interactive Shell"

comment "Open a full PTY shell session on a remote agent"
comment "Encrypted end-to-end, supports terminal resize"
sleep 1

comment "Starting shell (type commands, then 'exit' to close)"
type_cmd "rf --relay $RELAY shell --token agent1"
echo ""
echo "  (Interactive shell session — this recording shows the concept)"
echo "  In a live session, you get a full bash prompt on the remote agent."
echo "  All keystrokes are encrypted and forwarded through the Noise XX tunnel."
echo ""
sleep 3

section "Full PTY allocation — like SSH, but with Noise XX encryption"
sleep 2
