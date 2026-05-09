#!/usr/bin/env bash
# Fleet Orchestration — Multi-Distro Linux
#
# Demonstrates fleet orchestration across heterogeneous Linux distributions.
# Run the same playbook across Ubuntu, Alpine, Fedora, and more — the static
# binary and orchestration engine work identically regardless of distro.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9092}"
RF="${RF_CLI:-rf}"

echo "=== Fleet Orchestration — Multi-Distro ==="
echo ""

# --- Part 1: Cross-Distro Fleet Inventory ---

echo "--- Part 1: Cross-Distro Fleet Inventory ---"
echo ""

DISTROS=("ubuntu:Ubuntu" "alpine:Alpine" "fedora:Fedora" "debian:Debian" "rocky:Rocky")

for entry in "${DISTROS[@]}"; do
    token="${entry%%:*}"
    label="${entry#*:}"
    echo "  [$label]"
    $RF --relay "$RELAY" exec --token "$token" \
        'echo "    Hostname: $(hostname)" && echo "    Kernel: $(uname -r)" && echo "    libc: $(ldd --version 2>&1 | head -1 || echo musl)"' 2>/dev/null | grep -v "^2"
    sleep 4
done
echo ""

# --- Part 2: Parallel Deploy Across Distros ---

echo "--- Part 2: Parallel Deploy Across Distros ---"
echo ""
echo "  Same command runs on glibc, musl, rpm, and deb-based systems."
echo ""

for entry in "${DISTROS[@]}"; do
    token="${entry%%:*}"
    label="${entry#*:}"
    echo -n "  Deploying to $label... "
    $RF --relay "$RELAY" exec --token "$token" \
        'mkdir -p /opt/app && echo "v2.0" > /opt/app/version.txt && echo "done"' 2>/dev/null | grep -v "^2"
    sleep 4
done
echo ""

# --- Part 3: Fleet Verification ---

echo "--- Part 3: Verify Deployment Across All Distros ---"
echo ""

for entry in "${DISTROS[@]}"; do
    token="${entry%%:*}"
    label="${entry#*:}"
    echo -n "  $label: version "
    $RF --relay "$RELAY" exec --token "$token" 'cat /opt/app/version.txt' 2>/dev/null | grep -v "^2"
    sleep 4
done
echo ""

# --- Part 4: Why Cross-Distro Matters ---

echo "--- Part 4: Why This Matters ---"
echo ""
echo "  Traditional tools require per-distro agents (apt vs dnf vs apk)."
echo "  RavenFabric uses one static binary — same playbook, any distro."
echo ""
echo "  ┌──────────────┬────────────┬────────────┬─────────────┐"
echo "  │ Distribution │ Package Mgr│ libc       │ rf works?   │"
echo "  ├──────────────┼────────────┼────────────┼─────────────┤"
echo "  │ Ubuntu       │ apt        │ glibc      │ Yes         │"
echo "  │ Alpine       │ apk        │ musl       │ Yes         │"
echo "  │ Fedora       │ dnf        │ glibc      │ Yes         │"
echo "  │ Debian       │ apt        │ glibc      │ Yes         │"
echo "  │ Rocky        │ dnf        │ glibc      │ Yes         │"
echo "  └──────────────┴────────────┴────────────┴─────────────┘"
echo ""
sleep 6

# --- Key Takeaways ---

echo "=== Key Takeaways ==="
echo ""
echo "  1. One playbook works across all Linux distributions"
echo "  2. Static binary = no per-distro agent packages"
echo "  3. Fleet inventory, deploy, and verify — all via rf"
echo "  4. Same orchestration strategies on heterogeneous fleets"
echo ""
echo "=== Fleet Orchestration Scenario Complete ==="
