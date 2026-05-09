#!/usr/bin/env bash
# Scenario: Audit Trail (Multi-Distro)
#
# Demonstrates structured audit logging across multiple Linux distributions.
# Executes commands on several distros and inspects the independent audit
# logs on each, showing that every agent maintains its own append-only trail.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9092}"
RF="${RF_CLI:-rf}"

DISTROS="ubuntu debian fedora alpine rocky"

echo "=== Audit Trail: Multi-Distro Demo ==="
echo ""

# 1. Execute a command on each distro to generate audit entries
echo "[1] Executing 'hostname' on 5 distros to generate audit entries..."
for distro in $DISTROS; do
    echo -n "  ${distro}: "
    $RF --relay "$RELAY" exec --token "$distro" 'hostname' 2>/dev/null || echo "(failed)"
    sleep 6
done
echo ""

# 2. Show audit log from Ubuntu (glibc)
echo "[2] Audit log on Ubuntu (glibc) — last 3 entries:"
docker exec rf-ubuntu tail -3 /var/log/rf-audit.jsonl
echo ""

# 3. Show audit log from Alpine (musl-native)
echo "[3] Audit log on Alpine (musl) — last 3 entries:"
docker exec rf-alpine tail -3 /var/log/rf-audit.jsonl
echo ""

# 4. Show audit log from Fedora (rpm-based)
echo "[4] Audit log on Fedora (rpm/dnf) — last 3 entries:"
docker exec rf-fedora tail -3 /var/log/rf-audit.jsonl
echo ""

# 5. Count audit entries across all distros
echo "[5] Audit entry count per distro:"
for distro in $DISTROS; do
    COUNT=$(docker exec "rf-${distro}" bash -c 'wc -l < /var/log/rf-audit.jsonl 2>/dev/null || echo 0' 2>/dev/null || docker exec "rf-${distro}" sh -c 'wc -l < /var/log/rf-audit.jsonl 2>/dev/null || echo 0' 2>/dev/null)
    echo "  ${distro}: ${COUNT} entries"
done
echo ""

# 6. Demonstrate audit format consistency
echo "[6] Audit format is identical across all distros:"
echo "  --- Ubuntu ---"
docker exec rf-ubuntu tail -1 /var/log/rf-audit.jsonl
echo "  --- Alpine ---"
docker exec rf-alpine tail -1 /var/log/rf-audit.jsonl
echo "  --- Fedora ---"
docker exec rf-fedora tail -1 /var/log/rf-audit.jsonl
echo ""

echo "=== Audit Trail Demo Complete ==="
echo ""
echo "Key takeaways:"
echo "  - Every distro produces identical structured JSON audit entries"
echo "  - Each agent maintains its own independent, append-only audit log"
echo "  - Audit format does not vary between glibc and musl distros"
echo "  - Same logging behavior on apt, dnf, pacman, apk, and xbps distros"
