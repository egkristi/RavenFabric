#!/usr/bin/env bash
# Scenario 04: Grains-Based Targeting
#
# Demonstrates using system grains (facts) to decide which
# agents should apply a desired-state spec.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "============================================================"
echo "  Desired-State Demo — Grains-Based Targeting"
echo "============================================================"
echo ""
echo "Agents collect grains (OS, arch, role, env) and match them"
echo "against the spec's target label selector."
echo ""

PASS=0
FAIL=0

for test in test_desired_state_grains_match test_desired_state_grains_no_match test_desired_state_grains_empty_matches_all test_desired_state_grains_multi_label; do
    case "$test" in
        test_desired_state_grains_match)            display="Labels match target" ;;
        test_desired_state_grains_no_match)         display="Labels do not match" ;;
        test_desired_state_grains_empty_matches_all) display="Empty selector matches all" ;;
        test_desired_state_grains_multi_label)       display="Multi-label AND matching" ;;
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
