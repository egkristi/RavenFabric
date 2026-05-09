#!/usr/bin/env bash
# Scenario: Audit Trail (Kubernetes + CloudNativePG)
#
# Demonstrates structured audit logging in a Kubernetes environment.
# Executes SQL queries and system commands, then inspects the audit log
# on the agent pod showing timestamped entries for every action.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9093}"
RF="${RF_CLI:-rf}"
NAMESPACE="ravenfabric"

echo "=== Audit Trail: Kubernetes + CloudNativePG Demo ==="
echo ""

# 1. Execute commands to generate audit entries
echo "[1] Executing commands to generate audit trail..."
$RF --relay "$RELAY" exec --token cnpg 'hostname' > /dev/null 2>&1 || true
sleep 6
$RF --relay "$RELAY" exec --token cnpg 'psql -c "SELECT version();"' > /dev/null 2>&1 || true
sleep 6
$RF --relay "$RELAY" exec --token cnpg 'uname -a' > /dev/null 2>&1 || true
sleep 6
echo "  3 commands executed."
echo ""

# 2. View audit log from the K8s pod
echo "[2] Audit log on rf-agent pod (last 5 entries):"
$RF --relay "$RELAY" exec --token cnpg 'cat /tmp/rf-audit.jsonl | tail -5'
echo ""
sleep 6

# 3. Count total audit entries
echo "[3] Total audit entries:"
$RF --relay "$RELAY" exec --token cnpg 'wc -l < /tmp/rf-audit.jsonl'
echo ""
sleep 6

# 4. Show audit log via kubectl (alternative path)
echo "[4] Audit log via kubectl (direct pod access):"
POD=$(kubectl get pod -n "$NAMESPACE" -l app=rf-agent -o jsonpath='{.items[0].metadata.name}' 2>/dev/null)
if [[ -n "$POD" ]]; then
    kubectl exec -n "$NAMESPACE" "$POD" -c rf-agent -- tail -3 /tmp/rf-audit.jsonl 2>/dev/null || echo "  (kubectl exec not available)"
else
    echo "  (pod not found)"
fi
echo ""

# 5. Execute a SQL query and show the resulting audit entry
echo "[5] Execute a traced SQL query:"
$RF --relay "$RELAY" exec --token cnpg 'psql -c "SELECT current_database(), current_user, now();"'
echo ""
sleep 6

echo "[6] Latest audit entry (for the SQL query above):"
$RF --relay "$RELAY" exec --token cnpg 'tail -1 /tmp/rf-audit.jsonl'
echo ""
sleep 6

echo "=== Audit Trail Demo Complete ==="
echo ""
echo "Key takeaways:"
echo "  - Every SQL query and system command is audited"
echo "  - Audit log accessible via RavenFabric tunnel or kubectl"
echo "  - Structured JSON format with timestamp, command, decision, duration"
echo "  - Append-only — entries persist across pod restarts (if volume-mounted)"
echo "  - K8s audit log at /tmp/rf-audit.jsonl (configurable via --audit-path)"
