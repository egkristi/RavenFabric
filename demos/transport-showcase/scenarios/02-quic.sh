#!/usr/bin/env bash
# Scenario 02: QUIC Transport (UDP)
#
# Demonstrates QUIC transport: direct client-to-agent connection over
# UDP with built-in multiplexing, 0-RTT reconnect, and connection migration.
# QUIC's TLS 1.3 runs underneath, with Noise XX on top for mutual auth.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Transport Showcase: QUIC (UDP) ==="
echo ""
echo "Running end-to-end encrypted command execution over QUIC:"
echo "  Client → QUIC (UDP) → Agent → Execute → Response"
echo ""

cargo test -p rf-integration-tests --test transport_showcase test_transport_quic -- --nocapture 2>&1 | \
    grep -E '(running|test |ok|FAILED|hello)'

echo ""
echo "QUIC transport: Noise XX handshake + ChaCha20-Poly1305 over UDP."
echo "Benefits: multiplexed streams, 0-RTT reconnect, mobile-friendly."
