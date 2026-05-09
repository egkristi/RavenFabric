#!/usr/bin/env bash
# Dev Mode (Zero-Setup) — Kubernetes + CloudNativePG
#
# Demonstrates how dev mode provides a quick way to test rf commands
# locally before deploying to Kubernetes. Prototype queries and
# workflows without a running cluster.
#
# Use case: develop rf exec commands locally, then run them
# against the real K8s environment with the same syntax.
#
# Prerequisites: rf binary available in $PATH

set -euo pipefail
cd "$(dirname "$0")/.."

RF="${RF_CLI:-rf}"
RELAY="${RF_RELAY:-ws://127.0.0.1:9093}"

echo "=== Dev Mode (Zero-Setup) — Kubernetes Context ==="
echo ""

# --- Part 1: Local Prototyping ---

echo "--- Part 1: Prototype Locally Before Deploying ---"
echo ""
echo "  Dev mode lets you test rf workflows on your laptop"
echo "  before running them against a real Kubernetes cluster."
echo ""
echo "  Local prototyping:"
echo "    $ rf dev"
echo "    $ rf exec --token dev 'echo SELECT 1 | psql ...'"
echo ""
echo "  Same command in production:"
echo "    $ rf --relay ws://relay.example.com exec --token cnpg 'psql ...'"
echo ""
echo "  Identical syntax — just change relay and token."
echo ""
sleep 6

# --- Part 2: Dev Mode vs K8s Demo ---

echo "--- Part 2: Dev Mode vs Full K8s Demo ---"
echo ""
echo "  ┌────────────────────┬─────────────────────────┬──────────────────────────────┐"
echo "  │                    │ Dev Mode                │ K8s + CNPG Demo              │"
echo "  ├────────────────────┼─────────────────────────┼──────────────────────────────┤"
echo "  │ Prerequisites      │ rf binary only          │ kind + kubectl + helm + CNPG │"
echo "  │ Setup time         │ < 1 second              │ 3-5 minutes                  │"
echo "  │ PostgreSQL cluster │ No (mock locally)       │ Yes (2-node CNPG cluster)    │"
echo "  │ Network policies   │ N/A (local)             │ Calico + Gatekeeper          │"
echo "  │ Port forwarding    │ Local only              │ Pod → local machine          │"
echo "  │ Audit logging      │ Yes (in-memory)         │ Yes (pod filesystem)         │"
echo "  │ Use case           │ Develop rf commands     │ Validate full K8s workflow   │"
echo "  └────────────────────┴─────────────────────────┴──────────────────────────────┘"
echo ""
sleep 6

# --- Part 3: Workflow ---

echo "--- Part 3: Recommended Workflow ---"
echo ""
echo "  1. Start dev mode:           rf dev"
echo "  2. Prototype commands:       rf exec --token dev 'your command'"
echo "  3. Test streaming:           rf exec --token dev --stream 'long task'"
echo "  4. Test port forwarding:     rf forward --token dev -L :5432 -R :5432"
echo "  5. When ready → deploy to K8s and use real relay + token"
echo ""
sleep 6

# --- Key Takeaways ---

echo "=== Key Takeaways ==="
echo ""
echo "  1. Dev mode = instant local environment for prototyping"
echo "  2. Same rf exec syntax works locally and against K8s"
echo "  3. Develop fast, deploy confidently"
echo "  4. No cluster required until you're ready to test integration"
echo ""
echo "=== Dev Mode Scenario Complete ==="
