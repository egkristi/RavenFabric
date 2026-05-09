#!/usr/bin/env bash
# Scenario 02: Streaming Execution
#
# Demonstrates real-time streaming output from a long-running command.
# Output is delivered incrementally as it's produced, not buffered until completion.
# Uses --stream flag to switch from batch to streaming mode.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 02: Streaming Execution ==="
echo ""

# 1. Stream incremental output (countdown)
echo "[1] Streaming countdown (watch output arrive in real-time):"
$RF --relay "$RELAY" exec --stream --token agent1 \
    'for i in 5 4 3 2 1; do echo "Countdown: $i"; sleep 1; done; echo "Done!"'
echo ""
sleep 6

# 2. Stream a log-like output
echo "[2] Streaming simulated log output:"
$RF --relay "$RELAY" exec --stream --token agent2 \
    'for i in $(seq 1 10); do echo "[$(date +%H:%M:%S)] Log entry $i: status=ok"; sleep 0.5; done'
echo ""
sleep 6

# 3. Stream with mixed stdout/stderr
echo "[3] Streaming with stdout and stderr:"
$RF --relay "$RELAY" exec --stream --token agent1 \
    'echo "stdout: starting" && echo "stderr: warning" >&2 && sleep 1 && echo "stdout: complete"'
echo ""

echo "=== Scenario 02 Complete ==="
