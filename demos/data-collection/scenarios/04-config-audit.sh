#!/usr/bin/env bash
# Scenario 4: Configuration Audit
#
# Collects and compares configuration files across the fleet
# to detect drift, verify consistency, and inventory settings.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9092}"
RF="${RF_CLI:-rf}"
AGENTS=("collector" "webserver" "database")

echo "=== Scenario 4: Configuration Audit ==="
echo ""

# 1. Application configs
echo "[1] Application configurations:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'cat /opt/app/config.yaml 2>/dev/null || echo "No config found"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 2. RavenFabric policy files — verify all agents have the same policy
echo "[2] Policy file checksums (should match across fleet):"
for token in "${AGENTS[@]}"; do
    echo -n "  $token: "
    $RF --relay "$RELAY" exec --token "$token" 'sha256sum /etc/ravenfabric/policy.yaml' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 3. OS configuration
echo "[3] OS release info:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'cat /etc/os-release | grep -E "^(NAME|VERSION|ID)="' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 4. Hostname and timezone
echo "[4] Hostname and timezone:"
for token in "${AGENTS[@]}"; do
    echo -n "  $token: hostname=$(
        $RF --relay "$RELAY" exec --token "$token" 'hostname' 2>/dev/null | grep -v "^2" | tr -d '\n'
    ), tz="
    $RF --relay "$RELAY" exec --token "$token" 'date +%Z' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 5. Installed packages count
echo "[5] Installed packages:"
for token in "${AGENTS[@]}"; do
    echo -n "  $token: "
    $RF --relay "$RELAY" exec --token "$token" 'echo "$(ls /usr/bin /usr/sbin 2>/dev/null | wc -l) binaries in PATH"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 6. Web server specific config
echo "[6] Web server configuration:"
$RF --relay "$RELAY" exec --token webserver 'cat /etc/ravenfabric/nginx.conf 2>/dev/null || echo "No nginx config"' 2>/dev/null | grep -v "^2"
echo ""
sleep 6

# 7. Config file inventory
echo "[7] Configuration file inventory per agent:"
for token in "${AGENTS[@]}"; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'find /etc/ravenfabric /opt/app -type f 2>/dev/null | sort' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

echo "=== Scenario 4 Complete ==="
