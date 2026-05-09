#!/usr/bin/env bash
# RavenFabric Kubernetes + CloudNativePG Demo
#
# Deploys a CloudNativePG PostgreSQL cluster with a RavenFabric agent
# companion pod for remote database access and command execution.
#
# Components:
#   Docker:     rf-relay-k8s (Ubuntu 24.04, port 9093)
#   Kubernetes: ravenfabric namespace
#     - pg-cluster (CNPG, 2 instances)
#     - rf-agent   (Deployment, psql + rf-agent binary)
#
# Usage:
#   ./setup.sh              # Deploy everything
#   ./setup.sh teardown     # Remove everything
#   ./setup.sh status       # Show component status
#   ./setup.sh verify       # Test connectivity and queries

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
VERSION="v0.1.4"
RELAY_PORT="${RELAY_PORT:-9093}"
RELAY_NAME="rf-relay-k8s"
NAMESPACE="ravenfabric"
CLUSTER_NAME="pg-cluster"
BASE_URL="https://github.com/egkristi/RavenFabric-Published/releases/download/${VERSION}"

# Detect architecture for relay binary
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)        BINARY_ARCH="linux-amd64-musl" ;;
    aarch64|arm64) BINARY_ARCH="linux-arm64-musl" ;;
    *)             echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# --- Functions ---

check_prerequisites() {
    echo "Checking prerequisites..."
    local missing=0

    for cmd in docker kubectl; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            echo "  MISSING: $cmd"
            missing=1
        fi
    done

    if [[ $missing -eq 1 ]]; then
        exit 1
    fi

    echo -n "  kubectl: "
    if kubectl cluster-info >/dev/null 2>&1; then
        local context
        context=$(kubectl config current-context 2>/dev/null || echo "unknown")
        echo "connected (context: ${context})"
    else
        echo "FAILED — cannot connect to Kubernetes cluster"
        echo "  Ensure Rancher Desktop is running"
        exit 1
    fi

    echo -n "  CNPG operator: "
    if kubectl get deployment -n cnpg-system -l app.kubernetes.io/name=cloudnative-pg --no-headers 2>/dev/null | grep -q .; then
        local version
        version=$(kubectl get deployment -n cnpg-system -l app.kubernetes.io/name=cloudnative-pg -o jsonpath='{.items[0].spec.template.spec.containers[0].image}' 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "unknown")
        echo "installed (v${version})"
    else
        echo "NOT FOUND"
        echo ""
        echo "  CloudNativePG operator is required. Install with:"
        echo "    helm repo add cnpg https://cloudnative-pg.github.io/charts"
        echo "    helm upgrade --install cnpg-operator cnpg/cloudnative-pg -n cnpg-system --create-namespace --wait"
        exit 1
    fi
}

detect_relay_host() {
    # If explicitly set, use that
    if [[ "${RELAY_HOST:-}" != "" ]]; then
        echo "$RELAY_HOST"
        return
    fi

    # macOS: get IP of the default route interface
    if command -v ipconfig >/dev/null 2>&1; then
        local iface ip
        iface=$(route -n get default 2>/dev/null | awk '/interface:/{print $2}')
        if [[ -n "$iface" ]]; then
            ip=$(ipconfig getifaddr "$iface" 2>/dev/null || true)
            if [[ -n "$ip" ]]; then
                echo "$ip"
                return
            fi
        fi
    fi

    # Fallback: first non-loopback IP
    ifconfig 2>/dev/null | grep 'inet ' | grep -v '127.0.0.1' | head -1 | awk '{print $2}'
}

exempt_namespace() {
    echo "  Exempting '${NAMESPACE}' namespace from Gatekeeper constraints..."
    local count=0
    while IFS= read -r constraint; do
        [[ -z "$constraint" ]] && continue
        # Skip if already excluded
        local excluded
        excluded=$(kubectl get "$constraint" -o jsonpath='{.spec.match.excludedNamespaces[*]}' 2>/dev/null || echo "")
        if echo " $excluded " | grep -qw "$NAMESPACE"; then
            continue
        fi
        # Try appending; if excludedNamespaces doesn't exist, create it
        if kubectl patch "$constraint" --type=json \
            -p="[{\"op\":\"add\",\"path\":\"/spec/match/excludedNamespaces/-\",\"value\":\"${NAMESPACE}\"}]" \
            >/dev/null 2>&1; then
            count=$((count + 1))
        elif kubectl patch "$constraint" --type=json \
            -p="[{\"op\":\"add\",\"path\":\"/spec/match/excludedNamespaces\",\"value\":[\"${NAMESPACE}\"]}]" \
            >/dev/null 2>&1; then
            count=$((count + 1))
        fi
    done < <(kubectl get constraints -o name 2>/dev/null)
    echo "  Patched ${count} constraint(s)"
}

