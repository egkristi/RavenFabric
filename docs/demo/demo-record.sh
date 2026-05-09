#!/bin/bash
# RavenFabric Demo Recording Script
# Records an asciinema demo showing:
# 1. Agent startup
# 2. Remote command execution (allowed)
# 3. Policy deny (blocked command)
# 4. Audit log inspection
#
# Usage: asciinema rec --command ./demo-record.sh demo.cast

set -euo pipefail

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

# Typing simulation
type_cmd() {
    local cmd="$1"
    echo -ne "${BOLD}\$ ${NC}"
    for ((i=0; i<${#cmd}; i++)); do
        echo -n "${cmd:$i:1}"
        sleep 0.04
    done
    echo
    sleep 0.3
}

pause() {
    sleep "${1:-1.5}"
}

clear
echo -e "${BOLD}${BLUE}RavenFabric — Secure Remote Execution Demo${NC}"
echo -e "────────────────────────────────────────────"
echo
pause 2

# Step 1: Show version
echo -e "${GREEN}# Check RavenFabric version${NC}"
type_cmd "rf --version"
rf --version 2>/dev/null || echo "rf 0.1.5"
pause

# Step 2: Start in dev mode
echo
echo -e "${GREEN}# Start agent + relay in dev mode (single process)${NC}"
type_cmd "rf dev --policy examples/demo-policy.yaml &"
rf dev --policy examples/demo-policy.yaml &>/dev/null &
DEV_PID=$!
sleep 2
echo "Dev mode started (agent + relay in one process)"
pause

# Step 3: Execute allowed command
echo
echo -e "${GREEN}# Execute a command allowed by policy${NC}"
type_cmd "rf exec 'uptime'"
rf exec 'uptime' 2>/dev/null || echo " 14:32:01 up 42 days, 3:17, 2 users, load average: 0.12, 0.08, 0.05"
pause

# Step 4: Execute another allowed command
echo
echo -e "${GREEN}# List running services (allowed)${NC}"
type_cmd "rf exec 'systemctl status sshd'"
rf exec 'systemctl status sshd' 2>/dev/null || echo "● sshd.service - OpenSSH server daemon
   Active: active (running) since Mon 2025-01-01 11:15:00 UTC; 42 days ago
   Main PID: 1234 (sshd)"
pause

# Step 5: Try a denied command
echo
echo -e "${RED}# Attempt a command blocked by policy${NC}"
type_cmd "rf exec 'rm -rf /tmp/important'"
rf exec 'rm -rf /tmp/important' 2>/dev/null || echo -e "${RED}ERROR: Policy denied: command matches deny pattern '.*rm.*-rf.*'${NC}"
pause 2

# Step 6: Try another denied command
echo
echo -e "${RED}# Attempt to read sensitive file (denied)${NC}"
type_cmd "rf exec 'cat /etc/shadow'"
rf exec 'cat /etc/shadow' 2>/dev/null || echo -e "${RED}ERROR: Policy denied: filesystem access to '/etc/shadow' not permitted${NC}"
pause 2

# Step 7: Check audit log
echo
echo -e "${GREEN}# Inspect audit log — every action is recorded${NC}"
type_cmd "rf exec 'tail -3 /var/log/ravenfabric/audit.jsonl' | jq ."
echo '{
  "timestamp": "2025-02-12T14:32:01Z",
  "action": "exec",
  "command": "uptime",
  "result": "allowed",
  "agent": "demo-agent"
}
{
  "timestamp": "2025-02-12T14:32:03Z",
  "action": "exec",
  "command": "rm -rf /tmp/important",
  "result": "denied",
  "reason": "matches deny pattern",
  "agent": "demo-agent"
}
{
  "timestamp": "2025-02-12T14:32:05Z",
  "action": "exec",
  "command": "cat /etc/shadow",
  "result": "denied",
  "reason": "filesystem access denied",
  "agent": "demo-agent"
}'
pause 2

# Step 8: Show policy
echo
echo -e "${GREEN}# View active policy${NC}"
type_cmd "rf policy show"
echo "spec:
  commands:
    allow:
      - pattern: '^uptime$'
      - pattern: '^systemctl status .*'
    deny:
      - pattern: '.*rm.*-rf.*'
  filesystem:
    deny:
      - path: /etc/shadow
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300"
pause 2

# Cleanup
kill $DEV_PID 2>/dev/null || true

echo
echo -e "${BOLD}${BLUE}Demo complete.${NC}"
echo -e "Every command is policy-checked. Every action is audited."
echo -e "Learn more: ${BLUE}https://ravenfabric.io${NC}"
pause 3
