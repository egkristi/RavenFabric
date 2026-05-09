#!/usr/bin/env bash
# Scenario 01: Standard Remote Execution
#
# Demonstrates basic command execution on remote agents via the relay.
# Each command is sent encrypted (Noise XX), policy-checked, and audited.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 01: Standard Remote Execution ==="
echo ""

# 1. Simple command on agent 1
echo "[1] hostname on agent 1:"
$RF --relay "$RELAY" exec --token agent1 'hostname'
echo ""
sleep 6

# 2. Multi-command pipeline on agent 2
echo "[2] System info on agent 2:"
$RF --relay "$RELAY" exec --token agent2 'uname -a && cat /etc/os-release | head -3'
echo ""
sleep 6

# 3. Environment and working directory
echo "[3] Environment inspection on agent 1:"
$RF --relay "$RELAY" exec --token agent1 'echo "USER=$USER PWD=$PWD HOSTNAME=$HOSTNAME"'
echo ""
sleep 6

# 4. File operations
echo "[4] Write and read a file on agent 2:"
$RF --relay "$RELAY" exec --token agent2 'echo "Hello from RavenFabric" > /tmp/rf-test.txt && cat /tmp/rf-test.txt'
echo ""
sleep 6

# 5. Process listing
echo "[5] Running processes on agent 1:"
$RF --relay "$RELAY" exec --token agent1 'ps aux --no-header | head -5'
echo ""
sleep 6

# 6. Exit code propagation
echo "[6] Exit code test (expect non-zero):"
$RF --relay "$RELAY" exec --token agent1 'exit 42' || echo "  Exit code propagated correctly (non-zero)"
echo ""

echo "=== Scenario 01 Complete ==="
