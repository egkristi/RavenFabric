#!/usr/bin/env bash
# Scenario 7: Fleet Snapshot
#
# Collects a comprehensive point-in-time snapshot of the entire fleet
# into a structured report. This is the "full collection run" that
# combines inventory, resources, and health into one pass.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9092}"
RF="${RF_CLI:-rf}"
AGENTS=("collector" "webserver" "database")

TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)

echo "=== Scenario 7: Fleet Snapshot ==="
echo ""
echo "Snapshot timestamp: ${TIMESTAMP}"
echo ""

# Collect from each agent
for token in "${AGENTS[@]}"; do
    echo "================================================================"
    echo "  Agent: $token"
    echo "================================================================"
    echo ""

    echo "  [hostname]"
    $RF --relay "$RELAY" exec --token "$token" 'hostname' 2>/dev/null | grep -v "^2"
    sleep 6

    echo "  [os]"
    $RF --relay "$RELAY" exec --token "$token" 'cat /etc/os-release | grep PRETTY_NAME | cut -d= -f2 | tr -d \"' 2>/dev/null | grep -v "^2"
    sleep 6

    echo "  [kernel]"
    $RF --relay "$RELAY" exec --token "$token" 'uname -r' 2>/dev/null | grep -v "^2"
    sleep 6

    echo "  [uptime]"
    $RF --relay "$RELAY" exec --token "$token" 'uptime' 2>/dev/null | grep -v "^2"
    sleep 6

    echo "  [cpu]"
    $RF --relay "$RELAY" exec --token "$token" 'cat /proc/cpuinfo | grep "model name" | head -1' 2>/dev/null | grep -v "^2"
    sleep 6

    echo "  [memory]"
    $RF --relay "$RELAY" exec --token "$token" 'free -h | grep Mem' 2>/dev/null | grep -v "^2"
    sleep 6

    echo "  [disk]"
    $RF --relay "$RELAY" exec --token "$token" 'df -h / | tail -1' 2>/dev/null | grep -v "^2"
    sleep 6

    echo "  [load]"
    $RF --relay "$RELAY" exec --token "$token" 'cat /proc/loadavg' 2>/dev/null | grep -v "^2"
    sleep 6

    echo "  [ip]"
    $RF --relay "$RELAY" exec --token "$token" 'ip addr show | grep "inet " | grep -v 127.0.0.1 | head -1' 2>/dev/null | grep -v "^2"
    sleep 6

    echo "  [processes]"
    $RF --relay "$RELAY" exec --token "$token" 'echo "$(ps aux | wc -l) running"' 2>/dev/null | grep -v "^2"
    sleep 6

    echo "  [config]"
    $RF --relay "$RELAY" exec --token "$token" 'cat /opt/app/config.yaml 2>/dev/null || echo "none"' 2>/dev/null | grep -v "^2"
    sleep 6

    echo ""
done

echo "================================================================"
echo "  Snapshot Summary"
echo "================================================================"
echo ""
echo "  Agents collected: ${#AGENTS[@]}"
echo "  Timestamp:        ${TIMESTAMP}"
echo "  Collection mode:  read-only (deny-by-default policy)"
echo ""

echo "=== Scenario 7 Complete ==="
