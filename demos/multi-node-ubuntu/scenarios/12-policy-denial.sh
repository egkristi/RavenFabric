#!/usr/bin/env bash
# Scenario 12: Policy Denial
#
# Demonstrates deny-by-default policy in action. Applies a restrictive
# policy, shows allowed commands succeeding and dangerous commands being
# denied, then inspects the audit log for denial entries.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 12: Policy Denial ==="
echo ""

# 1. Show current permissive policy
echo "[1] Current policy on agent 1 (permissive — allows everything):"
$RF --relay "$RELAY" exec --token agent1 'cat /etc/ravenfabric/policy.yaml'
echo ""
sleep 6

# 2. Apply restrictive policy — only safe read-only commands allowed
RESTRICTIVE_POLICY='spec:
  commands:
    allow:
      - pattern: "^hostname$"
      - pattern: "^uname.*"
      - pattern: "^cat /etc/os-release.*"
      - pattern: "^uptime$"
      - pattern: "^whoami$"
      - pattern: "^date$"
      - pattern: "^cat /etc/ravenfabric/policy\\.yaml$"
      - pattern: "^cat /var/log/rf-audit\\.jsonl.*"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*shutdown.*"
      - pattern: ".*reboot.*"
      - pattern: ".*mkfs.*"
      - pattern: ".*dd.*if=.*"
      - pattern: ".*curl.*"
      - pattern: ".*wget.*"
      - pattern: ".*chmod.*"
      - pattern: ".*chown.*"
      - pattern: ".*apt.*"
      - pattern: ".*pip.*"
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30'

echo "[2] Applying restrictive policy to agent 1..."
docker exec rf-agent-1 bash -c "cat > /etc/ravenfabric/policy.yaml << 'EOF'
${RESTRICTIVE_POLICY}
EOF"
echo "  Policy written."
echo ""

# 3. Restart agent to load new policy
echo "[3] Restarting agent 1 to load new policy..."
RELAY_IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' rf-relay)
docker exec rf-agent-1 bash -c 'pkill rf-agent || true'
sleep 1
docker exec -d rf-agent-1 bash -c "RUST_LOG=info rf-agent \
    --relay ws://${RELAY_IP}:9090 --id rf-agent-1 --token agent1 \
    --policy-path /etc/ravenfabric/policy.yaml \
    --audit-path /var/log/rf-audit.jsonl \
    --key-path /etc/ravenfabric/agent.key \
    > /var/log/rf-agent.log 2>&1"
sleep 3
echo "  Agent restarted with restrictive policy."
echo ""

# 4. Show the new policy
echo "[4] New restrictive policy:"
$RF --relay "$RELAY" exec --token agent1 'cat /etc/ravenfabric/policy.yaml'
echo ""
sleep 6

# 5. Test ALLOWED commands
echo "[5] ALLOWED — hostname:"
$RF --relay "$RELAY" exec --token agent1 'hostname'
echo ""
sleep 6

echo "[6] ALLOWED — uname -a:"
$RF --relay "$RELAY" exec --token agent1 'uname -a'
echo ""
sleep 6

echo "[7] ALLOWED — uptime:"
$RF --relay "$RELAY" exec --token agent1 'uptime'
echo ""
sleep 6

# 6. Test DENIED commands
echo "[8] DENIED — rm -rf / (destructive):"
$RF --relay "$RELAY" exec --token agent1 'rm -rf /' 2>&1 || true
echo ""
sleep 6

echo "[9] DENIED — curl (network access):"
$RF --relay "$RELAY" exec --token agent1 'curl http://example.com' 2>&1 || true
echo ""
sleep 6

echo "[10] DENIED — apt install (package management):"
$RF --relay "$RELAY" exec --token agent1 'apt install -y nmap' 2>&1 || true
echo ""
sleep 6

echo "[11] DENIED — shutdown (system control):"
$RF --relay "$RELAY" exec --token agent1 'shutdown -h now' 2>&1 || true
echo ""
sleep 6

echo "[12] DENIED — chmod 777 (permission change):"
$RF --relay "$RELAY" exec --token agent1 'chmod 777 /etc/passwd' 2>&1 || true
echo ""
sleep 6

# 7. Check audit log for denial entries
echo "[13] Audit log — last 10 entries (showing denied commands):"
$RF --relay "$RELAY" exec --token agent1 'cat /var/log/rf-audit.jsonl | tail -10'
echo ""
sleep 6

# 8. Restore permissive policy
echo "[14] Restoring permissive policy..."
docker exec rf-agent-1 bash -c 'cat > /etc/ravenfabric/policy.yaml << "EOF"
spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxOutputBytes: 104857600
    timeoutSeconds: 300
EOF'
docker exec rf-agent-1 bash -c 'pkill rf-agent || true'
sleep 1
docker exec -d rf-agent-1 bash -c "RUST_LOG=info rf-agent \
    --relay ws://${RELAY_IP}:9090 --id rf-agent-1 --token agent1 \
    --policy-path /etc/ravenfabric/policy.yaml \
    --audit-path /var/log/rf-audit.jsonl \
    --key-path /etc/ravenfabric/agent.key \
    > /var/log/rf-agent.log 2>&1"
sleep 2
echo "  Permissive policy restored."
echo ""

echo "=== Scenario 12 Complete ==="
echo ""
echo "Key takeaways:"
echo "  - Deny-by-default: commands not matching any allow pattern are blocked"
echo "  - Every denial is logged in the structured audit log"
echo "  - Policy changes take effect on agent restart"
echo "  - The relay never sees command content (end-to-end encrypted)"
