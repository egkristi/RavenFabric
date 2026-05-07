# Conference Lightning Talk Proposal

## Title

**RavenFabric: Letting AI Agents Touch Your Servers (Safely)**

## Abstract (100 words)

AI coding assistants increasingly need to execute commands on real infrastructure. But SSH + Ansible weren't designed for autonomous agents making thousands of decisions per hour. RavenFabric is a Rust agent that enforces cryptographic mutual authentication (Noise XX) and deny-by-default policy on every command. In this talk, I'll demo an AI agent executing infrastructure commands through RavenFabric's policy engine — showing how commands get allowed, denied, and audit-logged in real-time. Single static binary, 43k LOC, 840 tests, zero TLS certificates.

## Target Conferences

- **NDC Oslo** (June) — "Security" or "DevOps" track
- **RustConf** (September) — systems programming audience
- **FOSDEM** (February) — Security devroom or Rust devroom

## Outline (5 minutes)

1. **Problem** (45s): AI agents want shell access. Current tools have no policy layer.
2. **Demo** (2m): Live `rf exec` showing allow/deny/audit in terminal
3. **Architecture** (1m): Noise XX, deny-by-default, audit log, MCP protocol
4. **Results** (45s): 43k LOC, 840 tests, single binary, works everywhere
5. **Call to action** (30s): GitHub link, try it today

## Speaker Bio

Building RavenFabric — a secure remote execution agent for AI-driven infrastructure. Background in systems security and distributed systems. Rust enthusiast.

## Technical Requirements

- Screen sharing for terminal demo
- No special hardware needed
- Demo runs entirely local (`rf dev` mode)
