#!/usr/bin/env bash
# Scenario: Policy Denial (Kubernetes + CloudNativePG)
#
# Demonstrates deny-by-default policy enforcement in a Kubernetes
# environment. Updates the agent's ConfigMap with a restrictive policy,
# restarts the pod, then shows allowed PostgreSQL queries succeeding
# while dangerous commands are blocked.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9093}"
RF="${RF_CLI:-rf}"
NAMESPACE="ravenfabric"

echo "=== Policy Denial: Kubernetes + CloudNativePG Demo ==="
echo ""

# 1. Show current permissive policy
echo "[1] Current policy (ConfigMap — allows everything):"
kubectl get configmap rf-agent-policy -n "$NAMESPACE" -o jsonpath='{.data.policy\.yaml}'
echo ""
echo ""
sleep 2

# 2. Apply restrictive policy via ConfigMap — only safe read-only + psql SELECT allowed
echo "[2] Applying restrictive policy via ConfigMap..."
kubectl apply -f - <<'EOF'
apiVersion: v1
kind: ConfigMap
metadata:
  name: rf-agent-policy
  namespace: ravenfabric
data:
  policy.yaml: |
    spec:
      commands:
        allow:
          - pattern: "^hostname$"
          - pattern: "^uname.*"
          - pattern: "^cat /etc/os-release.*"
          - pattern: "^psql -c \"SELECT.*"
          - pattern: "^psql -c \"\\\\l\"$"
          - pattern: "^cat /tmp/rf-audit\\.jsonl.*"
        deny:
          - pattern: ".*rm.*-rf.*"
          - pattern: ".*curl.*"
          - pattern: ".*wget.*"
          - pattern: ".*shutdown.*"
          - pattern: ".*DROP.*"
          - pattern: ".*DELETE.*"
          - pattern: ".*TRUNCATE.*"
          - pattern: ".*ALTER.*"
          - pattern: ".*chmod.*"
          - pattern: ".*apt.*"
      resources:
        maxOutputBytes: 1048576
        timeoutSeconds: 30
EOF
echo "  ConfigMap updated."
echo ""

# 3. Restart agent pod to pick up new policy
echo "[3] Restarting rf-agent pod to load new policy..."
kubectl rollout restart deployment/rf-agent -n "$NAMESPACE"
kubectl rollout status deployment/rf-agent -n "$NAMESPACE" --timeout=60s
echo "  Agent restarted with restrictive policy."
echo ""
sleep 5

# 4. Test ALLOWED commands
echo "[4] ALLOWED — hostname:"
$RF --relay "$RELAY" exec --token cnpg 'hostname'
echo ""
sleep 6

echo "[5] ALLOWED — SELECT version():"
$RF --relay "$RELAY" exec --token cnpg 'psql -c "SELECT version();"'
echo ""
sleep 6

echo "[6] ALLOWED — list databases:"
$RF --relay "$RELAY" exec --token cnpg 'psql -c "\l"'
echo ""
sleep 6

# 5. Test DENIED commands
echo "[7] DENIED — DROP TABLE (destructive SQL):"
$RF --relay "$RELAY" exec --token cnpg 'psql -c "DROP TABLE IF EXISTS demo;"' 2>&1 || true
echo ""
sleep 6

echo "[8] DENIED — curl (network access):"
$RF --relay "$RELAY" exec --token cnpg 'curl http://example.com' 2>&1 || true
echo ""
sleep 6

echo "[9] DENIED — rm -rf /tmp (destructive):"
$RF --relay "$RELAY" exec --token cnpg 'rm -rf /tmp' 2>&1 || true
echo ""
sleep 6

echo "[10] DENIED — apt install (package management):"
$RF --relay "$RELAY" exec --token cnpg 'apt install -y nmap' 2>&1 || true
echo ""
sleep 6

# 6. Check audit log
echo "[11] Audit log — last 8 entries (showing denied commands):"
$RF --relay "$RELAY" exec --token cnpg 'cat /tmp/rf-audit.jsonl | tail -8'
echo ""
sleep 6

# 7. Restore permissive policy
echo "[12] Restoring permissive policy..."
kubectl apply -f - <<'EOF'
apiVersion: v1
kind: ConfigMap
metadata:
  name: rf-agent-policy
  namespace: ravenfabric
data:
  policy.yaml: |
    spec:
      commands:
        allow:
          - pattern: ".*"
      resources:
        maxOutputBytes: 104857600
        timeoutSeconds: 300
EOF
kubectl rollout restart deployment/rf-agent -n "$NAMESPACE"
kubectl rollout status deployment/rf-agent -n "$NAMESPACE" --timeout=60s
echo "  Permissive policy restored."
echo ""

echo "=== Policy Denial Demo Complete ==="
echo ""
echo "Key takeaways:"
echo "  - Policy stored in Kubernetes ConfigMap — GitOps-friendly"
echo "  - SELECT queries allowed, DROP/DELETE/TRUNCATE blocked"
echo "  - Same deny-by-default engine works in K8s as in Docker"
echo "  - Every denial is audited in structured JSON format"
echo "  - Policy changes require pod restart (hot-reload planned)"
