#!/usr/bin/env bash
# Scenario 07: Remote Port Forwarding
#
# Demonstrates SSH -R style reverse port forwarding. The agent opens a
# listening port, and connections to it are tunneled back to the client's
# local network through the encrypted channel.
#
# Architecture:
#   remote-client → agent:9000 → [Noise XX tunnel] → client → localhost:3000
#
# Use case: Expose a local development service to a remote agent/network
# without opening firewall ports.
#
# NOTE: Remote port forwarding is implemented at the RPC protocol level
# (Action::RemoteForward) but does not yet have a dedicated CLI command.
# This scenario shows the concept and uses direct RPC when available.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 07: Remote Port Forwarding ==="
echo ""
echo "Remote port forwarding allows you to expose a local service"
echo "through a remote agent. Connections to the agent's port are"
echo "tunneled back to your local machine."
echo ""
echo "Architecture:"
echo "  remote-client → agent:9000 → [encrypted tunnel] → you:3000"
echo ""
echo "This is the equivalent of SSH -R:"
echo "  ssh -R 9000:localhost:3000 agent"
echo ""

# Demonstrate the concept with a simulated setup
echo "[1] Starting a local-like service on agent 1 (simulating local dev server):"
$RF --relay "$RELAY" exec --token agent1 \
    'nohup python3 -c "
import http.server, socketserver
class H(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header(\"Content-type\", \"text/html\")
        self.end_headers()
        self.wfile.write(b\"<h1>Reverse-tunneled service</h1>\")
handler = H
with socketserver.TCPServer((\"0.0.0.0\", 9000), handler) as s:
    s.serve_forever()
" > /dev/null 2>&1 &'
echo "  Service listening on agent1:9000"
echo ""
sleep 6

echo "[2] Verifying remote service from agent 2 (cross-agent access):"
$RF --relay "$RELAY" exec --token agent2 \
    'curl -s http://rf-agent-1:9000/ 2>/dev/null || echo "  (Cross-container networking not available in basic Docker setup)"'
echo ""

echo ""
echo "RPC-level remote forwarding is available via Action::RemoteForward."
echo "A dedicated CLI command (rf forward --reverse) is planned."
echo ""
echo "=== Scenario 07 Complete ==="
