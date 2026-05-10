#!/usr/bin/env bash
# Scenario 07: All Scenarios — Sequential Run
#
# Runs all desired-state scenarios and the full lifecycle test.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "============================================================"
echo "  RavenFabric Desired-State Demo — All Scenarios"
echo "============================================================"
echo ""
echo "Running all desired-state convergence tests: drift detection,"
echo "remediation, report mode, grains targeting, event triggers,"
echo "version constraints, and full lifecycle."
echo ""
echo "------------------------------------------------------------"

PASS=0
FAIL=0

TESTS=(
    "test_desired_state_drift_detection:Drift detection (5 resource types)"
    "test_desired_state_missing_package:Missing package detection"
    "test_desired_state_remediation:Auto-remediation (success)"
    "test_desired_state_remediation_failure:Remediation failure handling"
    "test_desired_state_report_mode:Report-only mode (no changes)"
    "test_desired_state_report_mode_converged:Report mode (all converged)"
    "test_desired_state_grains_match:Grains label match"
    "test_desired_state_grains_no_match:Grains label mismatch"
    "test_desired_state_grains_empty_matches_all:Empty selector matches all"
    "test_desired_state_grains_multi_label:Multi-label AND matching"
    "test_desired_state_timer_trigger:Timer trigger (converge action)"
    "test_desired_state_webhook_trigger:Webhook trigger (converge action)"
    "test_desired_state_trigger_lifecycle:Trigger register/remove lifecycle"
    "test_desired_state_version_exact:Version constraint (exact)"
    "test_desired_state_version_gte:Version constraint (>=)"
    "test_desired_state_version_lt:Version constraint (<)"
    "test_desired_state_full_lifecycle:Full lifecycle (E2E)"
    "test_desired_state_report_json:Report JSON serialization"
)

for entry in "${TESTS[@]}"; do
    test_name="${entry%%:*}"
    display="${entry#*:}"
    printf "  %-40s " "$display"

    if cargo test -p rf-integration-tests --test desired_state_showcase "$test_name" 2>&1 | grep -q "test result: ok"; then
        echo "PASS"
        PASS=$((PASS + 1))
    else
        echo "FAIL"
        FAIL=$((FAIL + 1))
    fi
done

echo ""
echo "------------------------------------------------------------"
echo "Results: $PASS passed, $FAIL failed (${#TESTS[@]} tests)"
echo ""

if [ "$FAIL" -eq 0 ]; then
    echo "All desired-state scenarios verified:"
    echo "  - Declarative YAML specs parsed correctly"
    echo "  - Drift detected across packages, files, services, sysctl"
    echo "  - Remediation mode fixes drift; report mode observes only"
    echo "  - Grains-based targeting matches agents by labels"
    echo "  - Event triggers invoke convergence (timer, webhook)"
    echo "  - Version constraints enforced (exact, >=, <)"
else
    echo "Some scenarios failed. Run individual scripts for details."
    exit 1
fi
