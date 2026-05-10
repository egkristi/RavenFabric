#!/usr/bin/env bash
# RavenFabric Data Collection Demo — Setup Script
#
# Creates 4 Ubuntu containers simulating a heterogeneous fleet:
#   rf-relay      — stateless relay broker (port 9092)
#   rf-collector  — data aggregation node (token: collector)
#   rf-webserver  — simulated web server (token: webserver)
#   rf-database   — simulated database server (token: database)
#
# Usage:
#   ./setup.sh          # Create and start all containers
#   ./setup.sh teardown  # Stop and remove all containers

set -euo pipefail

VERSION="v0.2.0"
RELAY_PORT="${RELAY_PORT:-9092}"
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
CONTAINERS=("rf-relay" "rf-collector" "rf-webserver" "rf-database")

# Read-only data collection policy — no writes, no destructive commands
POLICY='spec:
  commands:
    allow:
      - pattern: "^hostname$"
      - pattern: "^uname.*"
      - pattern: "^cat /etc/.*"
      - pattern: "^cat /proc/(cpuinfo|meminfo|loadavg|uptime|version|net/dev|diskstats)$"
      - pattern: "^cat /var/log/.*\\.log$"
      - pattern: "^df.*"
      - pattern: "^free.*"
      - pattern: "^uptime$"
      - pattern: "^ps aux.*"
      - pattern: "^top -bn1.*"
      - pattern: "^netstat -tlnp$"
      - pattern: "^ss -tlnp$"
      - pattern: "^ip addr.*"
      - pattern: "^ip route.*"
      - pattern: "^ls -la /.*"
      - pattern: "^du -sh /.*"
      - pattern: "^wc -l /var/log/.*"
      - pattern: "^tail -[0-9]+ /var/log/.*"
      - pattern: "^head -[0-9]+ /var/log/.*"
      - pattern: "^grep .* /var/log/.*"
      - pattern: "^date.*"
      - pattern: "^whoami$"
      - pattern: "^id$"
      - pattern: "^env$"
      - pattern: "^echo .*"
      - pattern: "^find /var/log.*"
      - pattern: "^stat .*"
      - pattern: "^sha256sum .*"
      - pattern: "^md5sum .*"
    deny:
      - pattern: ".*rm .*"
      - pattern: ".*shutdown.*"
      - pattern: ".*reboot.*"
      - pattern: ".*mkfs.*"
      - pattern: ".*dd .*"
      - pattern: ".*curl.*"
      - pattern: ".*wget.*"
      - pattern: ".*apt.*"
      - pattern: ".*pip.*"
      - pattern: ".*chmod.*"
      - pattern: ".*chown.*"
      - pattern: ".*kill.*"
      - pattern: ".*pkill.*"
      - pattern: ".*systemctl.*"
      - pattern: ".*service .*"
      - pattern: ".*mount.*"
      - pattern: ".*umount.*"
      - pattern: ".*iptables.*"
  filesystem:
    allow:
      - path: /proc
      - path: /sys
      - path: /etc
      - path: /var/log
      - path: /opt/app
    deny:
      - path: /etc/shadow
      - path: /etc/gshadow
      - path: /root
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 60'

