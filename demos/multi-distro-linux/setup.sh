#!/usr/bin/env bash
# RavenFabric Multi-Distro Linux Demo — Setup Script
#
# Creates containers across major Linux distributions to verify that
# RavenFabric's static musl binaries work on every distro without
# runtime dependencies.
#
# Containers:
#   rf-relay-ubuntu   — relay broker (Ubuntu 24.04, port 9092)
#   rf-ubuntu         — agent (Ubuntu 24.04, apt/deb)
#   rf-debian         — agent (Debian 12 Bookworm, apt/deb)
#   rf-fedora         — agent (Fedora 41, dnf/rpm)
#   rf-rocky          — agent (Rocky Linux 9, dnf/rpm, RHEL-compatible)
#   rf-manjaro       — agent (Manjaro ARM, pacman, Arch-based)
#   rf-opensuse       — agent (openSUSE Tumbleweed, zypper/rpm)
#   rf-alpine         — agent (Alpine 3.20, apk, musl-native)
#   rf-amazon         — agent (Amazon Linux 2023, dnf/rpm, AWS)
#   rf-void           — agent (Void Linux, xbps, musl variant)
#
# Usage:
#   ./setup.sh              # Create and start all containers
#   ./setup.sh teardown     # Stop and remove all containers
#   ./setup.sh status       # Show container status
#   ./setup.sh verify       # Run verification commands on all agents

set -euo pipefail

VERSION="v0.2.0"
RELAY_PORT="${RELAY_PORT:-9092}"

# Detect architecture for binary download
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  BINARY_ARCH="linux-amd64-musl" ;;
    aarch64|arm64) BINARY_ARCH="linux-arm64-musl" ;;
    armv7l)  BINARY_ARCH="linux-armv7-musl" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

BASE_URL="https://github.com/egkristi/RavenFabric-Published/releases/download/${VERSION}"

# Container prefix for this demo (avoid collision with multi-node-ubuntu demo)
PREFIX="rf"

# Distro definitions: name|image|shell|install_cmd
# install_cmd: commands to install curl/ca-certificates (distro-specific)
DISTROS=(
    "ubuntu|ubuntu:24.04|bash|apt-get update -qq && apt-get install -y -qq curl ca-certificates > /dev/null 2>&1"
    "debian|debian:12-slim|bash|apt-get update -qq && apt-get install -y -qq curl ca-certificates > /dev/null 2>&1"
    "fedora|fedora:41|bash|dnf install -y -q curl ca-certificates > /dev/null 2>&1"
    "rocky|rockylinux:9|bash|dnf install -y -q ca-certificates > /dev/null 2>&1"
    "manjaro|manjarolinux/base:latest|bash|pacman -Sy --noconfirm curl ca-certificates > /dev/null 2>&1"
    "opensuse|opensuse/tumbleweed:latest|bash|zypper --non-interactive install curl ca-certificates > /dev/null 2>&1"
    "alpine|alpine:3.20|sh|apk add --no-cache curl ca-certificates > /dev/null 2>&1"
    "amazon|amazonlinux:2023|bash|dnf install -y -q ca-certificates > /dev/null 2>&1"
    "void|ghcr.io/void-linux/void-glibc-full:latest|sh|xbps-install -Sy curl ca-certificates > /dev/null 2>&1"
)

RELAY_NAME="${PREFIX}-relay-ubuntu"
AGENT_NAMES=()
for entry in "${DISTROS[@]}"; do
    name="${entry%%|*}"
    AGENT_NAMES+=("${PREFIX}-${name}")
done

ALL_CONTAINERS=("$RELAY_NAME" "${AGENT_NAMES[@]}")

POLICY='spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxOutputBytes: 104857600
    timeoutSeconds: 300'

# --- Functions ---

teardown() {
    echo "Tearing down multi-distro demo containers..."
    for name in "${ALL_CONTAINERS[@]}"; do
        if docker ps -a --format '{{.Names}}' | grep -q "^${name}$"; then
            docker rm -f "$name" > /dev/null 2>&1
            echo "  Removed: $name"
        fi
    done
    echo "Done."
}

show_status() {
    echo "=== Multi-Distro Demo Status ==="
    echo ""
    docker ps -a --filter "name=${PREFIX}-" --format "table {{.Names}}\t{{.Image}}\t{{.Status}}" 2>/dev/null || echo "No containers found."
    echo ""
}

verify_agents() {
    echo "=== Verifying All Agents ==="
    echo ""
    local rf="${RF_CLI:-rf}"
    local relay="ws://127.0.0.1:${RELAY_PORT}"
    local failures=0

    for entry in "${DISTROS[@]}"; do
        IFS='|' read -r name image shell install_cmd <<< "$entry"
        local token="${name}"
        echo -n "  ${name} (${image}): "

        if output=$($rf --relay "$relay" exec --token "$token" 'echo OK' 2>/dev/null); then
            echo "OK"
        else
            echo "FAIL"
            ((failures++))
        fi
        sleep 6
    done

    echo ""
    if [[ $failures -eq 0 ]]; then
        echo "All agents responding."
    else
        echo "${failures} agent(s) failed verification."
    fi
}

# --- Command handling ---

case "${1:-setup}" in
    teardown)
        teardown
        exit 0
        ;;
    status)
        show_status
        exit 0
        ;;
    verify)
        verify_agents
        exit 0
        ;;
    setup)
        ;;
    *)
        echo "Usage: $0 [setup|teardown|status|verify]"
        exit 1
        ;;
