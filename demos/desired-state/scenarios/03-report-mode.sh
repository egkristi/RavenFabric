#!/usr/bin/env bash
# Scenario 03: Report-Only Mode
#
# Demonstrates report mode: drift is detected and reported,
# but no changes are made to the system.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "============================================================"
echo "  Desired-State Demo — Report-Only Mode"
echo "============================================================"
echo ""
echo "Mode: report"
echo "Detects drift without making any changes — ideal for"
echo "monitoring and compliance auditing."
echo ""

PASS=0
FAIL=0

for test in test_desired_state_report_mode test_desired_state_report_mode_converged; do
    case "$test" in
        test_desired_state_report_mode)           display="Report mode (drift detected)" ;;
        test_desired_state_report_mode_converged)  display="Report mode (all converged)" ;;
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
