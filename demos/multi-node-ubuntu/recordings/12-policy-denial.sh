#!/usr/bin/env bash
# Recording: Policy Denial
# Run inside: asciinema rec --command "bash recordings/12-policy-denial.sh"
source "$(dirname "$0")/helpers.sh"

RELAY="${RF_RELAY:-ws://127.0.0.1:9091}"

clear
section "RavenFabric — Policy Denial"

comment "Every command is checked against the agent's deny-by-default policy"
comment "Let's apply a restrictive policy and see it in action"
sleep 1

# Apply restrictive policy
comment "Apply a restrictive policy (only hostname, uname, uptime allowed)"
RELAY_IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' rf-relay)
docker exec rf-agent-1 bash -c 'cat > /etc/ravenfabric/policy.yaml << "PEOF"
spec:
  commands:
    allow:
      - pattern: "^hostname$"
      - pattern: "^uname.*"
      - pattern: "^uptime$"
      - pattern: "^cat /var/log/rf-audit\\.jsonl.*"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*curl.*"
      - pattern: ".*wget.*"
      - pattern: ".*shutdown.*"
      - pattern: ".*apt.*"
      - pattern: ".*chmod.*"
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30
PEOF'
docker exec rf-agent-1 bash -c 'pkill rf-agent || true'
sleep 1
docker exec -d rf-agent-1 bash -c "RUST_LOG=warn rf-agent \
    --relay ws://${RELAY_IP}:9090 --id rf-agent-1 --token agent1 \
    --policy-path /etc/ravenfabric/policy.yaml \
    --audit-path /var/log/rf-audit.jsonl \
    --key-path /etc/ravenfabric/agent.key \
    > /var/log/rf-agent.log 2>&1"
sleep 4
echo "  Restrictive policy applied and agent restarted."
echo ""
sleep 1

comment "ALLOWED — hostname (matches allow pattern)"
run_cmd "rf --relay $RELAY exec --token agent1 'hostname'"
sleep 5

comment "ALLOWED — uname -a (matches allow pattern)"
run_cmd "rf --relay $RELAY exec --token agent1 'uname -a'"
sleep 5

comment "DENIED — rm -rf / (blocked by deny pattern)"
run_cmd "rf --relay $RELAY exec --token agent1 'rm -rf /' || echo '  BLOCKED by policy'"
sleep 5

comment "DENIED — curl (network access blocked)"
run_cmd "rf --relay $RELAY exec --token agent1 'curl http://example.com' || echo '  BLOCKED by policy'"
sleep 5

comment "DENIED — apt install (package management blocked)"
run_cmd "rf --relay $RELAY exec --token agent1 'apt install -y nmap' || echo '  BLOCKED by policy'"
sleep 5

comment "Inspect audit log — every denial is recorded"
run_cmd "rf --relay $RELAY exec --token agent1 'cat /var/log/rf-audit.jsonl | tail -5'"
sleep 5

# Restore permissive policy
docker exec rf-agent-1 bash -c 'cat > /etc/ravenfabric/policy.yaml << "PEOF"
spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxOutputBytes: 104857600
    timeoutSeconds: 300
PEOF'
docker exec rf-agent-1 bash -c 'pkill rf-agent || true'
sleep 1
docker exec -d rf-agent-1 bash -c "RUST_LOG=warn rf-agent \
    --relay ws://${RELAY_IP}:9090 --id rf-agent-1 --token agent1 \
    --policy-path /etc/ravenfabric/policy.yaml \
    --audit-path /var/log/rf-audit.jsonl \
    --key-path /etc/ravenfabric/agent.key \
    > /var/log/rf-agent.log 2>&1"

section "Deny by default — allow by policy — audit everything"
sleep 2
