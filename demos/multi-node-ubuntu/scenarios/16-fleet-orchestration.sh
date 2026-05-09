#!/usr/bin/env bash
# Scenario 16: Fleet Orchestration
#
# Demonstrates multi-agent orchestration with playbooks. Execute commands
# across a fleet of agents using parallel, sequential, and canary rollout
# strategies — with automatic rollback on failure.
#
# Architecture:
#   CLI → playbook.yaml → Orchestrator → [agent1, agent2, ...] → results
#
# Strategies:
#   parallel:   all agents simultaneously
#   sequential: one at a time, stop on failure
#   rolling:    batches (e.g. 25% at a time)
#   canary:     test on N agents first, then the rest
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 16: Fleet Orchestration ==="
echo ""

# --- Part 1: Fleet Inventory ---

echo "--- Part 1: Fleet Inventory ---"
echo ""
echo "  Collect system info from all agents in the fleet."
echo ""

echo "[1] Hostname inventory:"
for token in agent1 agent2; do
    echo -n "  $token: "
    $RF --relay "$RELAY" exec --token "$token" 'hostname' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

echo "[2] OS and kernel versions:"
for token in agent1 agent2; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" \
        'source /etc/os-release && echo "  OS: $PRETTY_NAME" && echo "  Kernel: $(uname -r)"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# --- Part 2: Parallel Execution ---

echo "--- Part 2: Parallel Strategy ---"
echo ""
echo "  Execute on all agents simultaneously."
echo ""

echo "[3] Show parallel playbook:"
echo "  ---"
cat scenarios/playbooks/parallel-update.yaml | sed 's/^/  /'
echo "  ---"
echo ""
sleep 6

echo "[4] Run parallel update:"
$RF --relay "$RELAY" playbook --token agent1 scenarios/playbooks/parallel-update.yaml
echo ""
sleep 6

# --- Part 3: Canary Deployment ---

echo "--- Part 3: Canary Strategy ---"
echo ""
echo "  Test on 1 agent first. If it succeeds, roll out to the rest."
echo "  If it fails, automatically rollback."
echo ""

echo "[5] Show canary playbook:"
echo "  ---"
cat scenarios/playbooks/canary-deploy.yaml | sed 's/^/  /'
echo "  ---"
echo ""
sleep 6

echo "[6] Run canary deploy:"
$RF --relay "$RELAY" playbook --token agent1 scenarios/playbooks/canary-deploy.yaml
echo ""
sleep 6

echo "[7] Verify deployment:"
for token in agent1 agent2; do
    echo -n "  $token version: "
    $RF --relay "$RELAY" exec --token "$token" 'cat /opt/app/version.txt 2>/dev/null || echo "not deployed"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# --- Part 4: Strategy Comparison ---

echo "--- Part 4: Strategy Comparison ---"
echo ""
echo "  ┌─────────────┬──────────────────────────┬──────────────────────────────────┐"
echo "  │ Strategy    │ Behavior                 │ Best For                         │"
echo "  ├─────────────┼──────────────────────────┼──────────────────────────────────┤"
echo "  │ parallel    │ All agents at once       │ Status checks, non-destructive   │"
echo "  │ sequential  │ One at a time            │ Ordered migrations, dependencies │"
echo "  │ rolling     │ Batches (e.g. 25%)       │ Large fleets, gradual rollout    │"
echo "  │ canary      │ Test N, then remainder   │ Risky deploys, validation first  │"
echo "  └─────────────┴──────────────────────────┴──────────────────────────────────┘"
echo ""
sleep 6

# --- Key Takeaways ---

echo "=== Key Takeaways ==="
echo ""
echo "  1. Playbooks define command, targets, strategy, and rollback"
echo "  2. Four strategies: parallel, sequential, rolling, canary"
echo "  3. Automatic rollback on failure (canary + rollback policy)"
echo "  4. Same YAML format for 2 agents or 2,000"
echo "  5. Every execution is audited per-agent"
echo ""
echo "=== Scenario 16 Complete ==="
