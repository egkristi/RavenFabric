#!/usr/bin/env bash
# Scenario 6: Security Scan
#
# Performs a lightweight security audit across the fleet:
# file permissions, running processes, open ports, user accounts.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9092}"
RF="${RF_CLI:-rf}"
AGENTS=("collector" "webserver" "database")

echo "=== Scenario 6: Security Scan ==="
echo ""

# 1. Running user context
echo "[1] Agent execution context:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'whoami && id' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 2. World-writable files in /etc
echo "[2] World-writable files in /etc (should be none):"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'find /etc -perm -o+w -type f 2>/dev/null | head -10 || echo "  None found"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 3. SUID/SGID binaries
echo "[3] SUID binaries:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'find /usr -perm -4000 -type f 2>/dev/null | head -10 || echo "  None found"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 4. User accounts
echo "[4] User accounts with login shell:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'cat /etc/passwd | grep -v nologin | grep -v false' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 5. Listening services
echo "[5] Listening services:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'ss -tlnp 2>/dev/null || echo "  ss not available"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 6. Key file permissions
echo "[6] Critical file permissions:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'ls -la /etc/ravenfabric/ 2>/dev/null && stat -c "%a %n" /etc/ravenfabric/agent.key 2>/dev/null || echo "  Key file not found"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 7. Environment variables (filtered for secrets)
echo "[7] Environment snapshot (checking for leaked secrets):"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'env | grep -iE "^(HOME|USER|PATH|HOSTNAME|RUST_LOG)=" | sort' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

echo "=== Scenario 6 Complete ==="
