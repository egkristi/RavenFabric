#!/usr/bin/env bash
# Scenario: Port Forwarding (Multi-Distro)
#
# Demonstrates local port forwarding across different Linux distributions.
# Starts web servers on Ubuntu (glibc) and Alpine (musl) agents, then
# shows how to tunnel local ports to each through encrypted channels.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9092}"
RF="${RF_CLI:-rf}"

echo "=== Port Forwarding: Multi-Distro Demo ==="
echo ""

# --- Ubuntu (glibc, python3 available) ---
echo "--- Ubuntu (glibc) ---"
echo ""

echo "[1] Starting web server on Ubuntu agent (port 8000)..."
$RF --relay "$RELAY" exec --token ubuntu \
    'mkdir -p /tmp/www && echo "<h1>Hello from Ubuntu ($(cat /etc/os-release | grep PRETTY_NAME | cut -d= -f2 | tr -d \"))</h1>" > /tmp/www/index.html && nohup python3 -m http.server 8000 --directory /tmp/www > /dev/null 2>&1 &'
echo "  Done."
echo ""
sleep 6

echo "[2] Verify from inside Ubuntu:"
$RF --relay "$RELAY" exec --token ubuntu 'curl -s http://127.0.0.1:8000/'
echo ""
sleep 6

echo "[3] Forward command (Ubuntu):"
echo "  rf --relay $RELAY forward --token ubuntu -L 127.0.0.1:8080 -R 127.0.0.1:8000"
echo ""

# --- Alpine (musl-native, python3 may not be available) ---
echo "--- Alpine (musl-native) ---"
echo ""

echo "[4] Starting web server on Alpine agent (port 8000)..."
$RF --relay "$RELAY" exec --token alpine \
    'mkdir -p /tmp/www && echo "<h1>Hello from Alpine (musl-native)</h1>" > /tmp/www/index.html && if command -v python3 > /dev/null 2>&1; then nohup python3 -m http.server 8000 --directory /tmp/www > /dev/null 2>&1 &; else echo "python3 not available — using busybox httpd"; nohup busybox httpd -f -p 8000 -h /tmp/www > /dev/null 2>&1 &; fi'
echo "  Done."
echo ""
sleep 6

echo "[5] Verify from inside Alpine:"
$RF --relay "$RELAY" exec --token alpine 'wget -qO- http://127.0.0.1:8000/ 2>/dev/null || echo "(web server may not be available on minimal Alpine)"'
echo ""
sleep 6

echo "[6] Forward command (Alpine):"
echo "  rf --relay $RELAY forward --token alpine -L 127.0.0.1:8081 -R 127.0.0.1:8000"
echo ""

# --- Fedora (dnf-based, python3 available) ---
echo "--- Fedora (rpm/dnf) ---"
echo ""

echo "[7] Starting web server on Fedora agent (port 8000)..."
$RF --relay "$RELAY" exec --token fedora \
    'mkdir -p /tmp/www && echo "<h1>Hello from Fedora ($(rpm -E %fedora))</h1>" > /tmp/www/index.html && nohup python3 -m http.server 8000 --directory /tmp/www > /dev/null 2>&1 &'
echo "  Done."
echo ""
sleep 6

echo "[8] Verify from inside Fedora:"
$RF --relay "$RELAY" exec --token fedora 'curl -s http://127.0.0.1:8000/'
echo ""
sleep 6

echo "[9] Forward command (Fedora):"
echo "  rf --relay $RELAY forward --token fedora -L 127.0.0.1:8082 -R 127.0.0.1:8000"
echo ""

# --- Summary ---
echo "--- Summary ---"
echo ""
echo "  Port forwarding works identically across all distros:"
echo "  Each agent's service tunneled through the same Noise XX channel."
echo ""
echo "  Distro      Local Port  →  Agent Port"
echo "  ─────────────────────────────────────"
echo "  Ubuntu      :8080       →  :8000"
echo "  Alpine      :8081       →  :8000"
echo "  Fedora      :8082       →  :8000"
echo ""

# Clean up
echo "[10] Cleaning up web servers..."
for distro in ubuntu alpine fedora; do
    $RF --relay "$RELAY" exec --token "$distro" 'pkill -f "python3 -m http.server" 2>/dev/null; pkill -f "busybox httpd" 2>/dev/null; true' > /dev/null 2>&1 || true
    sleep 6
done
echo "  Done."
echo ""

echo "=== Port Forwarding Demo Complete ==="
echo ""
echo "Key takeaways:"
echo "  - Same forwarding mechanism on glibc (Ubuntu, Fedora) and musl (Alpine)"
echo "  - No firewall changes — all traffic goes through the relay"
echo "  - Each distro's service is tunneled independently"
echo "  - Forwarded traffic is encrypted end-to-end (relay sees nothing)"
