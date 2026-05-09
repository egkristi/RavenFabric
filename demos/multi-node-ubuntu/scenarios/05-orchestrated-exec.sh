#!/usr/bin/env bash
# Scenario 05: Orchestrated Multi-Agent Execution
#
# Demonstrates executing commands across multiple agents using a playbook.
# Supports parallel, sequential, rolling, and canary rollout strategies
# with automatic rollback on failure.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 05: Orchestrated Multi-Agent Execution ==="
echo ""

# 1. Run the parallel update playbook
echo "[1] Running parallel update playbook (both agents simultaneously):"
$RF --relay "$RELAY" playbook --token agent1 scenarios/playbooks/parallel-update.yaml
echo ""
sleep 6

# 2. Run the sequential health check playbook
echo "[2] Running sequential health check playbook (one agent at a time):"
$RF --relay "$RELAY" playbook --token agent1 scenarios/playbooks/sequential-healthcheck.yaml
echo ""
sleep 6

# 3. Run the rolling deploy playbook
echo "[3] Running canary deploy playbook (canary first, then remaining):"
$RF --relay "$RELAY" playbook --token agent1 scenarios/playbooks/canary-deploy.yaml
echo ""

echo "=== Scenario 05 Complete ==="
