#!/usr/bin/env bash
# Comprehensive RavenFabric demo script for asciinema recording.
# Simulates human typing for a polished recording.
#
# Run inside: asciinema rec --command "bash demos/demo-script.sh"
#
# Prerequisites:
#   - cargo build --release -p rf-cli -p rf-agent -p rf-relay
#   - asciinema installed (pipx install asciinema)
#
# Output: demos/ravenfabric-demo.cast

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RF="$ROOT/target/release/rf"

# ── Typing simulation ──────────────────────────────────────────
type_cmd() {
    local cmd="$1"
    local delay="${2:-0.04}"
    printf '\033[1;32m$\033[0m '
    for ((i=0; i<${#cmd}; i++)); do
        printf '%s' "${cmd:$i:1}"
        sleep "$delay"
    done
    echo ""
    sleep 0.3
}

run_cmd() {
    type_cmd "$1"
    eval "$1"
    echo ""
    sleep "${2:-1.5}"
}

section() {
    echo ""
    printf '\033[1;36m  %s\033[0m\n' "$1"
    echo "  $(printf '%.0s─' {1..55})"
    echo ""
    sleep 1.5
}

comment() {
    printf '\033[0;90m  # %s\033[0m\n' "$1"
    sleep 0.5
}

pause() {
    sleep "${1:-1.5}"
}

# ── Clean start ────────────────────────────────────────────────
clear
echo ""
printf '\033[1;36m  RavenFabric v1.0.0-rc.6\033[0m — Security-first remote execution & mesh networking\n'
echo "  $(printf '%.0s─' {1..55})"
echo ""
sleep 2

# ════════════════════════════════════════════════════════════════
# SECTION 1: Binary & Version Info
# ════════════════════════════════════════════════════════════════
section "1. Single Static Binary"

comment "One binary, zero dependencies — replaces Tailscale, Ansible, Salt, NetBird"
run_cmd "ls -lh $ROOT/target/release/rf | awk '{print \"  rf CLI:\", \$5}'"
run_cmd "ls -lh $ROOT/target/release/rf-agent | awk '{print \"  rf-agent:\", \$5}'"
run_cmd "ls -lh $ROOT/target/release/rf-relay | awk '{print \"  rf-relay:\", \$5}'"

comment "Check version"
run_cmd "$RF --version"

# ════════════════════════════════════════════════════════════════
# SECTION 2: Dev Mode (Zero-Setup)
# ════════════════════════════════════════════════════════════════
section "2. Dev Mode — Zero-Setup Local Testing"

comment "rf dev starts a relay + agent in one process — no config needed"
type_cmd "rf dev &"
"$RF" dev > /tmp/rf-dev.log 2>&1 &
RF_DEV_PID=$!
sleep 2
printf '\033[0;32m  ✓ relay + agent running on ws://127.0.0.1:9090\033[0m\n'
echo ""
sleep 1

comment "Execute a command — E2E encrypted via Noise XX handshake"
run_cmd "$RF exec --token dev 'echo \"Hello from RavenFabric\" && hostname'"

comment "Streaming execution — real-time output"
run_cmd "$RF exec --stream --token dev 'for i in 1 2 3; do echo \"Line \$i\"; sleep 0.5; done'"

comment "Background execution — fire and forget with audit reason"
run_cmd "$RF exec --background --reason 'scheduled maintenance check' --token dev 'sleep 2 && echo \"Background task done\"'"

# ════════════════════════════════════════════════════════════════
# SECTION 3: Policy Enforcement
# ════════════════════════════════════════════════════════════════
section "3. Policy Enforcement — Deny-by-Default"

comment "Allowed commands execute normally"
run_cmd "$RF exec --token dev 'uname -a'"

comment "Denied commands are blocked with a clear policy violation message"
run_cmd "$RF exec --token dev 'rm -rf /' 2>&1 || true"

comment "Shell injection attempts are detected and blocked"
run_cmd "$RF exec --token dev 'cat /etc/shadow' 2>&1 || true"

comment "Policy templates — list available templates"
run_cmd "$RF policy list"

comment "Show a policy template"
run_cmd "$RF policy show production-read-only"

comment "Compose multiple templates (deny-wins conflict resolution)"
run_cmd "$RF policy compose production-read-only,ci-cd-agent 2>&1 || echo '  (compose merges two policies, deny-wins)'"

comment "Validate a policy file"
run_cmd "$RF policy validate --template production-read-only"

comment "Lint for dangerous patterns"
run_cmd "$RF policy lint --template production-read-only"

# ════════════════════════════════════════════════════════════════
# SECTION 4: Interactive Shell
# ════════════════════════════════════════════════════════════════
section "4. Interactive Shell"

comment "Full PTY shell — like SSH, but Noise XX authenticated"
type_cmd "$RF shell --token dev"
printf '\033[0;33m  (interactive — opens a full remote PTY shell with history, tab completion)\033[0m\n'
echo ""
sleep 1

# ════════════════════════════════════════════════════════════════
# SECTION 5: Port Forwarding
# ════════════════════════════════════════════════════════════════
section "5. Port Forwarding — Encrypted Tunnels"

comment "Local port forwarding (like SSH -L)"
type_cmd "$RF forward --token dev --local 127.0.0.1:8080 --remote localhost:80"
printf '\033[0;33m  (localhost:8080 → agent:80 via encrypted tunnel)\033[0m\n'
echo ""
sleep 1

comment "Remote port forwarding (like SSH -R)"
type_cmd "$RF forward --token dev --local 127.0.0.1:9090 --remote localhost:9090"
printf '\033[0;33m  (agent → localhost:9090 via encrypted tunnel)\033[0m\n'
echo ""
sleep 1

# ════════════════════════════════════════════════════════════════
# SECTION 6: File Transfer
# ════════════════════════════════════════════════════════════════
section "6. File Transfer — Encrypted Copy"

comment "Create a test file to transfer"
run_cmd "echo 'RavenFabric secure config v1.0' > /tmp/demo-config.txt"

comment "Push a file to the remote agent"
run_cmd "$RF cp --token dev /tmp/demo-config.txt agent:/tmp/remote-config.txt 2>&1 || echo '  (file transfer requires a full agent — works with rf-agent binary)'"

comment "Pull a file from the remote agent"
type_cmd "$RF cp --token dev agent:/tmp/remote-config.txt /tmp/pulled-config.txt"
printf '\033[0;33m  (bidirectional — push and pull with delta sync support)\033[0m\n'
echo ""
sleep 1

comment "Delta sync — only transfer changed blocks (rsync-like)"
type_cmd "$RF cp --delta --token dev /tmp/demo-config.txt agent:/tmp/remote-config.txt"
printf '\033[0;33m  (rolling checksum — only sends changed blocks)\033[0m\n'
echo ""
sleep 1

comment "Recursive directory copy"
type_cmd "$RF cp --recursive --token dev /tmp/demo-dir agent:/tmp/remote-dir"
printf '\033[0;33m  (recursive directory copy with subdirectory support)\033[0m\n'
echo ""
sleep 1

comment "Clean up temp files"
run_cmd "rm -rf /tmp/demo-config.txt /tmp/demo-dir"

# ════════════════════════════════════════════════════════════════
# SECTION 7: TCP Proxy
# ════════════════════════════════════════════════════════════════
section "7. TCP Proxy — Transparent Tunnel"

comment "Open a TCP proxy through the agent to any target"
type_cmd "$RF proxy --token dev --target example.com:80 --listen 8888"
printf '\033[0;33m  (localhost:8888 → agent → example.com:80)\033[0m\n'
echo ""
sleep 1

# ════════════════════════════════════════════════════════════════
# SECTION 8: Secret Management
# ════════════════════════════════════════════════════════════════
section "8. Secret Management — Encrypted at Rest"

comment "Push a secret to the agent (sealed, never returned)"
run_cmd "$RF secret push --token dev --name db_password --value 's3cret!' 2>&1 || echo '  (secret store requires a configured agent with key file)'"

comment "List secrets (names only — values never leave the agent)"
type_cmd "$RF secret list --token dev"
printf '\033[0;33m  (secrets are sealed at rest, values never returned)\033[0m\n'
echo ""
sleep 1

# ════════════════════════════════════════════════════════════════
# SECTION 9: Audit Logging
# ════════════════════════════════════════════════════════════════
section "9. Audit Logging — Every Action Recorded"

comment "Every RPC action produces a structured JSON audit entry"
comment "Derive an HMAC audit key from the agent identity key"
run_cmd "$RF audit derive-key client.key"

comment "Audit log verification — HMAC chain integrity check"
type_cmd "$RF audit verify --key-file <hex-key> /var/log/ravenfabric/audit.jsonl"
printf '\033[0;33m  (tamper-proof HMAC chain — any modification is detected)\033[0m\n'
echo ""
sleep 1

# ════════════════════════════════════════════════════════════════
# SECTION 10: Agent Status & Health
# ════════════════════════════════════════════════════════════════
section "10. Agent Status & Health"

comment "Check agent connection status — shows peer key, uptime, version"
run_cmd "$RF status --token dev"

# ════════════════════════════════════════════════════════════════
# SECTION 11: Shell Completions
# ════════════════════════════════════════════════════════════════
section "11. Shell Completions"

comment "Generate shell completions for bash, zsh, fish, or PowerShell"
run_cmd "$RF completions bash | head -5"

# ════════════════════════════════════════════════════════════════
# SECTION 12: Playbook Orchestration
# ════════════════════════════════════════════════════════════════
section "12. Playbook Orchestration — Multi-Agent"

comment "Execute a YAML playbook across multiple agents"
type_cmd "$RF playbook --token dev demos/multi-node-ubuntu/scenarios/playbooks/parallel-update.yaml"
printf '\033[0;33m  (parallel, sequential, and canary deployment strategies)\033[0m\n'
echo ""
sleep 1

# ════════════════════════════════════════════════════════════════
# SECTION 13: Direct Connect (No Relay)
# ════════════════════════════════════════════════════════════════
section "13. Direct Connect — No Relay Needed"

comment "Point-to-point connection — bypasses the relay entirely"
type_cmd "$RF --connect ws://agent:9999 exec --token unused 'hostname'"
printf '\033[0;33m  (direct Noise XX handshake, no intermediary)\033[0m\n'
echo ""
sleep 1

# ════════════════════════════════════════════════════════════════
# SECTION 14: Cross-Platform Compatibility Mode
# ════════════════════════════════════════════════════════════════
section "14. Cross-Platform Compatibility Mode"

comment "Use --compat-mode when connecting across platforms (e.g., macOS → Linux)"
type_cmd "$RF --compat-mode exec --token dev 'uname -a'"
printf '\033[0;33m  (resolves Noise XX handshake differences between platforms)\033[0m\n'
echo ""
sleep 1

# ════════════════════════════════════════════════════════════════
# SECTION 15: Transport Diversity
# ════════════════════════════════════════════════════════════════
section "15. Transport Diversity — Any Channel Works"

comment "RavenFabric runs over any byte-moving channel:"
echo ""
printf '\033[0;90m    • WebSocket  (TCP, default)\033[0m\n'
printf '\033[0;90m    • QUIC       (UDP, low-latency)\033[0m\n'
printf '\033[0;90m    • UNIX socket (IPC, local)\033[0m\n'
printf '\033[0;90m    • Stdio pipe  (parent-child process)\033[0m\n'
printf '\033[0;90m    • Memory     (in-process, testing)\033[0m\n'
printf '\033[0;90m    • LoRa, BLE, AX.25, satellite, mixnet\033[0m\n'
echo ""
sleep 2

# ════════════════════════════════════════════════════════════════
# SECTION 16: Platform Reach
# ════════════════════════════════════════════════════════════════
section "16. Run Anywhere"

comment "One binary, every platform:"
echo ""
printf '\033[0;90m    • Linux (x86_64, arm64, armv7, riscv64)\033[0m\n'
printf '\033[0;90m    • macOS (Intel + Apple Silicon)\033[0m\n'
printf '\033[0;90m    • Windows (x86_64)\033[0m\n'
printf '\033[0;90m    • FreeBSD, Android, iOS\033[0m\n'
printf '\033[0;90m    • WASM/WASI, OpenWrt, ESP32\033[0m\n'
printf '\033[0;90m    • Static musl — no libc dependency\033[0m\n'
echo ""
sleep 2

# ════════════════════════════════════════════════════════════════
# SECTION 17: MCP Integration
# ════════════════════════════════════════════════════════════════
section "17. AI Agent Integration (MCP)"

comment "RavenFabric provides an MCP server for AI agents"
type_cmd "$RF exec --token dev 'rf-mcp-server --transport stdio'"
printf '\033[0;33m  (AI agents use MCP tools: rf_exec, rf_file_read, rf_policy_check)\033[0m\n'
echo ""
sleep 1

comment "MCP client SDK — build MCP-aware applications in Rust"
type_cmd "cargo add rf-mcp-client"
printf '\033[0;33m  (Rust library for building MCP-aware applications)\033[0m\n'
echo ""
sleep 1

# ════════════════════════════════════════════════════════════════
# SECTION 18: Desired-State Convergence
# ════════════════════════════════════════════════════════════════
section "18. Desired-State Convergence"

comment "Declarative resource management — detect and fix drift"
type_cmd "$RF exec --token dev 'rf desired-state apply /etc/ravenfabric/desired-state.yaml'"
printf '\033[0;33m  (packages, files, services, sysctl — auto-remediation)\033[0m\n'
echo ""
sleep 1

# ════════════════════════════════════════════════════════════════
# SECTION 19: Cleanup
# ════════════════════════════════════════════════════════════════
section "19. Cleanup"

comment "Stop dev mode"
kill "$RF_DEV_PID" 2>/dev/null || true
wait "$RF_DEV_PID" 2>/dev/null || true
printf '\033[0;32m  ✓ dev mode stopped\033[0m\n'
echo ""
sleep 1

# ════════════════════════════════════════════════════════════════
# SUMMARY
# ════════════════════════════════════════════════════════════════
echo ""
echo "  $(printf '%.0s═' {1..55})"
echo ""
printf '\033[1;36m  RavenFabric v1.0.0-rc.6\033[0m — Security-first distributed execution\n'
echo ""
printf '\033[0;37m  ✓ Noise XX mutual authentication (no TLS, no certificates)\033[0m\n'
printf '\033[0;37m  ✓ Deny-by-default policy engine\033[0m\n'
printf '\033[0;37m  ✓ Structured audit logging (every action recorded)\033[0m\n'
printf '\033[0;37m  ✓ End-to-end encryption (relay never decrypts)\033[0m\n'
printf '\033[0;37m  ✓ Single static binary, zero dependencies\033[0m\n'
printf '\033[0;37m  ✓ Runs everywhere — Linux, macOS, Windows, IoT\033[0m\n'
printf '\033[0;37m  ✓ Any transport — WebSocket, QUIC, LoRa, BLE, satellite\033[0m\n'
printf '\033[0;37m  ✓ AI-ready — MCP server for agent integration\033[0m\n'
printf '\033[0;37m  ✓ Cross-platform compatibility mode\033[0m\n'
echo ""
printf '\033[0;37m  github.com/egkristi/RavenFabric\033[0m\n'
echo ""
sleep 4
