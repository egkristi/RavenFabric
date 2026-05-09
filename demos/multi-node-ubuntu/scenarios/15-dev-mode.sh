#!/usr/bin/env bash
# Scenario 15: Dev Mode (Zero-Setup)
#
# Demonstrates RavenFabric's dev mode — a single command that spins up
# a relay and agent in one process with a permissive policy. No Docker,
# no config files, no key management. Perfect for local development and
# quick experiments.
#
# Architecture:
#   rf dev  →  [relay on :9090] + [agent with token "dev"]
#   rf exec --token dev "cmd"  →  executed locally via the in-process agent
#
# Prerequisites: rf binary is available in $PATH

set -euo pipefail
cd "$(dirname "$0")/.."

RF="${RF_CLI:-rf}"

echo "=== Scenario 15: Dev Mode (Zero-Setup) ==="
echo ""

# --- Part 1: Start Dev Mode ---

echo "--- Part 1: What Is Dev Mode? ---"
echo ""
echo "  Dev mode starts a relay + agent in a single process."
echo "  No Docker. No config files. No key exchange."
echo "  One command → ready to use."
echo ""
echo "  Start dev mode:"
echo "    $ rf dev"
echo ""
echo "  Output:"
echo "    RavenFabric Dev Mode"
echo "    ===================="
echo "    Relay:  127.0.0.1:9090"
echo "    Token:  dev"
echo "    "
echo "    Usage:"
echo "      rf exec --token dev \"<command>\""
echo "    "
echo "    Press Ctrl+C to stop."
echo ""
sleep 6

# --- Part 2: Using Dev Mode ---

echo "--- Part 2: Execute Commands in Dev Mode ---"
echo ""

# In a real scenario, rf dev would be running in the background.
# Here we show the commands you'd run in a second terminal.

echo "[1] Basic command execution:"
echo "    $ rf exec --token dev 'hostname'"
echo ""
sleep 6

echo "[2] Multi-line scripts:"
echo "    $ rf exec --token dev 'uname -a && uptime && whoami'"
echo ""
sleep 6

echo "[3] File operations:"
echo "    $ rf exec --token dev 'echo \"hello\" > /tmp/test.txt && cat /tmp/test.txt'"
echo ""
sleep 6

echo "[4] Streaming output:"
echo "    $ rf exec --token dev --stream 'for i in 1 2 3 4 5; do echo \$i; sleep 1; done'"
echo ""
sleep 6

# --- Part 3: Dev Mode vs Full Setup Comparison ---

echo "--- Part 3: Dev Mode vs Full Setup ---"
echo ""
echo "  ┌──────────────────┬─────────────────────────┬────────────────────────┐"
echo "  │                  │ Dev Mode                │ Full Setup             │"
echo "  ├──────────────────┼─────────────────────────┼────────────────────────┤"
echo "  │ Start command    │ rf dev                  │ relay + agent + config │"
echo "  │ Config files     │ None                    │ raven.toml + policy    │"
echo "  │ Key management   │ Ephemeral (in-memory)   │ Persistent on disk     │"
echo "  │ Token            │ \"dev\" (fixed)            │ Custom per-agent       │"
echo "  │ Policy           │ Permissive (allow-all)  │ Deny-by-default        │"
echo "  │ Multi-agent      │ No (single agent)       │ Yes (unlimited)        │"
echo "  │ Production use   │ No                      │ Yes                    │"
echo "  │ Time to ready    │ < 1 second              │ Minutes                │"
echo "  └──────────────────┴─────────────────────────┴────────────────────────┘"
echo ""
sleep 6

# --- Part 4: Custom Options ---

echo "--- Part 4: Custom Port and Bind Address ---"
echo ""
echo "  Default:      rf dev                 → 127.0.0.1:9090"
echo "  Custom port:  rf dev --port 8080     → 127.0.0.1:8080"
echo "  Custom bind:  rf dev --bind 0.0.0.0  → 0.0.0.0:9090"
echo "  Both:         rf dev -p 8080 -b 0.0.0.0 → 0.0.0.0:8080"
echo ""
sleep 6

# --- Key Takeaways ---

echo "=== Key Takeaways ==="
echo ""
echo "  1. rf dev → one command, zero config, instant dev environment"
echo "  2. Same rf exec / rf forward commands work in dev mode"
echo "  3. Permissive policy — all commands allowed (dev only!)"
echo "  4. Ephemeral keys — no files written to disk"
echo "  5. Stop with Ctrl+C — clean shutdown, no orphans"
echo ""
echo "=== Scenario 15 Complete ==="
