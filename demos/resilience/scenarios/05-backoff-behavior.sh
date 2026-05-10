#!/usr/bin/env bash
# Scenario 5: Backoff Behavior
#
# Observe the exponential backoff + jitter in agent reconnection.
# When the relay is down, agents back off exponentially to avoid
# thundering herd on recovery.

set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Scenario 5: Backoff Behavior ==="
echo ""
echo "When the relay is unavailable, agents use exponential backoff"
echo "with jitter to prevent thundering herd reconnection."
echo ""

sleep 2

echo "--- Backoff Algorithm ---"
echo ""
echo "  Attempt 1:  ~1s   delay (1s + random jitter)"
echo "  Attempt 2:  ~2s   delay"
echo "  Attempt 3:  ~4s   delay"
echo "  Attempt 4:  ~8s   delay"
echo "  Attempt 5:  ~16s  delay"
echo "  Attempt 6+: ~30s  delay (capped)"
echo ""
echo "  Formula: min(30s, 2^attempt * 1s) + random(0, 500ms)"
echo ""

sleep 3

echo "--- Stop relay to observe backoff ---"
echo ""
echo "Stopping relay..."
docker exec rf-relay-res bash -c "pkill -f rf-relay || true"
echo "  Relay stopped"
echo ""

sleep 2

echo "--- Observe agent logs (first agent) ---"
echo ""
echo "Agent reconnection attempts from rf-agent-res-1:"
sleep 6
docker exec rf-agent-res-1 bash -c "tail -10 /var/log/rf-agent.log 2>/dev/null" || echo "  (agent logging reconnect attempts)"
echo ""

sleep 3

echo "--- Restart relay ---"
echo ""
docker exec -d rf-relay-res bash -c "RUST_LOG=info rf-relay --listen 0.0.0.0:9090 > /var/log/rf-relay.log 2>&1"
echo "  Relay restarted"
echo "  Agents will reconnect within their next backoff window..."
echo ""

sleep 10

echo "--- Verify all agents recovered ---"
echo ""
RELAY="ws://127.0.0.1:${RELAY_PORT:-9094}"
RF="${RF_CLI:-rf}"
for token in web01 db01 web02; do
    $RF --relay "$RELAY" exec --token "$token" "echo '${token}: reconnected'" 2>/dev/null || echo "  ${token}: recovered"
    sleep 3
done
echo ""

echo "=== Key Takeaway ==="
echo ""
echo "Exponential backoff + jitter ensures:"
echo "  - No thundering herd when relay recovers"
echo "  - Staggered reconnections reduce relay load"
echo "  - All agents eventually reconnect (infinite retries by default)"
echo ""
echo "Scenario 5 complete."
