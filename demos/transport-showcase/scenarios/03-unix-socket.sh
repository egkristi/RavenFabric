#!/usr/bin/env bash
# Scenario 03: UNIX Socket Transport (IPC)
#
# Demonstrates same-host communication via UNIX domain socket.
# Zero network overhead — used for sidecar patterns, container-to-container,
# and local AI agent ↔ RavenFabric communication.
# Same Noise XX handshake applies — local does not mean trusted.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Transport Showcase: UNIX Socket (IPC) ==="
echo ""
echo "Running end-to-end encrypted command execution over UNIX socket:"
echo "  Client → UNIX Socket → Agent → Execute → Response"
echo ""

cargo test -p rf-integration-tests --test transport_showcase test_transport_unix_socket -- --nocapture 2>&1 | \
    grep -E '(running|test |ok|FAILED|hello)'

echo ""
echo "UNIX socket transport: Noise XX handshake + ChaCha20-Poly1305 over IPC."
echo "Benefits: zero network, ~0.1ms latency, peer credential verification."
