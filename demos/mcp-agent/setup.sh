#!/usr/bin/env bash
# RavenFabric MCP / AI Agent Demo — Setup Script
#
# Creates a local MCP server with policy enforcement and approval workflow.
# Demonstrates AI agent integration through the Model Context Protocol.
#
# Components:
#   rf-mcp-demo  — Ubuntu container running rf-mcp-server (stdio mode)
#
# Usage:
#   ./setup.sh          # Create and start the demo container
#   ./setup.sh teardown  # Stop and remove the container

set -euo pipefail

VERSION="v0.2.0"
IMAGE="ubuntu:24.04"
CONTAINER="rf-mcp-demo"

# Detect architecture for binary download
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  BINARY_ARCH="linux-amd64-musl" ;;
    aarch64|arm64) BINARY_ARCH="linux-arm64-musl" ;;
    armv7l)  BINARY_ARCH="linux-armv7-musl" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

BASE_URL="https://github.com/egkristi/RavenFabric-Published/releases/download/${VERSION}"

# Policy: read-only by default, write operations require approval
POLICY='spec:
  commands:
    allow:
      - pattern: "^cat .*"
      - pattern: "^ls .*"
      - pattern: "^uname .*"
      - pattern: "^hostname$"
      - pattern: "^whoami$"
      - pattern: "^df .*"
      - pattern: "^free .*"
      - pattern: "^uptime$"
      - pattern: "^ps .*"
      - pattern: "^id$"
    deny:
      - pattern: ".*rm .*-rf.*"
      - pattern: ".*shutdown.*"
      - pattern: ".*reboot.*"
  filesystem:
    allow:
      - path: /tmp
      - path: /var/log
      - path: /etc/os-release
    deny:
      - path: /etc/shadow
      - path: /etc/passwd
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30'

teardown() {
    echo "Tearing down MCP demo container..."
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

# Clean up existing container
if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER}$"; then
    echo "Removing existing container: $CONTAINER"
    docker rm -f "$CONTAINER" > /dev/null 2>&1
fi

echo "=== RavenFabric MCP / AI Agent Demo ==="
echo ""
echo "Version:      ${VERSION}"
echo "Architecture: ${ARCH} (${BINARY_ARCH})"
echo ""

# --- Create demo container ---
echo "[1/1] Setting up MCP demo container..."
docker run -d --name "$CONTAINER" "$IMAGE" sleep infinity > /dev/null
docker exec "$CONTAINER" bash -c "
    apt-get update -qq && apt-get install -y -qq curl ca-certificates > /dev/null 2>&1 &&
    curl -fsSL -o /usr/local/bin/rf-mcp-server '${BASE_URL}/ravenfabric-${BINARY_ARCH}-mcp-server' &&
    chmod +x /usr/local/bin/rf-mcp-server &&
    mkdir -p /etc/ravenfabric /var/log/ravenfabric &&
    cat > /etc/ravenfabric/policy.yaml << 'POLICYEOF'
${POLICY}
POLICYEOF
"
echo "  MCP server binary installed"

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Container:"
docker ps --filter name=rf-mcp-demo --format "  {{.Names}}\t{{.Image}}\t{{.Status}}"
echo ""
echo "The MCP server runs in stdio mode inside the container."
echo "Run scenario scripts to see it in action:"
echo ""
echo "  ./scenarios/01-policy-discovery.sh"
echo "  ./scenarios/02-safe-execution.sh"
echo "  ./scenarios/03-policy-denial.sh"
echo "  ./scenarios/04-human-approval.sh"
echo "  ./scenarios/05-audit-trail.sh"
echo "  ./scenarios/06-file-operations.sh"
echo ""
echo "Teardown:"
echo "  ./setup.sh teardown"
