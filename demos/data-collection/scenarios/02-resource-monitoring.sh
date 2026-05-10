#!/usr/bin/env bash
# Scenario 2: Resource Monitoring
#
# Collects real-time resource utilization across the fleet:
# CPU load, memory pressure, disk I/O, process counts.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9092}"
RF="${RF_CLI:-rf}"
AGENTS=("collector" "webserver" "database")

echo "=== Scenario 2: Resource Monitoring ==="
echo ""

# 1. Load averages
echo "[1] Load averages (1m / 5m / 15m):"
for token in "${AGENTS[@]}"; do
    echo -n "  $token: "
    $RF --relay "$RELAY" exec --token "$token" 'cat /proc/loadavg' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 2. Memory utilization with percentages
echo "[2] Memory utilization:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'free -h | head -2 && echo "usage: $(free | awk "/Mem:/ {printf \"%.1f%%\", \$3/\$2 * 100}")"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 3. Disk I/O stats
echo "[3] Disk I/O stats:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'cat /proc/diskstats | head -5' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 4. Top processes by CPU
echo "[4] Top 5 processes by CPU:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'ps aux --sort=-%cpu | head -6' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 5. Process count
echo "[5] Process counts:"
for token in "${AGENTS[@]}"; do
    echo -n "  $token: "
    $RF --relay "$RELAY" exec --token "$token" 'echo "$(ps aux | wc -l) processes running"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 6. Filesystem usage breakdown
echo "[6] Filesystem usage by mount:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'df -h' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

echo "=== Scenario 2 Complete ==="
