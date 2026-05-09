#!/usr/bin/env bash
# Scenario 08: SOCKS5 Dynamic Port Forwarding
#
# Demonstrates SOCKS5 proxy forwarding through an agent. A local SOCKS5
# proxy is created, and all connections through it are tunneled to the
# agent's network — like SSH -D.
#
# Architecture:
#   Browser (SOCKS5 proxy) → localhost:1080 → [Noise XX tunnel] → agent → internet
#
# Use case: Browse the web or access services as if you were on the agent's
# network, with all traffic encrypted through RavenFabric.
#
# NOTE: SOCKS5 forwarding is implemented at the RPC protocol level
# (Action::Socks5Forward) but does not yet have a dedicated CLI command.
# This scenario shows the concept and architecture.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 08: SOCKS5 Dynamic Port Forwarding ==="
echo ""
echo "SOCKS5 dynamic forwarding creates a local proxy that tunnels"
echo "all traffic through the remote agent's network."
echo ""
echo "Architecture:"
echo "  App (SOCKS5) → localhost:1080 → [encrypted] → agent → destination"
echo ""
echo "This is the equivalent of SSH -D:"
echo "  ssh -D 1080 agent"
echo ""

# Demonstrate network access from agent's perspective
echo "[1] Agent 1's network view:"
$RF --relay "$RELAY" exec --token agent1 'hostname -I 2>/dev/null || ip addr show 2>/dev/null | grep inet | head -3'
echo ""
sleep 6

echo "[2] Agent 2's network view:"
$RF --relay "$RELAY" exec --token agent2 'hostname -I 2>/dev/null || ip addr show 2>/dev/null | grep inet | head -3'
echo ""
sleep 6

echo "[3] DNS resolution from agent 1:"
$RF --relay "$RELAY" exec --token agent1 'cat /etc/resolv.conf | grep nameserver'
echo ""

echo ""
echo "RPC-level SOCKS5 forwarding is available via Action::Socks5Forward."
echo "The agent runs a full SOCKS5 server with CONNECT handling and"
echo "policy-checked destinations."
echo ""
echo "A dedicated CLI command (rf forward --socks5) is planned."
echo ""
echo "=== Scenario 08 Complete ==="