unexempt_namespace() {
    echo "  Removing '${NAMESPACE}' from Gatekeeper constraint exclusions..."
    local count=0
    while IFS= read -r constraint; do
        [[ -z "$constraint" ]] && continue
        local idx=0
        while IFS= read -r ns; do
            if [[ "$ns" == "$NAMESPACE" ]]; then
                if kubectl patch "$constraint" --type=json \
                    -p="[{\"op\":\"remove\",\"path\":\"/spec/match/excludedNamespaces/${idx}\"}]" \
                    >/dev/null 2>&1; then
                    count=$((count + 1))
                fi
                break
            fi
            idx=$((idx + 1))
        done < <(kubectl get "$constraint" -o jsonpath='{range .spec.match.excludedNamespaces[*]}{@}{"\n"}{end}' 2>/dev/null)
    done < <(kubectl get constraints -o name 2>/dev/null)
    echo "  Cleaned ${count} constraint(s)"
}

setup_relay() {
    # Remove existing relay if present
    if docker ps -a --format '{{.Names}}' | grep -q "^${RELAY_NAME}$"; then
        docker rm -f "$RELAY_NAME" > /dev/null 2>&1
    fi

    docker run -d --name "$RELAY_NAME" -p "${RELAY_PORT}:9090" ubuntu:24.04 sleep infinity > /dev/null
    docker exec "$RELAY_NAME" bash -c "
        apt-get update -qq && apt-get install -y -qq curl ca-certificates > /dev/null 2>&1 &&
        curl -fsSL -o /usr/local/bin/rf-relay '${BASE_URL}/ravenfabric-${BINARY_ARCH}-relay' &&
        chmod +x /usr/local/bin/rf-relay
    "
    docker exec -d "$RELAY_NAME" bash -c "RUST_LOG=info rf-relay --listen 0.0.0.0:9090 > /var/log/rf-relay.log 2>&1"
    sleep 1
    echo "  Relay listening on host port ${RELAY_PORT}"
}

deploy_cnpg_cluster() {
    kubectl apply -f "${SCRIPT_DIR}/manifests/namespace.yaml"
    kubectl apply -f "${SCRIPT_DIR}/manifests/cnpg-cluster.yaml"

    echo "  Waiting for CNPG cluster pods (this may take a few minutes)..."

    # Wait for at least one pod to appear
    local retries=60
    while [[ $retries -gt 0 ]]; do
        local count
        count=$(kubectl get pods -l "cnpg.io/cluster=${CLUSTER_NAME}" -n "${NAMESPACE}" --no-headers 2>/dev/null | wc -l | tr -d ' ')
        if [[ "$count" -ge 1 ]]; then
            break
        fi
        sleep 5
        ((retries--))
    done

    # Wait for pods to become ready
    if ! kubectl wait --for=condition=Ready pod \
        -l "cnpg.io/cluster=${CLUSTER_NAME}" \
        -n "${NAMESPACE}" \
        --timeout=300s 2>/dev/null; then
        echo "  WARNING: CNPG cluster may not be fully ready"
        echo "  Check with: kubectl get pods -n ${NAMESPACE}"
    fi

    echo "  CNPG cluster deployed"
}

deploy_rf_agent() {
    local relay_host="$1"

    kubectl apply -f "${SCRIPT_DIR}/manifests/rf-agent-configmap.yaml"

    # Substitute relay host/port/version into the deployment manifest
    sed -e "s|__RELAY_HOST__|${relay_host}|g" \
        -e "s|__RELAY_PORT__|${RELAY_PORT}|g" \
        -e "s|__VERSION__|${VERSION}|g" \
        "${SCRIPT_DIR}/manifests/rf-agent-deployment.yaml" | kubectl apply -f -

    echo "  Waiting for rf-agent pod to be ready..."
    if ! kubectl rollout status deployment/rf-agent -n "${NAMESPACE}" --timeout=120s 2>/dev/null; then
        echo "  WARNING: rf-agent may not be ready yet"
        echo "  Check with: kubectl get pods -n ${NAMESPACE} -l app=rf-agent"
    fi

    echo "  RavenFabric agent deployed"
}

