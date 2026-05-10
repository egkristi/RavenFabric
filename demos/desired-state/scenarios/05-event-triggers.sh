#!/usr/bin/env bash
# Scenario 05: Event Triggers
#
# Demonstrates event triggers that invoke desired-state convergence:
# Timer, Webhook, and trigger lifecycle management.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "============================================================"
echo "  Desired-State Demo — Event Triggers"
echo "============================================================"
echo ""
echo "Events (Timer, Webhook, Cron, FileWatch) can trigger"
echo "convergence checks automatically."
echo ""

PASS=0
FAIL=0

for test in test_desired_state_timer_trigger test_desired_state_webhook_trigger test_desired_state_trigger_lifecycle; do
    case "$test" in
        test_desired_state_timer_trigger)    display="Timer trigger → converge" ;;
        test_desired_state_webhook_trigger)  display="Webhook trigger → converge" ;;
        test_desired_state_trigger_lifecycle) display="Register/remove triggers" ;;
    esac
    printf "  %-35s " "$display"

    if cargo test -p rf-integration-tests --test desired_state_showcase "$test" 2>&1 | grep -q "test result: ok"; then
        echo "PASS"
        PASS=$((PASS + 1))
    else
        echo "FAIL"
        FAIL=$((FAIL + 1))
    fi
done

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
