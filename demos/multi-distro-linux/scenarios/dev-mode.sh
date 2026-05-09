#!/usr/bin/env bash
# Dev Mode (Zero-Setup) — Multi-Distro Linux
#
# Demonstrates that dev mode works identically regardless of the host
# distribution. Since dev mode runs relay + agent in a single process,
# it's the fastest way to test RavenFabric on any Linux distro.
#
# No Docker compose, no config files — just the rf binary.
#
# Prerequisites: rf binary available in $PATH

set -euo pipefail
cd "$(dirname "$0")/.."

RF="${RF_CLI:-rf}"

echo "=== Dev Mode (Zero-Setup) — Multi-Distro ==="
echo ""

# --- Part 1: Dev Mode on Any Distro ---

echo "--- Part 1: One Binary, Any Distro ---"
echo ""
echo "  Dev mode works identically on every Linux distribution."
echo "  The statically-linked rf binary has zero dependencies."
echo ""
echo "  Ubuntu (glibc):     rf dev  →  ready"
echo "  Alpine (musl):      rf dev  →  ready"
echo "  Fedora (rpm):       rf dev  →  ready"
echo "  Arch (pacman):      rf dev  →  ready"
echo "  Void (xbps/musl):   rf dev  →  ready"
echo ""
echo "  No package manager required. No libraries to install."
echo ""
sleep 6

# --- Part 2: Show it on Selected Distros ---

DISTROS=("ubuntu:Ubuntu (glibc)" "alpine:Alpine (musl)" "fedora:Fedora (rpm)")

echo "--- Part 2: Verify Dev Mode Across Distros ---"
echo ""

for entry in "${DISTROS[@]}"; do
    token="${entry%%:*}"
    label="${entry#*:}"
    echo "  [$label]"
    echo "    Start:  rf dev"
    echo "    Test:   rf exec --token dev 'hostname && uname -r'"
    echo "    Stop:   Ctrl+C"
    echo ""
    sleep 4
done

# --- Part 3: Comparison ---

echo "--- Part 3: Dev Mode vs Docker Demo Setup ---"
echo ""
echo "  ┌─────────────────────┬──────────────────────────┬──────────────────────────┐"
echo "  │                     │ Dev Mode                 │ Docker Demo              │"
echo "  ├─────────────────────┼──────────────────────────┼──────────────────────────┤"
echo "  │ Prerequisites       │ rf binary only           │ Docker + docker-compose  │"
echo "  │ Setup time          │ < 1 second               │ 30-60 seconds            │"
echo "  │ Config files        │ None                     │ docker-compose.yaml      │"
echo "  │ Multiple distros    │ One at a time            │ All 9 simultaneously     │"
echo "  │ Cross-distro test   │ No (single process)      │ Yes (separate containers)│"
echo "  │ Resource usage      │ ~5 MB                    │ ~500 MB                  │"
echo "  └─────────────────────┴──────────────────────────┴──────────────────────────┘"
echo ""
sleep 6

# --- Key Takeaways ---

echo "=== Key Takeaways ==="
echo ""
echo "  1. Same rf binary, same dev mode, works on every distro"
echo "  2. Static linking means zero host dependencies"
echo "  3. Dev mode is ideal for single-node testing and development"
echo "  4. Use the Docker demo for multi-distro cross-testing"
echo ""
echo "=== Dev Mode Scenario Complete ==="
