#!/usr/bin/env bash
# Scenario 8: Policy Boundary Testing
#
# Demonstrates the deny-by-default policy in action.
# The data collection policy only allows read operations —
# any write, install, or destructive command is denied.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9092}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 8: Policy Boundary Testing ==="
echo ""
echo "The data collection policy allows ONLY read operations."
echo "All write/destructive commands should be denied."
echo ""

# --- Allowed commands (should succeed) ---

echo "--- ALLOWED COMMANDS (should succeed) ---"
echo ""

echo "[1] Read hostname (allowed):"
$RF --relay "$RELAY" exec --token webserver 'hostname' 2>/dev/null | grep -v "^2"
echo ""
sleep 6

echo "[2] Read system info (allowed):"
$RF --relay "$RELAY" exec --token webserver 'uname -a' 2>/dev/null | grep -v "^2"
echo ""
sleep 6

echo "[3] Read /proc (allowed):"
$RF --relay "$RELAY" exec --token webserver 'cat /proc/loadavg' 2>/dev/null | grep -v "^2"
echo ""
sleep 6

echo "[4] Read logs (allowed):"
$RF --relay "$RELAY" exec --token webserver 'tail -3 /var/log/access.log' 2>/dev/null | grep -v "^2"
echo ""
sleep 6

echo "[5] Read config (allowed):"
$RF --relay "$RELAY" exec --token webserver 'cat /opt/app/config.yaml' 2>/dev/null | grep -v "^2"
echo ""
sleep 6

# --- Denied commands (should fail) ---

echo "--- DENIED COMMANDS (should be rejected by policy) ---"
echo ""

echo "[6] Attempt: rm file (denied):"
$RF --relay "$RELAY" exec --token webserver 'rm /opt/app/config.yaml' 2>&1 || true
echo ""
sleep 6

echo "[7] Attempt: curl external URL (denied):"
$RF --relay "$RELAY" exec --token webserver 'curl https://example.com' 2>&1 || true
echo ""
sleep 6

echo "[8] Attempt: install package (denied):"
$RF --relay "$RELAY" exec --token webserver 'apt install nginx' 2>&1 || true
echo ""
sleep 6

echo "[9] Attempt: shutdown (denied):"
$RF --relay "$RELAY" exec --token webserver 'shutdown -h now' 2>&1 || true
echo ""
sleep 6

echo "[10] Attempt: kill process (denied):"
$RF --relay "$RELAY" exec --token webserver 'kill -9 1' 2>&1 || true
echo ""
sleep 6

echo "[11] Attempt: modify permissions (denied):"
$RF --relay "$RELAY" exec --token webserver 'chmod 777 /etc/ravenfabric/policy.yaml' 2>&1 || true
echo ""
sleep 6

echo "[12] Attempt: mount filesystem (denied):"
$RF --relay "$RELAY" exec --token webserver 'mount /dev/sda1 /mnt' 2>&1 || true
echo ""
sleep 6

echo "--- SUMMARY ---"
echo ""
echo "  Allowed: hostname, uname, cat /proc/*, tail /var/log/*, cat config"
echo "  Denied:  rm, curl, apt, shutdown, kill, chmod, mount"
echo "  Policy:  deny-by-default with explicit read-only allowlist"
echo ""

echo "=== Scenario 8 Complete ==="
