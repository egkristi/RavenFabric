#!/usr/bin/env bash
# Scenario 02: Remediation
#
# Demonstrates automatic remediation of drifted resources.
# The engine detects drift, then calls the Remediator to fix each item.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "============================================================"
echo "  Desired-State Demo — Auto-Remediation"
echo "============================================================"
echo ""
echo "Mode: remediate"
echo "The convergence engine detects drift, then automatically"
echo "remediates each drifted resource back to desired state."
echo ""

PASS=0
FAIL=0

for test in test_desired_state_remediation test_desired_state_remediation_failure; do
    case "$test" in
        test_desired_state_remediation)        display="Successful remediation" ;;
        test_desired_state_remediation_failure) display="Remediation failure handling" ;;
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
