#!/usr/bin/env bash
# Scenario 03: Policy Denial — verify deny-by-default blocks dangerous commands
set -euo pipefail

PORT="${LISTEN_PORT:-9999}"
RF="rf --connect ws://127.0.0.1:${PORT}"

echo "=== Scenario 03: Policy Denial ==="
echo ""

echo "--- Allowed: hostname ---"
$RF exec --token unused 'hostname' && echo "(allowed)"
echo ""

echo "--- Denied: rm -rf / ---"
$RF exec --token unused 'rm -rf /' || echo "(denied — policy blocked)"
echo ""

echo "--- Denied: shutdown now ---"
$RF exec --token unused 'shutdown now' || echo "(denied — policy blocked)"
echo ""

echo "--- Denied: curl (not in allow list) ---"
$RF exec --token unused 'curl https://example.com' || echo "(denied — not in allow list)"
echo ""

echo "=== Done ==="
