#!/usr/bin/env bash
# Scenario 1: System Inventory
#
# Collects basic system information from all agents in the fleet:
# hostname, OS version, kernel, CPU, memory, disk.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9092}"
RF="${RF_CLI:-rf}"
AGENTS=("collector" "webserver" "database")

echo "=== Scenario 1: System Inventory ==="
echo ""

# 1. Hostname and OS identification
echo "[1] Fleet hostnames and OS:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'echo "hostname: $(hostname)" && echo "os: $(cat /etc/os-release | grep PRETTY_NAME | cut -d= -f2 | tr -d \")"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 2. Kernel version
echo "[2] Kernel versions:"
for token in "${AGENTS[@]}"; do
    echo -n "  $token: "
    $RF --relay "$RELAY" exec --token "$token" 'uname -r' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 3. CPU information
echo "[3] CPU info:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'cat /proc/cpuinfo | grep "model name" | head -1 && echo "cores: $(cat /proc/cpuinfo | grep processor | wc -l)"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 4. Memory summary
echo "[4] Memory summary:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'free -h | head -2' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 5. Disk usage
echo "[5] Disk usage:"
for token in "${AGENTS[@]}"; do
    echo -n "  $token: "
    $RF --relay "$RELAY" exec --token "$token" 'df -h / | tail -1' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 6. System uptime
echo "[6] Uptime:"
for token in "${AGENTS[@]}"; do
    echo -n "  $token: "
    $RF --relay "$RELAY" exec --token "$token" 'uptime' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

echo "=== Scenario 1 Complete ==="
