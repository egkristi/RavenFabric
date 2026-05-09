#!/usr/bin/env bash
# Scenario: Policy Denial (Multi-Distro)
#
# Demonstrates deny-by-default policy enforcement across multiple Linux
# distributions. Applies a restrictive policy to the Ubuntu agent, shows
# allowed and denied commands, then verifies the same policy works
# identically on Alpine (musl-native).
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9092}"
RF="${RF_CLI:-rf}"

RESTRICTIVE_POLICY='spec:
  commands:
    allow:
      - pattern: "^hostname$"
      - pattern: "^uname.*"
      - pattern: "^cat /etc/os-release.*"
      - pattern: "^uptime$"
      - pattern: "^cat /var/log/rf-audit\\.jsonl.*"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*curl.*"
      - pattern: ".*wget.*"
      - pattern: ".*shutdown.*"
      - pattern: ".*apt.*"
      - pattern: ".*apk.*"
      - pattern: ".*dnf.*"
      - pattern: ".*chmod.*"
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30'

PERMISSIVE_POLICY='spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxOutputBytes: 104857600
    timeoutSeconds: 300'

apply_policy() {
    local container="$1"
    local shell="$2"
    local policy="$3"
    local relay_ip="$4"
    local token="$5"

    docker exec "$container" "$shell" -c "cat > /etc/ravenfabric/policy.yaml << 'PEOF'
${policy}
PEOF"
    docker exec "$container" "$shell" -c 'pkill rf-agent || true'
    sleep 1
    docker exec -d "$container" "$shell" -c "RUST_LOG=info rf-agent \
        --relay ws://${relay_ip}:9090 --id ${container} --token ${token} \
        --policy-path /etc/ravenfabric/policy.yaml \
        --audit-path /var/log/rf-audit.jsonl \
        --key-path /etc/ravenfabric/agent.key \
        > /var/log/rf-agent.log 2>&1"
    sleep 3
}

RELAY_IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' rf-relay-ubuntu)

echo "=== Policy Denial: Multi-Distro Demo ==="
echo ""

# --- Ubuntu (glibc, apt) ---
echo "--- Ubuntu (glibc, apt-based) ---"
echo ""

echo "[1] Applying restrictive policy to Ubuntu agent..."
apply_policy "rf-ubuntu" "bash" "$RESTRICTIVE_POLICY" "$RELAY_IP" "ubuntu"
echo "  Done."
echo ""

echo "[2] ALLOWED — hostname:"
$RF --relay "$RELAY" exec --token ubuntu 'hostname'
echo ""
sleep 6

echo "[3] ALLOWED — cat /etc/os-release (first 2 lines):"
$RF --relay "$RELAY" exec --token ubuntu 'cat /etc/os-release | head -2'
echo ""
sleep 6

echo "[4] DENIED — rm -rf /tmp (destructive):"
$RF --relay "$RELAY" exec --token ubuntu 'rm -rf /tmp' 2>&1 || true
echo ""
sleep 6

echo "[5] DENIED — apt install nmap (package management):"
$RF --relay "$RELAY" exec --token ubuntu 'apt install -y nmap' 2>&1 || true
echo ""
sleep 6

echo "[6] DENIED — curl (network access):"
$RF --relay "$RELAY" exec --token ubuntu 'curl http://example.com' 2>&1 || true
echo ""
sleep 6

echo "[7] Audit log (Ubuntu) — last 5 entries:"
$RF --relay "$RELAY" exec --token ubuntu 'cat /var/log/rf-audit.jsonl | tail -5'
echo ""
sleep 6

echo "[8] Restoring permissive policy on Ubuntu..."
apply_policy "rf-ubuntu" "bash" "$PERMISSIVE_POLICY" "$RELAY_IP" "ubuntu"
echo "  Done."
echo ""

# --- Alpine (musl-native, apk) ---
echo "--- Alpine (musl-native, apk-based) ---"
echo ""

echo "[9] Applying restrictive policy to Alpine agent..."
apply_policy "rf-alpine" "sh" "$RESTRICTIVE_POLICY" "$RELAY_IP" "alpine"
echo "  Done."
echo ""

echo "[10] ALLOWED — hostname:"
$RF --relay "$RELAY" exec --token alpine 'hostname'
echo ""
sleep 6

echo "[11] ALLOWED — uname -a:"
$RF --relay "$RELAY" exec --token alpine 'uname -a'
echo ""
sleep 6

echo "[12] DENIED — apk add nmap (package management):"
$RF --relay "$RELAY" exec --token alpine 'apk add nmap' 2>&1 || true
echo ""
sleep 6

echo "[13] DENIED — wget (network access):"
$RF --relay "$RELAY" exec --token alpine 'wget http://example.com' 2>&1 || true
echo ""
sleep 6

echo "[14] Restoring permissive policy on Alpine..."
apply_policy "rf-alpine" "sh" "$PERMISSIVE_POLICY" "$RELAY_IP" "alpine"
echo "  Done."
echo ""

echo "=== Policy Denial Demo Complete ==="
echo ""
echo "Key takeaways:"
echo "  - Same policy engine works on glibc (Ubuntu) and musl (Alpine)"
echo "  - Package managers, network tools, and destructive commands all blocked"
echo "  - Every denial produces a structured audit entry"
echo "  - Policy is enforced agent-side — the relay never sees command content"
