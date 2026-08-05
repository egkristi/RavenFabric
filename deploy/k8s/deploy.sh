#!/bin/bash
# RavenFabric K8s deploy script
# Usage: ./deploy/k8s/deploy.sh [version]
#   version: RavenFabric version (default: 1.0.0-rc.6)

set -euo pipefail

NAMESPACE="${NAMESPACE:-ravenfabric}"
VERSION="${1:-1.0.0-rc.6}"

echo "=== RavenFabric K8s Deploy v${VERSION} ==="
echo "Namespace: ${NAMESPACE}"

# Clean up existing pods
echo ""
echo "=== Cleaning namespace ==="
kubectl delete pod -n "${NAMESPACE}" --all --force --grace-period=0 2>/dev/null || true
kubectl delete svc -n "${NAMESPACE}" relay-svc 2>/dev/null || true
sleep 3

# Deploy with version override
echo ""
echo "=== Deploying relay + 2 agents ==="
RAVENFABRIC_VERSION="${VERSION}" envsubst < deploy/k8s/ravenfabric.yaml | kubectl apply -n "${NAMESPACE}" -f -

# Wait for all pods
echo ""
echo "=== Waiting for pods ==="
for i in $(seq 1 30); do
    READY=$(kubectl get pod -n "${NAMESPACE}" --no-headers 2>/dev/null | grep -c "Running" || true)
    TOTAL=$(kubectl get pod -n "${NAMESPACE}" --no-headers 2>/dev/null | wc -l || echo 0)
    echo "  ${i}/30: ${READY}/${TOTAL} ready"
    if [ "${READY}" -ge 3 ] 2>/dev/null; then
        break
    fi
    sleep 5
done

echo ""
echo "=== Pod status ==="
kubectl get pod -n "${NAMESPACE}" -o wide

echo ""
echo "=== Service ==="
kubectl get svc -n "${NAMESPACE}" relay-svc

echo ""
echo "=== Agent connectivity check ==="
sleep 30

for pod in agent1 agent2; do
    echo "  ${pod}:"
    kubectl logs -n "${NAMESPACE}" "${pod}" --tail=5 2>/dev/null | grep -E "handshake|connecting|session|reconnect|failing" | tail -3 || echo "    (no relevant entries yet)"
done

echo ""
echo "=== Relay ==="
kubectl logs -n "${NAMESPACE}" relay --tail=3 2>/dev/null | grep "listening"

echo ""
echo "=== Deploy complete ==="
echo ""
echo "To test:"
echo "  kubectl port-forward -n ${NAMESPACE} pod/relay 9090:9090 &"
echo "  rf --relay ws://127.0.0.1:9090 exec --token agent1 'hostname && echo WORKS'"
echo "  rf --relay ws://127.0.0.1:9090 exec --token agent2 'hostname && echo WORKS'"
echo ""
echo "To teardown:"
echo "  kubectl delete pod -n ${NAMESPACE} --all"
