#!/usr/bin/env bash
# Scenario 03: Background Execution
#
# Demonstrates fire-and-forget command execution. The command starts on the
# agent and a job ID is returned immediately. You can query the job status
# later or wait for completion.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 03: Background Execution ==="
echo ""

# 1. Start a background job
echo "[1] Starting background job on agent 1 (sleeps 5 seconds then writes result):"
$RF --relay "$RELAY" exec --background --token agent1 \
    'sleep 5 && echo "Background job completed at $(date)" > /tmp/bg-result.txt && echo done'
echo ""
sleep 6

# 2. Check the result (after the job has completed)
echo "[2] Checking background job result:"
$RF --relay "$RELAY" exec --token agent1 'cat /tmp/bg-result.txt 2>/dev/null || echo "Job still running..."'
echo ""
sleep 6

# 3. Start a long-running background process
echo "[3] Starting a background service (simulated):"
$RF --relay "$RELAY" exec --background --token agent2 \
    'nohup bash -c "for i in $(seq 1 100); do echo \"tick $i\" >> /tmp/bg-service.log; sleep 1; done" &'
echo ""
sleep 6

# 4. Check the service is running
echo "[4] Verifying background service output:"
$RF --relay "$RELAY" exec --token agent2 'tail -5 /tmp/bg-service.log 2>/dev/null || echo "Starting..."'
echo ""

echo "=== Scenario 03 Complete ==="