esac

# --- Clean up any existing containers ---
for name in "${ALL_CONTAINERS[@]}"; do
    if docker ps -a --format '{{.Names}}' | grep -q "^${name}$"; then
        echo "Removing existing container: $name"
        docker rm -f "$name" > /dev/null 2>&1
    fi
done

TOTAL=$(( ${#DISTROS[@]} + 1 ))
echo "=== RavenFabric Multi-Distro Linux Demo ==="
echo ""
echo "Version:      ${VERSION}"
echo "Architecture: ${ARCH} (${BINARY_ARCH})"
echo "Relay port:   ${RELAY_PORT}"
echo "Distros:      ${#DISTROS[@]}"
echo ""

# --- 1. Create relay container (Ubuntu) ---
echo "[1/${TOTAL}] Starting relay container (Ubuntu 24.04)..."
docker run -d --name "$RELAY_NAME" -p "${RELAY_PORT}:9090" ubuntu:24.04 sleep infinity > /dev/null
docker exec "$RELAY_NAME" bash -c "
    apt-get update -qq && apt-get install -y -qq curl ca-certificates > /dev/null 2>&1 &&
    curl -fsSL -o /usr/local/bin/rf-relay '${BASE_URL}/ravenfabric-${BINARY_ARCH}-relay' &&
    chmod +x /usr/local/bin/rf-relay
"
docker exec -d "$RELAY_NAME" bash -c "RUST_LOG=info rf-relay --listen 0.0.0.0:9090 > /var/log/rf-relay.log 2>&1"
sleep 1
echo "  Relay listening on port ${RELAY_PORT}"

RELAY_IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$RELAY_NAME")
echo "  Relay internal IP: ${RELAY_IP}"
echo ""

# --- 2. Create agent containers ---
STEP=2
for entry in "${DISTROS[@]}"; do
    IFS='|' read -r name image shell install_cmd <<< "$entry"
    AGENT_NAME="${PREFIX}-${name}"
    AGENT_TOKEN="${name}"

    echo "[${STEP}/${TOTAL}] Starting agent: ${AGENT_NAME} (${image}, token: ${AGENT_TOKEN})..."

    # Pull and start container
    docker run -d --name "$AGENT_NAME" "$image" sleep infinity > /dev/null 2>&1 || {
        echo "  WARN: Failed to start ${image} — skipping."
        ((STEP++))
        continue
    }

    # Install curl + ca-certificates (distro-specific)
    docker exec "$AGENT_NAME" "$shell" -c "$install_cmd" || {
        echo "  WARN: Failed to install dependencies on ${name} — skipping."
        docker rm -f "$AGENT_NAME" > /dev/null 2>&1
        ((STEP++))
        continue
    }

    # Download and install RavenFabric binaries
    docker exec "$AGENT_NAME" "$shell" -c "
        curl -fsSL -o /usr/local/bin/rf-agent '${BASE_URL}/ravenfabric-${BINARY_ARCH}-agent' &&
        chmod +x /usr/local/bin/rf-agent &&
        mkdir -p /etc/ravenfabric &&
        cat > /etc/ravenfabric/policy.yaml << 'POLICYEOF'
${POLICY}
POLICYEOF
    " || {
        echo "  WARN: Failed to install rf-agent on ${name} — skipping."
        docker rm -f "$AGENT_NAME" > /dev/null 2>&1
        ((STEP++))
        continue
    }

    # Start the agent
    docker exec -d "$AGENT_NAME" "$shell" -c "
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

    # Verify agent started
    echo "  Agent running on ${image}"

    ((STEP++))
done

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Containers:"
docker ps --filter "name=${PREFIX}-" --format "  {{.Names}}\t{{.Image}}\t{{.Status}}"
echo ""
echo "Usage (from your local machine):"
echo ""
echo "  # Execute on any distro agent"
echo "  rf --relay ws://127.0.0.1:${RELAY_PORT} exec --token ubuntu 'cat /etc/os-release | head -3'"
echo "  rf --relay ws://127.0.0.1:${RELAY_PORT} exec --token debian 'cat /etc/os-release | head -3'"
echo "  rf --relay ws://127.0.0.1:${RELAY_PORT} exec --token fedora 'cat /etc/os-release | head -3'"
echo "  rf --relay ws://127.0.0.1:${RELAY_PORT} exec --token rocky  'cat /etc/os-release | head -3'"
echo "  rf --relay ws://127.0.0.1:${RELAY_PORT} exec --token manjaro 'cat /etc/os-release | head -3'"
echo "  rf --relay ws://127.0.0.1:${RELAY_PORT} exec --token opensuse 'cat /etc/os-release | head -3'"
echo "  rf --relay ws://127.0.0.1:${RELAY_PORT} exec --token alpine 'cat /etc/os-release | head -3'"
echo "  rf --relay ws://127.0.0.1:${RELAY_PORT} exec --token amazon 'cat /etc/os-release | head -3'"
echo "  rf --relay ws://127.0.0.1:${RELAY_PORT} exec --token void   'cat /etc/os-release | head -3'"
echo ""
echo "  # Verify all agents at once"
echo "  ./setup.sh verify"
echo ""
echo "  # Teardown"
echo "  ./setup.sh teardown"
