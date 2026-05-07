# Reddit Submission Drafts

## r/rust — Technical Focus

**Title:** RavenFabric: Secure remote execution agent in Rust (Noise XX, deny-by-default, MCP for AI agents)

**Body:**

I've been building RavenFabric — a secure remote execution and mesh networking agent that replaces the Tailscale + Ansible combo with a single static binary.

**Technical highlights:**

- 13-crate Cargo workspace, ~43k LOC, 840+ tests, 0 clippy warnings
- Noise_XX_25519_ChaChaPoly_BLAKE2s for mutual authentication (via `snow` crate)
- Custom wire protocol: 4-byte magic + version + encrypted frames (yamux multiplexed)
- Deny-by-default policy engine with regex-based command patterns
- WebSocket, QUIC, Memory, Unix socket, Named pipe, Vsock, MASQUE transports
- Desired-state convergence (like Salt states but in Rust)
- MCP (Model Context Protocol) server for AI agent integration
- Async via Tokio, msgpack serialization, structured audit logging

**Design decisions I'd love feedback on:**

1. Using Noise XX instead of TLS — eliminates CA infrastructure but means no browser compatibility. Worth it for agent-to-agent comms?
2. Single static musl binary vs. separate crates on crates.io — chose monorepo for now
3. Policy as YAML regex patterns — simple but effective. Should I add OPA/Rego?

GitHub: https://github.com/egkristi/RavenFabric

---

## r/selfhosted — Deployment Focus

**Title:** RavenFabric — Self-hosted secure remote execution (single binary, replaces Ansible+Tailscale for AI-driven ops)

**Body:**

After years of managing Ansible playbooks and Tailscale ACLs separately, I built RavenFabric — a single static binary that handles both secure connectivity AND command execution with built-in policy enforcement.

**What it replaces:**
- Tailscale/Headscale (secure connectivity) — Noise XX mutual auth, encrypted relay
- Ansible/Salt (remote execution) — command execution with policy checks
- ACL management — YAML deny-by-default policy (if it's not explicitly allowed, it's denied)

**Self-hosting setup:**

```bash
# Deploy the relay (stateless, never sees plaintext)
docker compose up -d

# On each machine, run the agent
rf-agent --relay wss://your-relay:9090 --policy policy.yaml

# Execute commands from your workstation
rf exec --target web-01 "systemctl status nginx"
```

**Key features for self-hosters:**
- Single binary, zero runtime deps (static musl on Linux)
- Relay is stateless — easy to scale/replace
- Every action audit-logged (JSON-lines, append-only)
- AI agent support via MCP (let Claude manage your infra safely)
- < 10MB memory idle, < 15MB binary

GitHub: https://github.com/egkristi/RavenFabric

---

## r/sysadmin — Operations Focus

**Title:** Built a tool that lets AI agents (Claude, Cursor) execute commands on servers with cryptographic policy enforcement

**Body:**

The problem: AI coding assistants want to run commands on your infrastructure, but existing tools (Ansible, SSH) have no concept of "this AI should only be allowed to run read-only commands."

RavenFabric solves this with:

1. **Deny-by-default policy** — YAML file defines exactly which commands are allowed via regex patterns. Everything else is blocked.
2. **Audit trail** — Every command attempt (allowed or denied) is logged with timestamp, caller identity, and policy decision.
3. **Mutual authentication** — Noise XX protocol means both sides cryptographically prove identity. No passwords, no SSH keys to rotate.
4. **MCP integration** — Native support for AI agent protocols (Claude, Cursor, Aider connect directly).

**Example policy:**
```yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl status .*"
      - pattern: "^journalctl.*"
      - pattern: "^docker ps$"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*shutdown.*"
```

Single static binary, deploys in seconds, works on Linux/macOS/Windows/ARM.

Anyone else dealing with "how do I let AI tools touch production safely"?

GitHub: https://github.com/egkristi/RavenFabric