do_setup() {
    check_prerequisites
    echo ""
    echo "=== RavenFabric Kubernetes + CloudNativePG Demo ==="
    echo ""
    echo "Version:      ${VERSION}"
    echo "Relay port:   ${RELAY_PORT}"
    echo "Namespace:    ${NAMESPACE}"
    echo ""

    local total=5

    echo "[1/${total}] Setting up Gatekeeper exemptions..."
    exempt_namespace
    echo ""

    echo "[2/${total}] Starting relay on Docker..."
    setup_relay
    local relay_host
    relay_host=$(detect_relay_host)
    echo "  Relay host for k8s pods: ${relay_host}"
    echo ""

    echo "[3/${total}] Deploying CNPG cluster (2 instances)..."
    deploy_cnpg_cluster
    echo ""

    echo "[4/${total}] Deploying RavenFabric agent..."
    deploy_rf_agent "$relay_host"
    echo ""

    echo "[5/${total}] Setup complete!"
    echo ""
    echo "=== Resources ==="
    echo ""
    echo "Docker:"
    docker ps --filter "name=${RELAY_NAME}" --format "  {{.Names}}\t{{.Image}}\t{{.Status}}"
    echo ""
    echo "Kubernetes (namespace: ${NAMESPACE}):"
    kubectl get pods -n "${NAMESPACE}" -o wide 2>/dev/null | sed 's/^/  /'
    echo ""
    echo "=== Usage ==="
    echo ""
    local rf="${RF_CLI:-rf}"
    echo "  # Run a command on the agent pod"
    echo "  ${rf} --relay ws://127.0.0.1:${RELAY_PORT} exec --token cnpg 'uname -a'"
    echo ""
    echo "  # Query PostgreSQL"
    echo "  ${rf} --relay ws://127.0.0.1:${RELAY_PORT} exec --token cnpg 'psql -c \"SELECT version();\"'"
    echo ""
    echo "  # List databases"
    echo "  ${rf} --relay ws://127.0.0.1:${RELAY_PORT} exec --token cnpg 'psql -c \"\\\\l\"'"
    echo ""
    echo "  # Check replication status"
    echo "  ${rf} --relay ws://127.0.0.1:${RELAY_PORT} exec --token cnpg 'psql -c \"SELECT client_addr, state, sync_state FROM pg_stat_replication;\"'"
    echo ""
    echo "  # Create a table and insert data"
    echo "  ${rf} --relay ws://127.0.0.1:${RELAY_PORT} exec --token cnpg 'psql -c \"CREATE TABLE demo(id serial PRIMARY KEY, name text); INSERT INTO demo(name) VALUES (\\\"hello\\\"), (\\\"ravenfabric\\\"); SELECT * FROM demo;\"'"
    echo ""
    echo "  # Verify all components"
    echo "  ./setup.sh verify"
    echo ""
    echo "  # Teardown"
    echo "  ./setup.sh teardown"
}

do_teardown() {
    echo "Tearing down Kubernetes + CloudNativePG demo..."

    # Delete rf-agent
    kubectl delete deployment rf-agent -n "${NAMESPACE}" --ignore-not-found > /dev/null 2>&1
    kubectl delete configmap rf-agent-policy -n "${NAMESPACE}" --ignore-not-found > /dev/null 2>&1
    echo "  Removed: rf-agent"

    # Delete CNPG cluster
    kubectl delete cluster "${CLUSTER_NAME}" -n "${NAMESPACE}" --ignore-not-found > /dev/null 2>&1
    echo "  Removed: CNPG cluster (waiting for pods to terminate...)"

    # Wait for CNPG pods to terminate
    local retries=30
    while [[ $retries -gt 0 ]]; do
        local count
        count=$(kubectl get pods -n "${NAMESPACE}" --no-headers 2>/dev/null | wc -l | tr -d ' ')
        if [[ "$count" -eq 0 ]]; then
            break
        fi
        sleep 5
        ((retries--))
    done

    # Delete PVCs
    kubectl delete pvc -l "cnpg.io/cluster=${CLUSTER_NAME}" -n "${NAMESPACE}" --ignore-not-found > /dev/null 2>&1
    echo "  Removed: PVCs"

    # Delete namespace
    kubectl delete namespace "${NAMESPACE}" --ignore-not-found > /dev/null 2>&1
    echo "  Removed: namespace"

    # Remove Gatekeeper exemptions
    unexempt_namespace

    # Remove relay
    if docker ps -a --format '{{.Names}}' | grep -q "^${RELAY_NAME}$"; then
        docker rm -f "${RELAY_NAME}" > /dev/null 2>&1
        echo "  Removed: relay container"
    fi

    echo "Done."
}

