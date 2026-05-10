#!/usr/bin/env bash
# RavenFabric Multi-Node Ubuntu Demo — Setup Script
#
# Creates 3 Ubuntu containers:
#   rf-relay   — stateless relay broker (port 9091)
#   rf-agent-1 — managed agent (token: agent1)
#   rf-agent-2 — managed agent (token: agent2)
#
# Usage:
#   ./setup.sh          # Create and start all containers
#   ./setup.sh teardown  # Stop and remove all containers

set -euo pipefail

VERSION="v0.2.0"
RELAY_PORT="${RELAY_PORT:-9091}"
IMAGE="ubuntu:24.04"

# Detect architecture for binary download
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  BINARY_ARCH="linux-amd64-musl" ;;
    aarch64|arm64) BINARY_ARCH="linux-arm64-musl" ;;
    armv7l)  BINARY_ARCH="linux-armv7-musl" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

BASE_URL="https://github.com/egkristi/RavenFabric-Published/releases/download/${VERSION}"
CONTAINERS=("rf-relay" "rf-agent-1" "rf-agent-2")

POLICY='spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxOutputBytes: 104857600
    timeoutSeconds: 300'

teardown() {
    echo "Tearing down demo containers..."
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

# Clean up any existing containers
for name in "${CONTAINERS[@]}"; do
    if docker ps -a --format '{{.Names}}' | grep -q "^${name}$"; then
        echo "Removing existing container: $name"
        docker rm -f "$name" > /dev/null 2>&1
    fi
done

echo "=== RavenFabric Multi-Node Ubuntu Demo ==="
echo ""
echo "Version:      ${VERSION}"
echo "Architecture: ${ARCH} (${BINARY_ARCH})"
echo "Relay port:   ${RELAY_PORT}"
echo ""

# --- 1. Create relay container ---
echo "[1/3] Starting relay container..."
docker run -d --name rf-relay -p "${RELAY_PORT}:9090" "$IMAGE" sleep infinity > /dev/null
docker exec rf-relay bash -c "
    apt-get update -qq && apt-get install -y -qq curl ca-certificates > /dev/null 2>&1 &&
    curl -fsSL -o /usr/local/bin/rf-relay '${BASE_URL}/ravenfabric-${BINARY_ARCH}-relay' &&
    chmod +x /usr/local/bin/rf-relay
"
docker exec -d rf-relay bash -c "RUST_LOG=info rf-relay --listen 0.0.0.0:9090 > /var/log/rf-relay.log 2>&1"
sleep 1
echo "  Relay listening on port ${RELAY_PORT}"

RELAY_IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' rf-relay)
echo "  Relay internal IP: ${RELAY_IP}"

# --- 2. Create agent containers ---
for i in 1 2; do
    AGENT_NAME="rf-agent-${i}"
    AGENT_TOKEN="agent${i}"
    echo ""
    echo "[$(( i + 1 ))/3] Starting agent container: ${AGENT_NAME} (token: ${AGENT_TOKEN})..."

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

    HOSTNAME=$(docker exec "$AGENT_NAME" hostname)
    echo "  Agent ${AGENT_NAME} running (hostname: ${HOSTNAME})"
done

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Containers:"
docker ps --filter name=rf- --format "  {{.Names}}\t{{.Image}}\t{{.Status}}"
echo ""
echo "Usage (from your local machine):"
echo ""
echo "  # Build rf CLI (if you haven't already)"
echo "  cargo build --release -p rf-cli"
echo ""
echo "  # Execute on agent 1"
echo "  rf --relay ws://127.0.0.1:${RELAY_PORT} exec --token agent1 'hostname && uname -a'"
echo ""
echo "  # Execute on agent 2"
echo "  rf --relay ws://127.0.0.1:${RELAY_PORT} exec --token agent2 'hostname && uname -a'"
echo ""
echo "  # NOTE: After each exec, the agent reconnects with a brief delay."
echo "  # Wait ~5s between consecutive commands to the same agent."
echo ""
echo "  # Teardown"
echo "  ./setup.sh teardown"
