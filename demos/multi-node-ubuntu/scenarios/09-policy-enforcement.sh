#!/usr/bin/env bash
# Scenario 09: Policy Enforcement
#
# Demonstrates deny-by-default policy enforcement. Shows how the agent
# blocks dangerous commands while allowing safe ones, and how to apply
# custom restrictive policies with hot-reload.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 09: Policy Enforcement ==="
echo ""

# 1. Show current permissive policy
echo "[1] Current policy on agent 1 (permissive for demo):"
$RF --relay "$RELAY" exec --token agent1 'cat /etc/ravenfabric/policy.yaml'
echo ""
sleep 6

# 2. Apply a restrictive policy
echo "[2] Applying restrictive policy on agent 1:"
docker exec rf-agent-1 bash -c 'cat > /etc/ravenfabric/policy.yaml << "EOF"
spec:
  commands:
    allow:
      - pattern: "^hostname$"
      - pattern: "^uname.*"
      - pattern: "^cat /etc/.*"
      - pattern: "^ps.*"
      - pattern: "^uptime$"
      - pattern: "^df.*"
      - pattern: "^echo.*"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*shutdown.*"
      - pattern: ".*reboot.*"
      - pattern: ".*mkfs.*"
      - pattern: ".*dd.*if=.*"
      - pattern: ".*curl.*"
      - pattern: ".*wget.*"
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30
EOF'
echo "  Restrictive policy applied."
echo ""

# 3. Restart agent to load new policy
echo "[3] Restarting agent 1 with new policy..."
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
echo "  Agent restarted."
echo ""

# 4. Test allowed commands
echo "[4] Allowed command (hostname):"
$RF --relay "$RELAY" exec --token agent1 'hostname'
echo ""
sleep 6

echo "[5] Allowed command (uname -a):"
$RF --relay "$RELAY" exec --token agent1 'uname -a'
echo ""
sleep 6

# 5. Test denied commands
echo "[6] Denied command (rm -rf /):"
$RF --relay "$RELAY" exec --token agent1 'rm -rf /' 2>&1 || true
echo ""
sleep 6

echo "[7] Denied command (curl):"
$RF --relay "$RELAY" exec --token agent1 'curl http://example.com' 2>&1 || true
echo ""
sleep 6

echo "[8] Denied command (shutdown):"
$RF --relay "$RELAY" exec --token agent1 'shutdown -h now' 2>&1 || true
echo ""
sleep 6

# 6. Restore permissive policy
echo "[9] Restoring permissive policy on agent 1..."
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

echo "=== Scenario 09 Complete ==="
