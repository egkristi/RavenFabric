#!/usr/bin/env bash
# Scenario 06: Local Port Forwarding
#
# Demonstrates SSH -L style local port forwarding through an agent.
# A local port is bound and all connections are forwarded through the
# encrypted RavenFabric channel to a target address accessible from the agent.
#
# Architecture:
#   Browser → localhost:8080 → [Noise XX tunnel] → agent → target:80
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 06: Local Port Forwarding ==="
echo ""

# 1. Start a web server on agent 1
echo "[1] Starting a web server on agent 1 (port 8000):"
$RF --relay "$RELAY" exec --token agent1 \
    'mkdir -p /tmp/www && echo "<h1>Hello from agent 1 ($(hostname))</h1><p>Served through RavenFabric encrypted tunnel</p>" > /tmp/www/index.html && nohup python3 -m http.server 8000 --directory /tmp/www > /dev/null 2>&1 &'
echo "  Web server started on agent 1:8000"
echo ""
sleep 6

# 2. Verify the web server is running inside the container
echo "[2] Verifying web server inside agent 1:"
$RF --relay "$RELAY" exec --token agent1 'curl -s http://127.0.0.1:8000/'
echo ""
sleep 6

# 3. Forward local port to agent's web server
echo "[3] Setting up local port forward:"
echo "    localhost:8080 → agent1:8000"
echo ""
echo "    In another terminal, try: curl http://localhost:8080"
echo "    Press Ctrl+C to stop the forward."
echo ""

$RF --relay "$RELAY" forward --token agent1 -L 127.0.0.1:8080 -R 127.0.0.1:8000

echo ""
echo "=== Scenario 06 Complete ==="
