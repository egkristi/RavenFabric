#!/usr/bin/env bash
# Scenario 02: System Info — collect system information via direct connection
set -euo pipefail

PORT="${LISTEN_PORT:-9999}"
RF="rf --connect ws://127.0.0.1:${PORT}"

echo "=== Scenario 02: System Info ==="
echo ""

echo "--- OS Release ---"
$RF exec --token unused 'cat /etc/os-release'
echo ""

echo "--- Disk Usage ---"
$RF exec --token unused 'df -h'
echo ""

echo "--- Memory ---"
$RF exec --token unused 'free -h'
echo ""

echo "--- Processes ---"
$RF exec --token unused 'ps aux'
echo ""

echo "=== Done ==="
