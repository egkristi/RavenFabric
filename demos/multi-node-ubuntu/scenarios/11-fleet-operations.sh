#!/usr/bin/env bash
# Scenario 11: Multi-Agent Fleet Operations
#
# Demonstrates managing multiple agents as a fleet — running the same
# command across all agents, comparing outputs, and collecting system info.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 11: Multi-Agent Fleet Operations ==="
echo ""

# 1. Collect hostname from all agents
echo "[1] Fleet inventory — hostnames:"
for token in agent1 agent2; do
    echo -n "  $token: "
    $RF --relay "$RELAY" exec --token "$token" 'hostname' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 2. System info across the fleet
echo "[2] System info across fleet:"
for token in agent1 agent2; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'uname -r && cat /etc/os-release | grep PRETTY_NAME' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 3. Disk usage comparison
echo "[3] Disk usage comparison:"
for token in agent1 agent2; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'df -h / | tail -1' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 4. Deploy a file to all agents
echo "[4] Deploying configuration to all agents:"
for token in agent1 agent2; do
    $RF --relay "$RELAY" exec --token "$token" \
        'mkdir -p /opt/app && echo "version: 1.0" > /opt/app/config.yaml && echo "  Deployed to $(hostname)"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 5. Verify deployment
echo "[5] Verifying deployment:"
for token in agent1 agent2; do
    echo -n "  $token: "
    $RF --relay "$RELAY" exec --token "$token" 'cat /opt/app/config.yaml' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

echo "=== Scenario 11 Complete ==="