teardown() {
    echo "Tearing down data-collection demo containers..."
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

echo "=== RavenFabric Data Collection Demo ==="
echo ""
echo "Version:      ${VERSION}"
echo "Architecture: ${ARCH} (${BINARY_ARCH})"
echo "Relay port:   ${RELAY_PORT}"
echo ""

# --- 1. Create relay container ---
echo "[1/4] Starting relay container..."
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

# --- 2. Create agent containers with role-specific data ---
declare -A AGENT_TOKENS=( ["rf-collector"]="collector" ["rf-webserver"]="webserver" ["rf-database"]="database" )
declare -A AGENT_ROLES=( ["rf-collector"]="aggregator" ["rf-webserver"]="web" ["rf-database"]="db" )
STEP=2

for AGENT_NAME in rf-collector rf-webserver rf-database; do
    AGENT_TOKEN="${AGENT_TOKENS[$AGENT_NAME]}"
    AGENT_ROLE="${AGENT_ROLES[$AGENT_NAME]}"
    echo ""
    echo "[${STEP}/4] Starting agent container: ${AGENT_NAME} (token: ${AGENT_TOKEN}, role: ${AGENT_ROLE})..."

    docker run -d --name "$AGENT_NAME" "$IMAGE" sleep infinity > /dev/null
    docker exec "$AGENT_NAME" bash -c "
        apt-get update -qq && apt-get install -y -qq curl ca-certificates procps net-tools > /dev/null 2>&1 &&
        curl -fsSL -o /usr/local/bin/rf-agent '${BASE_URL}/ravenfabric-${BINARY_ARCH}-agent' &&
        chmod +x /usr/local/bin/rf-agent &&
        mkdir -p /etc/ravenfabric /var/log /opt/app &&
        cat > /etc/ravenfabric/policy.yaml << 'POLICYEOF'
${POLICY}
POLICYEOF
    "

    # Seed role-specific data for realistic demo
    if [[ "$AGENT_ROLE" == "web" ]]; then
        docker exec "$AGENT_NAME" bash -c "
            echo 'server { listen 80; root /var/www/html; }' > /etc/ravenfabric/nginx.conf &&
            mkdir -p /var/log &&
            for i in \$(seq 1 50); do
                echo \"192.168.1.\$((RANDOM % 254 + 1)) - - [\$(date '+%d/%b/%Y:%H:%M:%S %z')] \\\"GET /api/v1/data HTTP/1.1\\\" \$((RANDOM % 2 == 0 ? 200 : 404)) \$((RANDOM % 5000 + 100))\" >> /var/log/access.log
            done &&
            echo 'app_version: 2.1.0' > /opt/app/config.yaml &&
            echo 'environment: production' >> /opt/app/config.yaml &&
            echo 'max_connections: 1000' >> /opt/app/config.yaml
        "
    elif [[ "$AGENT_ROLE" == "db" ]]; then
        docker exec "$AGENT_NAME" bash -c "
            mkdir -p /var/log &&
            for i in \$(seq 1 30); do
                echo \"\$(date '+%Y-%m-%d %H:%M:%S') [INFO] Query completed in \$((RANDOM % 500))ms: SELECT * FROM users WHERE id = \$((RANDOM % 10000))\" >> /var/log/db-query.log
            done &&
            echo 'db_engine: postgresql' > /opt/app/config.yaml &&
            echo 'max_connections: 200' >> /opt/app/config.yaml &&
            echo 'shared_buffers: 256MB' >> /opt/app/config.yaml &&
            echo 'data_directory: /var/lib/postgresql/data' >> /opt/app/config.yaml
        "
    elif [[ "$AGENT_ROLE" == "aggregator" ]]; then
        docker exec "$AGENT_NAME" bash -c "
            mkdir -p /var/log /opt/app/reports &&
            echo 'collection_interval: 60' > /opt/app/config.yaml &&
            echo 'retention_days: 30' >> /opt/app/config.yaml &&
            echo 'export_format: json' >> /opt/app/config.yaml &&
            echo '{\"last_collection\":\"never\",\"agents_queried\":0}' > /opt/app/reports/status.json
        "
    fi

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
    STEP=$((STEP + 1))
done

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Containers:"
docker ps --filter name=rf- --format "  {{.Names}}\t{{.Image}}\t{{.Status}}"
echo ""
echo "Agents:"
echo "  rf-collector  (token: collector)  — data aggregation node"
echo "  rf-webserver  (token: webserver)  — simulated web server with access logs"
echo "  rf-database   (token: database)   — simulated database with query logs"
echo ""
echo "Policy: read-only data collection (no writes, no destructive commands)"
echo ""
echo "Usage:"
echo ""
echo "  # Build rf CLI (if you haven't already)"
echo "  cargo build --release -p rf-cli"
echo ""
echo "  # Collect system info from all agents"
echo "  rf --relay ws://127.0.0.1:${RELAY_PORT} exec --token webserver 'hostname && uname -a'"
echo ""
echo "  # Run scenarios"
echo "  ./scenarios/01-system-inventory.sh"
echo "  ./scenarios/02-resource-monitoring.sh"
echo "  ./scenarios/03-log-collection.sh"
echo "  ./scenarios/04-config-audit.sh"
echo "  ./scenarios/05-network-topology.sh"
echo "  ./scenarios/06-security-scan.sh"
echo "  ./scenarios/07-fleet-snapshot.sh"
echo "  ./scenarios/08-policy-boundary.sh"
echo ""
echo "  # NOTE: After each exec, the agent reconnects with a brief delay."
echo "  # Wait ~5s between consecutive commands to the same agent."
echo ""
echo "  # Teardown"
echo "  ./setup.sh teardown"
