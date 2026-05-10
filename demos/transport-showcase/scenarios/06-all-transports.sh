#!/usr/bin/env bash
# Scenario 06: All Transports — Sequential Comparison
#
# Runs all 5 demoable transports sequentially, proving that
# the same Noise XX handshake + ChaCha20-Poly1305 encryption
# works identically over every byte channel.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "============================================================"
echo "  RavenFabric Transport Showcase — All Transports"
echo "============================================================"
echo ""
echo "Running the same encrypted RPC operation over 5 different"
echo "transports. Each uses identical Noise XX mutual authentication"
echo "and ChaCha20-Poly1305 encryption — only the byte channel differs."
echo ""
echo "------------------------------------------------------------"

PASS=0
FAIL=0

for transport in websocket quic unix_socket memory stdio_pipe; do
    case "$transport" in
        websocket)    display_name="WebSocket (TCP)" ;;
        quic)         display_name="QUIC (UDP)" ;;
        unix_socket)  display_name="UNIX Socket (IPC)" ;;
        memory)       display_name="Memory (In-Process)" ;;
        stdio_pipe)   display_name="Stdio Pipe (Process)" ;;
    esac
    printf "  %-25s " "$display_name"

    if cargo test -p rf-integration-tests --test transport_showcase "test_transport_${transport}" 2>&1 | grep -q "test result: ok"; then
        echo "PASS"
        PASS=$((PASS + 1))
    else
        echo "FAIL"
        FAIL=$((FAIL + 1))
    fi
done

echo ""
echo "------------------------------------------------------------"
echo "Results: $PASS passed, $FAIL failed (5 transports tested)"
echo ""

if [ "$FAIL" -eq 0 ]; then
    echo "All transports verified: same encryption, same policy,"
    echo "same execution — only the byte-moving layer changes."
else
    echo "Some transports failed. Run individual scenario scripts for details."
    exit 1
fi
