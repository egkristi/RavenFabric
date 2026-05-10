#!/usr/bin/env bash
# Scenario 05: Memory Transport (In-Process)
#
# Demonstrates in-process communication via tokio::io::duplex.
# Both client and agent run in the same Tokio runtime — used for
# testing, rf dev mode, and embedded scenarios.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Transport Showcase: Memory (In-Process) ==="
echo ""
echo "Running end-to-end encrypted command execution over in-process channel:"
echo "  Client (same process) ↔ Agent → Execute → Response"
echo ""

cargo test -p rf-integration-tests --test transport_showcase test_transport_memory -- --nocapture 2>&1 | \
    grep -E '(running|test |ok|FAILED|hello)'

echo ""
echo "Memory transport: Noise XX handshake + ChaCha20-Poly1305 over tokio::io::duplex."
echo "Benefits: zero overhead, ~0.01ms latency, used by rf dev mode."
