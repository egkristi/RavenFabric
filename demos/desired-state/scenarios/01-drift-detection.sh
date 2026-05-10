#!/usr/bin/env bash
# Scenario 01: Drift Detection
#
# Demonstrates detecting drift across all 4 resource types:
# packages, files, services, and sysctl parameters.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "============================================================"
echo "  Desired-State Demo — Drift Detection"
echo "============================================================"
echo ""
echo "Simulating a web server with multiple configuration drifts:"
echo "  - nginx version too old (1.22.0 < required >=1.24.0)"
echo "  - telnet installed (should be absent)"
echo "  - nginx.conf has wrong content"
echo "  - nginx service stopped (should be running)"
echo "  - ip_forward sysctl set to 1 (should be 0)"
echo ""

if cargo test -p rf-integration-tests --test desired_state_showcase test_desired_state_drift_detection -- --nocapture 2>&1 | tail -20; then
    echo ""
    echo "PASS: All 5 drift items correctly detected."
else
    echo "FAIL"
    exit 1
fi
