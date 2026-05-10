#!/usr/bin/env bash
# Scenario 04: Stdio Pipe Transport (Parent-Child Process)
#
# Demonstrates parent-child process communication over stdin/stdout.
# Used for MCP server integration (Claude Desktop, IDE extensions),
# embedded agents spawned by a parent process, and subprocess isolation.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Transport Showcase: Stdio Pipe (Process) ==="
echo ""
echo "Running end-to-end encrypted command execution over stdio pipe:"
echo "  Parent (stdin/stdout) ↔ Child Process → Execute → Response"
echo ""

cargo test -p rf-integration-tests --test transport_showcase test_transport_stdio_pipe -- --nocapture 2>&1 | \
    grep -E '(running|test |ok|FAILED|hello)'

echo ""
echo "Stdio pipe transport: Noise XX handshake + ChaCha20-Poly1305 over stdin/stdout."
echo "Benefits: no network config, works with any process launcher, MCP-compatible."
