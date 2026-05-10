#!/usr/bin/env bash
# RavenFabric Resilience Demo — Setup Script
#
# Creates 4 containers to demonstrate resilience:
#   rf-relay-res   — relay broker (port 9094)
#   rf-agent-res-1 — web server agent (token: web01)
#   rf-agent-res-2 — database agent (token: db01)
#   rf-agent-res-3 — web server agent (token: web02)
#
# Usage:
#   ./setup.sh          # Create and start all containers
#   ./setup.sh teardown  # Stop and remove all containers

set -euo pipefail

VERSION="v0.2.0"
RELAY_PORT="${RELAY_PORT:-9094}"
IMAGE="ubuntu:24.04"

ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  BINARY_ARCH="linux-amd64-musl" ;;
    aarch64|arm64) BINARY_ARCH="linux-arm64-musl" ;;
    armv7l)  BINARY_ARCH="linux-armv7-musl" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

BASE_URL="https://github.com/egkristi/RavenFabric-Published/releases/download/${VERSION}"
CONTAINERS=("rf-relay-res" "rf-agent-res-1" "rf-agent-res-2" "rf-agent-res-3")

POLICY='spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 60'

teardown() {
    echo "Tearing down resilience demo containers..."
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

echo "=== RavenFabric Resilience Demo ==="
echo ""
echo "Version:      ${VERSION}"
echo "Architecture: ${ARCH} (${BINARY_ARCH})"
echo "Relay port:   ${RELAY_PORT}"
echo ""

# --- 1. Create relay ---
echo "[1/4] Starting relay container..."
docker run -d --name rf-relay-res -p "${RELAY_PORT}:9090" "$IMAGE" sleep infinity > /dev/null
docker exec rf-relay-res bash -c "
    apt-get update -qq && apt-get install -y -qq curl ca-certificates > /dev/null 2>&1 &&
    curl -fsSL -o /usr/local/bin/rf-relay '${BASE_URL}/ravenfabric-${BINARY_ARCH}-relay' &&
    chmod +x /usr/local/bin/rf-relay
"
docker exec -d rf-relay-res bash -c "RUST_LOG=info rf-relay --listen 0.0.0.0:9090 > /var/log/rf-relay.log 2>&1"
sleep 1
echo "  Relay listening on port ${RELAY_PORT}"

RELAY_IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' rf-relay-res)

# --- 2-4. Create agents ---
TOKENS=("web01" "db01" "web02")
for i in 1 2 3; do
    AGENT_NAME="rf-agent-res-${i}"
    AGENT_TOKEN="${TOKENS[$((i-1))]}"
    echo ""
    echo "[$(( i + 1 ))/4] Starting agent: ${AGENT_NAME} (token: ${AGENT_TOKEN})..."

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
docker ps --filter name=rf-*-res* --format "  {{.Names}}\t{{.Image}}\t{{.Status}}"
echo ""
echo "Run scenario scripts to test resilience:"
echo ""
echo "  ./scenarios/01-agent-reconnect.sh"
echo "  ./scenarios/02-relay-restart.sh"
echo "  ./scenarios/03-network-partition.sh"
echo "  ./scenarios/04-graceful-degradation.sh"
echo "  ./scenarios/05-backoff-behavior.sh"
echo ""
echo "Teardown: ./setup.sh teardown"
