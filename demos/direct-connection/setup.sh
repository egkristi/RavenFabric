#!/usr/bin/env bash
# RavenFabric Direct Connection Demo — Setup Script
#
# Creates 1 Ubuntu container:
#   rf-direct — agent in listen mode on port 9999 (host-mapped)
#
# Usage:
#   ./setup.sh          # Create and start the container
#   ./setup.sh teardown  # Stop and remove the container

set -euo pipefail

VERSION="v0.2.0"
LISTEN_PORT="${LISTEN_PORT:-9999}"
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
CONTAINER="rf-direct"

POLICY='spec:
  commands:
    allow:
      - pattern: "^hostname$"
      - pattern: "^uname.*"
      - pattern: "^cat /etc/os-release$"
      - pattern: "^uptime$"
      - pattern: "^df -h$"
      - pattern: "^free -h$"
      - pattern: "^whoami$"
      - pattern: "^id$"
      - pattern: "^ps aux$"
      - pattern: "^date$"
      - pattern: "^echo .*"
      - pattern: "^for .*"
      - pattern: "^cat /var/log/rf-audit.jsonl$"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*shutdown.*"
      - pattern: ".*reboot.*"
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 60'

teardown() {
    echo "Tearing down demo container..."
    if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER}$"; then
        docker rm -f "$CONTAINER" > /dev/null 2>&1
        echo "  Removed: $CONTAINER"
    fi
    echo "Done."
}

if [[ "${1:-}" == "teardown" ]]; then
    teardown
    exit 0
fi

# Clean up any existing container
if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER}$"; then
    echo "Removing existing container: $CONTAINER"
    docker rm -f "$CONTAINER" > /dev/null 2>&1
fi

echo "=== RavenFabric Direct Connection Demo ==="
echo ""
echo "Version:      ${VERSION}"
echo "Architecture: ${ARCH} (${BINARY_ARCH})"
echo "Listen port:  ${LISTEN_PORT}"
echo ""

# --- Create agent container with listen mode ---
echo "[1/1] Starting agent container in direct-listen mode..."
docker run -d --name "$CONTAINER" -p "${LISTEN_PORT}:9999" "$IMAGE" sleep infinity > /dev/null
docker exec "$CONTAINER" bash -c "
    apt-get update -qq && apt-get install -y -qq curl ca-certificates > /dev/null 2>&1 &&
    curl -fsSL -o /usr/local/bin/rf-agent '${BASE_URL}/ravenfabric-${BINARY_ARCH}-agent' &&
    chmod +x /usr/local/bin/rf-agent &&
    mkdir -p /etc/ravenfabric &&
    cat > /etc/ravenfabric/policy.yaml << 'POLICYEOF'
${POLICY}
POLICYEOF
"

docker exec -d "$CONTAINER" bash -c "
    RUST_LOG=info rf-agent \
        --listen 0.0.0.0:9999 \
        --id rf-direct \
        --policy-path /etc/ravenfabric/policy.yaml \
        --audit-path /var/log/rf-audit.jsonl \
        --key-path /etc/ravenfabric/agent.key \
        > /var/log/rf-agent.log 2>&1
"
sleep 2

HOSTNAME=$(docker exec "$CONTAINER" hostname)
echo "  Agent running in listen mode (hostname: ${HOSTNAME})"

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Container:"
docker ps --filter name=rf-direct --format "  {{.Names}}\t{{.Image}}\t{{.Status}}"
echo ""
echo "Usage (from your local machine):"
echo ""
echo "  # Build rf CLI (if you haven't already)"
echo "  cargo build --release -p rf-cli"
echo ""
echo "  # Execute directly (no relay, no token needed)"
echo "  rf --connect ws://127.0.0.1:${LISTEN_PORT} exec --token unused 'hostname && uname -a'"
echo ""
echo "  # Stream output"
echo "  rf --connect ws://127.0.0.1:${LISTEN_PORT} exec --token unused --stream 'echo hello world'"
echo ""
echo "  # Check agent status"
echo "  rf --connect ws://127.0.0.1:${LISTEN_PORT} status --token unused"
echo ""
echo "  # Teardown"
echo "  ./setup.sh teardown"
