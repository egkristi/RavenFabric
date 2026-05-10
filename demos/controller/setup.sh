#!/usr/bin/env bash
# RavenFabric Controller / Web UI Demo — Setup Script
#
# Creates containers demonstrating the controller HTTP API and web dashboard:
#   rf-ctrl-relay  — relay + controller (ports 9095, 8080)
#   rf-ctrl-agent-1 — managed agent (token: node1)
#   rf-ctrl-agent-2 — managed agent (token: node2)
#
# Usage:
#   ./setup.sh          # Create and start all containers
#   ./setup.sh teardown  # Stop and remove all containers

set -euo pipefail

VERSION="v0.2.0"
RELAY_PORT="${RELAY_PORT:-9095}"
HTTP_PORT="${HTTP_PORT:-8080}"
IMAGE="ubuntu:24.04"

ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  BINARY_ARCH="linux-amd64-musl" ;;
    aarch64|arm64) BINARY_ARCH="linux-arm64-musl" ;;
    armv7l)  BINARY_ARCH="linux-armv7-musl" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

BASE_URL="https://github.com/egkristi/RavenFabric-Published/releases/download/${VERSION}"
CONTAINERS=("rf-ctrl-relay" "rf-ctrl-agent-1" "rf-ctrl-agent-2")

POLICY='spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300'

teardown() {
    echo "Tearing down controller demo containers..."
    for name in "${CONTAINERS[@]}"; do
        if docker ps -a --format '{{.Names}}' | grep -q "^${name}$"; then
            docker rm -f "$name" > /dev/null 2>&1
            echo "  Removed: $name"
        fi
    done
    echo "Done."
}

if [[ "${1:-}" == "teardown" ]]; then
    teardown
    exit 0
fi

for name in "${CONTAINERS[@]}"; do
    if docker ps -a --format '{{.Names}}' | grep -q "^${name}$"; then
        docker rm -f "$name" > /dev/null 2>&1
    fi
done

echo "=== RavenFabric Controller / Web UI Demo ==="
echo ""
echo "Version:      ${VERSION}"
echo "Architecture: ${ARCH} (${BINARY_ARCH})"
echo "Relay port:   ${RELAY_PORT}"
echo "HTTP port:    ${HTTP_PORT}"
echo ""

# --- 1. Create relay + controller ---
echo "[1/3] Starting relay + controller..."
docker run -d --name rf-ctrl-relay \
    -p "${RELAY_PORT}:9090" \
    -p "${HTTP_PORT}:8080" \
    "$IMAGE" sleep infinity > /dev/null

docker exec rf-ctrl-relay bash -c "
    apt-get update -qq && apt-get install -y -qq curl ca-certificates > /dev/null 2>&1 &&
    curl -fsSL -o /usr/local/bin/rf-relay '${BASE_URL}/ravenfabric-${BINARY_ARCH}-relay' &&
    chmod +x /usr/local/bin/rf-relay
"
docker exec -d rf-ctrl-relay bash -c "RUST_LOG=info rf-relay --listen 0.0.0.0:9090 > /var/log/rf-relay.log 2>&1"
sleep 1
echo "  Relay on port ${RELAY_PORT}, HTTP API on port ${HTTP_PORT}"

RELAY_IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' rf-ctrl-relay)

# --- 2-3. Create agents ---
for i in 1 2; do
    AGENT_NAME="rf-ctrl-agent-${i}"
    AGENT_TOKEN="node${i}"
    echo ""
    echo "[$(( i + 1 ))/3] Starting agent: ${AGENT_NAME} (token: ${AGENT_TOKEN})..."

    docker run -d --name "$AGENT_NAME" "$IMAGE" sleep infinity > /dev/null
    docker exec "$AGENT_NAME" bash -c "
        apt-get update -qq && apt-get install -y -qq curl ca-certificates > /dev/null 2>&1 &&
        curl -fsSL -o /usr/local/bin/rf-agent '${BASE_URL}/ravenfabric-${BINARY_ARCH}-agent' &&
        chmod +x /usr/local/bin/rf-agent &&
        mkdir -p /etc/ravenfabric &&
        cat > /etc/ravenfabric/policy.yaml << 'POLICYEOF'
${POLICY}
POLICYEOF
    "
    docker exec -d "$AGENT_NAME" bash -c "
        RUST_LOG=info rf-agent \
            --relay ws://${RELAY_IP}:9090 \
            --id ${AGENT_NAME} \
            --token ${AGENT_TOKEN} \
            --policy-path /etc/ravenfabric/policy.yaml \
            --audit-path /var/log/rf-audit.jsonl \
            --key-path /etc/ravenfabric/agent.key \
            > /var/log/rf-agent.log 2>&1
    "
    sleep 1
    echo "  Agent ${AGENT_NAME} running"
done

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Containers:"
docker ps --filter name=rf-ctrl --format "  {{.Names}}\t{{.Image}}\t{{.Status}}"
echo ""
echo "Web Dashboard:  http://localhost:${HTTP_PORT}"
echo "API Endpoints:"
echo "  GET  http://localhost:${HTTP_PORT}/api/agents"
echo "  GET  http://localhost:${HTTP_PORT}/api/health"
echo ""
echo "Run scenario scripts:"
echo ""
echo "  ./scenarios/01-agent-list.sh"
echo "  ./scenarios/02-health-check.sh"
echo "  ./scenarios/03-remote-execution.sh"
echo "  ./scenarios/04-fleet-dashboard.sh"
echo "  ./scenarios/05-policy-view.sh"
echo ""
echo "Teardown: ./setup.sh teardown"
