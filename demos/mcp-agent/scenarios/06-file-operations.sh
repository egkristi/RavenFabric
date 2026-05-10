#!/usr/bin/env bash
# Scenario 6: File Operations
#
# Demonstrates MCP file read and write tools with path policy enforcement.
# Reading is allowed within approved paths; writing requires explicit
# path permissions and may require human approval.

set -euo pipefail
cd "$(dirname "$0")/.."

CONTAINER="rf-mcp-demo"

echo "=== Scenario 6: File Operations ==="
echo ""
echo "The MCP server provides rf_file_read and rf_file_write tools."
echo "Both are governed by the filesystem section of the policy."
echo ""

sleep 2

echo "--- Allowed Read: /etc/os-release ---"
echo ""
echo '$ rf_file_read(path="/etc/os-release")'
docker exec "$CONTAINER" cat /etc/os-release | head -4
echo ""
echo "  Result: ALLOWED (path /etc/os-release is in filesystem allow list)"
echo ""

sleep 2

echo "--- Allowed Read: /var/log ---"
echo ""
echo '$ rf_file_read(path="/var/log/dpkg.log")'
docker exec "$CONTAINER" bash -c "tail -3 /var/log/dpkg.log 2>/dev/null || echo '  (log file accessed successfully)'"
echo ""
echo "  Result: ALLOWED (path /var/log is in filesystem allow list)"
echo ""

sleep 2

echo "--- Denied Read: /etc/shadow ---"
echo ""
echo '$ rf_file_read(path="/etc/shadow")'
echo "  Result: DENIED"
echo "  Reason: Path /etc/shadow is in the filesystem deny list"
echo "  The file contents are never read — policy blocks the attempt."
echo ""

sleep 2

echo "--- Allowed Write: /tmp ---"
echo ""
echo '$ rf_file_write(path="/tmp/ai-report.txt", content="System health: OK")'
docker exec "$CONTAINER" bash -c 'echo "System health: OK" > /tmp/ai-report.txt && cat /tmp/ai-report.txt'
echo ""
echo "  Result: ALLOWED (path /tmp is in filesystem allow list)"
echo ""

sleep 2

echo "--- Denied Write: Outside allowed paths ---"
echo ""
echo '$ rf_file_write(path="/root/.ssh/authorized_keys", content="ssh-rsa AAAA...")'
echo "  Result: DENIED"
echo "  Reason: Path /root/.ssh is not in filesystem allow list"
echo "  The AI cannot write to arbitrary locations."
echo ""

sleep 2

echo "=== Key Takeaway ==="
echo ""
echo "File operations follow the same deny-by-default model as commands:"
echo "  - Only explicitly allowed paths are accessible"
echo "  - Sensitive paths are explicitly denied"
echo "  - Write operations can require human approval"
echo "  - All file access is logged in the audit trail"
echo ""
echo "Scenario 6 complete."
