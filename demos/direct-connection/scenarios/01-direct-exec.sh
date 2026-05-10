#!/usr/bin/env bash
# Scenario 01: Direct Exec — basic command execution via direct connection
set -euo pipefail

PORT="${LISTEN_PORT:-9999}"
RF="rf --connect ws://127.0.0.1:${PORT}"

echo "=== Scenario 01: Direct Exec ==="
echo ""

echo "--- hostname ---"
$RF exec --token unused 'hostname'
echo ""

echo "--- uname -a ---"
$RF exec --token unused 'uname -a'
echo ""

echo "--- whoami ---"
$RF exec --token unused 'whoami'
echo ""

echo "--- date ---"
$RF exec --token unused 'date'
echo ""

echo "=== Done ==="
