#!/usr/bin/env bash
# Scenario 06: Version Constraints
#
# Demonstrates package version constraint matching:
# exact versions, >=, <, and boundary conditions.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "============================================================"
echo "  Desired-State Demo — Version Constraints"
echo "============================================================"
echo ""
echo "Package state declarations support version constraints:"
echo "  exact (3.1.0), >= (>=8.0.0), < (<2.0.0)"
echo ""

PASS=0
FAIL=0

for test in test_desired_state_version_exact test_desired_state_version_gte test_desired_state_version_lt; do
    case "$test" in
        test_desired_state_version_exact) display="Exact version match" ;;
        test_desired_state_version_gte)   display=">= constraint" ;;
        test_desired_state_version_lt)    display="< constraint" ;;
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
