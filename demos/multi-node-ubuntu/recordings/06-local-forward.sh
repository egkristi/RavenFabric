#!/usr/bin/env bash
# Recording: Local Port Forwarding
# Run inside: asciinema rec --command "bash recordings/06-local-forward.sh"
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Local Port Forwarding"

comment "Forward a local port through the encrypted tunnel to the agent's network"
comment "Like SSH -L, but with Noise XX encryption"
sleep 1

comment "Start a web server on the remote agent"
run_cmd "rf --relay $RELAY exec --token agent1 'mkdir -p /tmp/www && echo \"<h1>Tunneled!</h1>\" > /tmp/www/index.html && python3 -m http.server 8000 --directory /tmp/www > /dev/null 2>&1 &'"
sleep 5

comment "Verify it's running on the agent"
run_cmd "rf --relay $RELAY exec --token agent1 'curl -s http://127.0.0.1:8000/'"
sleep 1

comment "Set up local forward: localhost:8080 → agent:8000"
type_cmd "rf --relay $RELAY forward --token agent1 -L 127.0.0.1:8080 -R 127.0.0.1:8000"
echo ""
echo "  Port forward active: 127.0.0.1:8080 → 127.0.0.1:8000"
echo "  Now curl http://localhost:8080 to access the agent's web server"
echo "  Press Ctrl+C to stop."
sleep 3

section "Secure tunnels — access remote services through encrypted channels"
sleep 2
