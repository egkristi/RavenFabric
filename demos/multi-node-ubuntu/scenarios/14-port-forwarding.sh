#!/usr/bin/env bash
# Scenario 14: Port Forwarding
#
# Demonstrates SSH-style port forwarding through encrypted RavenFabric
# tunnels. Covers local forwarding (-L), reverse forwarding (-R concept),
# and SOCKS5 dynamic forwarding (-D concept).
#
# Architecture:
#   Local:   browser → localhost:8080 → [Noise XX] → agent → target:8000
#   Reverse: remote  → agent:9000    → [Noise XX] → client → local:3000
#   SOCKS5:  app     → localhost:1080 → [Noise XX] → agent → destination
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 14: Port Forwarding ==="
echo ""

# --- Part 1: Local Port Forwarding (SSH -L equivalent) ---

echo "--- Part 1: Local Port Forwarding ---"
echo ""
echo "  Architecture: localhost:8080 → [encrypted tunnel] → agent1:8000"
echo ""

# 1. Start a web server on agent 1
echo "[1] Starting a web server on agent 1 (port 8000)..."
$RF --relay "$RELAY" exec --token agent1 \
    'mkdir -p /tmp/www && echo "<h1>Hello from $(hostname)</h1><p>Served through RavenFabric encrypted tunnel</p><p>Agent: rf-agent-1</p>" > /tmp/www/index.html && nohup python3 -m http.server 8000 --directory /tmp/www > /dev/null 2>&1 &'
echo "  Web server started."
echo ""
sleep 6

# 2. Verify it works inside the container
echo "[2] Verify web server from inside the agent:"
$RF --relay "$RELAY" exec --token agent1 'curl -s http://127.0.0.1:8000/'
echo ""
sleep 6

# 3. Start a second web server on agent 2
echo "[3] Starting a web server on agent 2 (port 8000)..."
$RF --relay "$RELAY" exec --token agent2 \
    'mkdir -p /tmp/www && echo "<h1>Hello from $(hostname)</h1><p>Agent: rf-agent-2</p>" > /tmp/www/index.html && nohup python3 -m http.server 8000 --directory /tmp/www > /dev/null 2>&1 &'
echo "  Web server started."
echo ""
sleep 6

# 4. Show the forward command
echo "[4] Local port forward command:"
echo ""
echo "  # Forward localhost:8080 to agent1's web server"
echo "  rf --relay $RELAY forward --token agent1 -L 127.0.0.1:8080 -R 127.0.0.1:8000"
echo ""
echo "  # Then in another terminal:"
echo "  curl http://localhost:8080"
echo "  # → <h1>Hello from rf-agent-1</h1>"
echo ""

# --- Part 2: Cross-Agent Port Access ---

echo "--- Part 2: Cross-Agent Service Access ---"
echo ""

echo "[5] Agent 1 network info:"
$RF --relay "$RELAY" exec --token agent1 'hostname -I 2>/dev/null || ip addr show eth0 2>/dev/null | grep inet | head -1'
echo ""
sleep 6

echo "[6] Agent 2 network info:"
$RF --relay "$RELAY" exec --token agent2 'hostname -I 2>/dev/null || ip addr show eth0 2>/dev/null | grep inet | head -1'
echo ""
sleep 6

echo "[7] Each agent's service is only accessible through its own tunnel."
echo "    No direct network path between agents — relay pairs connections by token."
echo ""

# --- Part 3: Forward types overview ---

echo "--- Part 3: Port Forwarding Types ---"
echo ""
echo "  Type          CLI Flag    Architecture"
echo "  ─────────────────────────────────────────────────────────────"
echo "  Local         -L          localhost:PORT → tunnel → agent:PORT"
echo "  Reverse       --reverse   agent:PORT → tunnel → localhost:PORT"
echo "  SOCKS5        --socks5    localhost:1080 → tunnel → agent → dest"
echo ""
echo "  All forwarding types use the same Noise XX encrypted channel."
echo "  The relay never sees forwarded traffic content."
echo ""

# Clean up web servers
echo "[8] Cleaning up web servers..."
$RF --relay "$RELAY" exec --token agent1 'pkill -f "python3 -m http.server" 2>/dev/null || true' > /dev/null 2>&1 || true
sleep 6
$RF --relay "$RELAY" exec --token agent2 'pkill -f "python3 -m http.server" 2>/dev/null || true' > /dev/null 2>&1 || true
echo "  Done."
echo ""

echo "=== Scenario 14 Complete ==="
echo ""
echo "Key takeaways:"
echo "  - Local forwarding (-L) tunnels a local port to a remote service"
echo "  - All forwarded traffic is encrypted end-to-end (Noise XX)"
echo "  - Policy engine controls which forwarding destinations are allowed"
echo "  - Relay never sees forwarded data — it's a dumb pipe"
echo "  - No firewall ports needed — everything goes through the relay"
