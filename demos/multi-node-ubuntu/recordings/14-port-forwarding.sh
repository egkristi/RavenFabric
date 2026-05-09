#!/usr/bin/env bash
# Recording: Port Forwarding
# Run inside: asciinema rec --command "bash recordings/14-port-forwarding.sh"
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Port Forwarding"

comment "Forward local ports through Noise XX encrypted tunnels"
comment "Like SSH -L, but without SSH, without certificates, without firewall rules"
sleep 1

comment "Start a web server on the remote agent"
run_cmd "rf --relay $RELAY exec --token agent1 'mkdir -p /tmp/www && echo \"<h1>Tunneled from agent1</h1>\" > /tmp/www/index.html && python3 -m http.server 8000 --directory /tmp/www > /dev/null 2>&1 &'"
sleep 5

comment "Verify it's running inside the agent"
run_cmd "rf --relay $RELAY exec --token agent1 'curl -s http://127.0.0.1:8000/'"
sleep 5

comment "Set up local port forward: localhost:8080 → agent:8000"
type_cmd "rf --relay $RELAY forward --token agent1 -L 127.0.0.1:8080 -R 127.0.0.1:8000"
echo ""
echo "  Port forward active — all traffic encrypted through relay"
echo "  curl http://localhost:8080  →  tunneled to agent1:8000"
sleep 3

comment "Forward types available:"
echo "  Local:   -L localhost:8080 → agent:8000   (SSH -L equivalent)"
echo "  Reverse: --reverse agent:9000 → you:3000  (SSH -R equivalent)"
echo "  SOCKS5:  --socks5 localhost:1080 → agent   (SSH -D equivalent)"
sleep 2

comment "Clean up"
run_cmd "rf --relay $RELAY exec --token agent1 'pkill -f \"python3 -m http.server\" 2>/dev/null || true'" 2
sleep 5

section "Encrypted tunnels — no firewall changes, no port exposure"
sleep 2
