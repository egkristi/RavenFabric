#!/usr/bin/env bash
# Scenario 01: WebSocket Transport (TCP)
#
# Demonstrates the default relay transport: relay + agent + client
# communicating over WebSocket (TCP). This is the standard production
# transport — works through proxies, firewalls, and CDNs.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Transport Showcase: WebSocket (TCP) ==="
echo ""
echo "Running end-to-end encrypted command execution over WebSocket:"
echo "  Client → WebSocket → Relay → WebSocket → Agent → Execute → Response"
echo ""

cargo test -p rf-integration-tests --test transport_showcase test_transport_websocket -- --nocapture 2>&1 | \
    grep -E '(running|test |ok|FAILED|hello)'

echo ""
echo "WebSocket transport: Noise XX handshake + ChaCha20-Poly1305 over TCP."
