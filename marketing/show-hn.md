# Show HN: RavenFabric — Secure remote execution for AI agents (Rust, Noise XX, deny-by-default)

RavenFabric is a single static binary that replaces Tailscale + Ansible + Salt for AI-driven infrastructure. It uses Noise XX mutual authentication (no TLS, no certificates) with a deny-by-default policy engine that ensures AI agents can only execute commands explicitly allowed by security policy.

**What makes it different:**

1. **Built for AI agents first.** Native MCP (Model Context Protocol) server lets Claude, Cursor, and other AI tools execute commands on remote machines — with every action policy-checked and audit-logged before execution.

2. **Zero-trust by default.** No command executes without passing through the policy engine. Policy is YAML-defined regex patterns. If it's not in the allow list, it's denied. Period.

3. **Single static binary.** One `curl | tar` and you have a working agent. No Python runtime, no Ruby gems, no Docker. Compiles for Linux (musl), macOS, Windows, ARM, and eventually WASM.

4. **Noise XX, not TLS.** Mutual key authentication on every connection. No certificate authorities, no certificate rotation nightmares. Both sides prove identity cryptographically.

5. **End-to-end encrypted relay.** The relay server never sees plaintext. It's a dumb pipe that routes encrypted frames between authenticated peers.

**Architecture:** 13 Rust crates, ~43k LOC, 840+ tests, zero clippy warnings. Async (Tokio), msgpack wire protocol, yamux multiplexing.

**Try it:**

```bash
# Start a local dev environment (relay + agent in one process)
rf dev

# Execute a command (policy-checked)
rf exec --token dev "uname -a"

# This gets denied by policy:
rf exec --token dev "rm -rf /"
```

**Links:**
- Website: https://ravenfabric.io
- GitHub: https://github.com/egkristi/RavenFabric
- MCP integration docs: see repo README

Built because existing tools (Ansible, Salt, Tailscale) weren't designed for a world where AI agents need to execute commands on your infrastructure with cryptographic guarantees and audit trails.