do_status() {
    echo "=== Demo Status ==="
    echo ""
    echo "Docker Relay:"
    if docker ps --format '  {{.Names}}\t{{.Image}}\t{{.Status}}' --filter "name=${RELAY_NAME}" 2>/dev/null | head -1 | grep -q .; then
        docker ps --format '  {{.Names}}\t{{.Image}}\t{{.Status}}' --filter "name=${RELAY_NAME}"
    else
        echo "  Not running"
    fi
    echo ""
    echo "Kubernetes Pods (namespace: ${NAMESPACE}):"
    kubectl get pods -n "${NAMESPACE}" -o wide 2>/dev/null | sed 's/^/  /' || echo "  Namespace not found"
    echo ""
    echo "CNPG Cluster:"
    kubectl get cluster -n "${NAMESPACE}" 2>/dev/null | sed 's/^/  /' || echo "  Not found"
    echo ""
    echo "Services:"
    kubectl get svc -n "${NAMESPACE}" 2>/dev/null | sed 's/^/  /' || echo "  Not found"
}

do_verify() {
    echo "=== Verifying Setup ==="
    echo ""
    local failures=0

    # Check relay
    echo -n "  Relay (Docker):  "
    if docker ps --format '{{.Names}}' | grep -q "^${RELAY_NAME}$"; then
        echo "OK"
    else
        echo "MISSING"
        ((failures++))
    fi

    # Check CNPG pods
    echo -n "  CNPG pods:       "
    local ready
    ready=$(kubectl get pods -l "cnpg.io/cluster=${CLUSTER_NAME}" -n "${NAMESPACE}" --no-headers 2>/dev/null | grep -c "Running" || echo "0")
    if [[ "$ready" -ge 1 ]]; then
        echo "${ready} running"
    else
        echo "NONE running"
        ((failures++))
    fi

    # Check rf-agent pod
    echo -n "  rf-agent pod:    "
    local agent_status
    agent_status=$(kubectl get pods -l app=rf-agent -n "${NAMESPACE}" -o jsonpath='{.items[0].status.phase}' 2>/dev/null || echo "Missing")
    if [[ "$agent_status" == "Running" ]]; then
        echo "Running"
    else
        echo "$agent_status"
        ((failures++))
    fi

    # Check CNPG services
    echo -n "  CNPG services:   "
    local svc_count
    svc_count=$(kubectl get svc -l "cnpg.io/cluster=${CLUSTER_NAME}" -n "${NAMESPACE}" --no-headers 2>/dev/null | wc -l | tr -d ' ')
    echo "${svc_count} service(s)"

    echo ""

    # Test remote execution
    local rf="${RF_CLI:-rf}"
    if ! command -v "$rf" >/dev/null 2>&1; then
        echo "  rf CLI not found. Set RF_CLI=./target/release/rf or install it."
        return
    fi

    echo "  Testing remote execution..."
    echo -n "    OS info:       "
    if $rf --relay "ws://127.0.0.1:${RELAY_PORT}" exec --token cnpg 'cat /etc/os-release | head -1' 2>/dev/null; then
        :
    else
        echo "FAILED"
        ((failures++))
    fi

    sleep 6

    echo -n "    PostgreSQL:    "
    if $rf --relay "ws://127.0.0.1:${RELAY_PORT}" exec --token cnpg 'psql -t -A -c "SELECT version();"' 2>/dev/null; then
        :
    else
        echo "FAILED"
        ((failures++))
    fi

    echo ""
    if [[ $failures -eq 0 ]]; then
        echo "  All checks passed."
    else
        echo "  ${failures} check(s) failed."
    fi
}

# --- Main ---

case "${1:-setup}" in
    setup)    do_setup ;;
    teardown) do_teardown ;;
    status)   do_status ;;
    verify)   do_verify ;;
    *)
        echo "Usage: $0 [setup|teardown|status|verify]"
        exit 1
        ;;
esac
