#!/usr/bin/env bash
# Record all asciinema demos for the multi-node-ubuntu scenarios.
#
# Usage: ./recordings/record-all.sh
#
# Prerequisites:
#   - asciinema installed (pipx install asciinema)
#   - ../setup.sh has been run (containers are up)
#   - rf CLI is built (cargo build --release -p rf-cli)
#
# Output: recordings/*.cast files

set -euo pipefail
cd "$(dirname "$0")/.."

RECORDINGS_DIR="recordings"
mkdir -p "$RECORDINGS_DIR"

# Check prerequisites
if ! command -v asciinema &> /dev/null; then
    echo "Error: asciinema not found. Install with: pipx install asciinema"
    exit 1
fi

if ! docker ps --filter name=rf-relay --format '{{.Names}}' | grep -q rf-relay; then
    echo "Error: Demo containers not running. Run ./setup.sh first."
    exit 1
fi

SCENARIOS=(
    "01-standard-exec:Standard Remote Execution"
    "02-streaming-exec:Streaming Execution"
    "03-background-exec:Background Execution"
    "04-interactive-shell:Interactive Shell"
    "05-orchestrated-exec:Orchestrated Multi-Agent Execution"
    "06-local-forward:Local Port Forwarding"
    "09-policy-enforcement:Policy Enforcement"
    "10-audit-inspection:Audit Log Inspection"
    "11-fleet-operations:Multi-Agent Fleet Operations"
)

echo "=== Recording Asciinema Demos ==="
echo ""

for entry in "${SCENARIOS[@]}"; do
    name="${entry%%:*}"
    title="${entry#*:}"
    cast_file="${RECORDINGS_DIR}/${name}.cast"
    script="${RECORDINGS_DIR}/${name}.sh"

    if [[ ! -f "$script" ]]; then
        echo "  SKIP: $name (no recording script)"
        continue
    fi

    echo "  Recording: $name — $title"
    asciinema rec "$cast_file" \
        --title "RavenFabric — ${title}" \
        --cols 100 \
        --rows 30 \
        --command "bash $script" \
        --overwrite \
        2>/dev/null

    echo "    Saved: $cast_file"
    echo ""

    # Wait for agent reconnection between recordings
    sleep 8
done

echo "=== All recordings complete ==="
echo ""
echo "Cast files:"
ls -lh "$RECORDINGS_DIR"/*.cast 2>/dev/null || echo "  No cast files found."
echo ""
echo "Play a recording: asciinema play recordings/<name>.cast"
echo "Upload:           asciinema upload recordings/<name>.cast"
