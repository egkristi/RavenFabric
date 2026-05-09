#!/usr/bin/env bash
# Scenario 04: Interactive Shell
#
# Demonstrates opening a full interactive PTY shell session on a remote agent.
# The session is encrypted end-to-end and supports terminal resize.
#
# This scenario cannot be run non-interactively — it opens a live shell.
# Use the recording script (recordings/04-interactive-shell.sh) for asciinema.
#
# Prerequisites: ./setup.sh has been run
# Platform:      Unix only (PTY allocation)

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 04: Interactive Shell ==="
echo ""
echo "Opening an interactive shell on agent 1..."
echo "Type 'exit' to close the session."
echo ""

$RF --relay "$RELAY" shell --token agent1

echo ""
echo "=== Scenario 04 Complete ==="
