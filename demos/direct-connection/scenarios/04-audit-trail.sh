#!/usr/bin/env bash
# Scenario 04: Audit Trail — inspect the structured audit log
set -euo pipefail

PORT="${LISTEN_PORT:-9999}"
RF="rf --connect ws://127.0.0.1:${PORT}"

echo "=== Scenario 04: Audit Trail ==="
echo ""

echo "--- Generate some audit entries ---"
$RF exec --token unused 'hostname' > /dev/null 2>&1
$RF exec --token unused 'uname -a' > /dev/null 2>&1
$RF exec --token unused 'rm -rf /' 2> /dev/null || true

echo "--- Audit log contents ---"
$RF exec --token unused 'cat /var/log/rf-audit.jsonl'
echo ""

echo "=== Done ==="
