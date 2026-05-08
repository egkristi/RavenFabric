# Secure AI Agent Access

> **Scenario:** AI agents (coding assistants, operational agents, CI/CD
> automation) need real access to real systems — but cannot be fully trusted.
> RavenFabric provides command-level policy, cryptographic identity, approval
> workflows, and replay-grade audit that the agent cannot bypass.

---

## The Problem

AI agents in 2025-2026 execute terminal commands, manage infrastructure, query databases, and modify code. Each requires real access; each carries real risk:

- **Hallucination** — agents confidently execute wrong commands (`rm -rf /var/log/*` when asked to "clean up logs")
- **Prompt injection** — adversarial inputs in documents/repos/emails manipulate the agent into executing attacker-chosen commands
- **Scope creep** — "update the README" becomes "delete files I think are unused"
- **Lateral movement** — a compromised agent pivots to other systems at machine speed
- **Audit invisibility** — reconstructing what went wrong requires correlating logs from many tools
- **No human-in-loop** — the agent decides what doesn't need approval

### Why traditional mitigations fail

- **IAM roles** operate at API level, not command level — can't distinguish `kubectl get pods` from `kubectl delete namespace production`
- **Container sandboxes** are binary (block everything or allow everything within)
- **AI guardrails** catch `rm -rf /` but not context-dependent destructive commands
- **Manual review** defeats the purpose of automation

---

## How RavenFabric Addresses This

The core insight: **transport is incidental.** Whether the command travels over the network to a remote server or over a Unix socket to a local process, the same policy engine, audit log, and identity verification apply.

```
AI agent (Claude Code, Cursor, operational agent, CI/CD)
    │
    ├─ Path A: rf exec (CLI)     Path B: MCP tool call (rf-mcp-server)
    │
    ▼
RavenFabric policy layer
    ├─ Validate against agent-specific policy (deny-by-default)
    ├─ Check prompt injection heuristics (suspicion scoring)
    ├─ Apply approval workflow if required
    ├─ Enforce rate limits and resource quotas
    └─ Refuse if outside allowed scope
    │
    ▼
    ├─ Remote: Noise XX over network → rf-agent on target
    └─ Local: Noise XX over Unix socket → rf-agent on same machine
    │
    ▼
Complete audit trail:
    ├─ Command, policy decision, result
    ├─ Agent reasoning (optional `reason` parameter)
    └─ Cryptographically signed
```

| Capability | How |
|------------|-----|
| Cryptographic agent identity | Per-session Curve25519 keys (short-lived, auto-expiring) |
| Command-level policy | Allow/deny regex patterns on actual commands, not API level |
| Uniform local + remote | Same policy engine over Unix socket or network |
| Replay-grade audit | Every command, decision, result — with optional agent reasoning |
| Approval workflows | Sensitive ops escalate to human review (`rf_request_approval`) |
| Prompt injection detection | Heuristics, pattern library, evasion detection, suspicion scoring |
| Policy templates | Pre-built: coding-assistant, production-read-only, security-investigator, CI/CD, database-query |
| Blast radius limits | Resource quotas, timeout enforcement, output size limits |
| Capability delegation | Biscuit tokens with attenuation — never widen, only narrow |

---

## Two Integration Paths

### Path A: CLI (`rf exec`)

The agent uses `rf` as a shell tool, same as a human operator. Works with any agent that has shell access (Claude Code, Cursor, Aider, custom agents).

```bash
# Agent executes via rf instead of direct shell
$ rf exec local "cargo test"
$ rf exec prod-db-1 "psql -c 'SELECT count(*) FROM users'"
```

### Path B: MCP server (`rf-mcp-server`)

RavenFabric exposes an MCP server that AI clients connect to via stdio or HTTP+SSE. Structured tool calls instead of CLI parsing.

MCP tools available:

| Tool | Purpose |
|------|---------|
| `rf_exec` | Policy-validated command execution |
| `rf_query_policy` | Pre-flight check (would this command be allowed?) |
| `rf_request_approval` | Human-in-loop for sensitive operations |
| `rf_list_my_capabilities` | Dynamic capability discovery |
| `rf_file_read` / `rf_file_write` | Filesystem operations under path policy |

Claude Desktop integration:

```json
{
  "mcpServers": {
    "ravenfabric": {
      "command": "rf-mcp-server",
      "args": ["--stdio"]
    }
  }
}
```

Claude Code: `claude mcp add ravenfabric`

---

## Policy Configuration

```yaml
spec:
  commands:
    allow:
      # Build tools
      - pattern: "^cargo (build|test|check|fmt|clippy).*$"
      - pattern: "^npm (install|run|test|build).*$"
      - pattern: "^python -m pytest.*$"
      # Read-only inspection
      - pattern: "^git (status|log|diff|show|branch).*$"
      - pattern: "^ls .*$"
      - pattern: "^cat .*$"
      # Limited git mutations
      - pattern: "^git add .*$"
      - pattern: "^git commit -m .*$"

    deny:
      # Catastrophic
      - pattern: ".*rm -rf.*"
      - pattern: "^sudo .*"
      # Git destructive
      - pattern: "git push .*--force.*"
      - pattern: "git reset --hard.*"
      # Exfiltration
      - pattern: ".*curl.*\\|.*sh.*"

  filesystem:
    allow:
      - path: /home/user/project
      - path: /tmp
    deny:
      - path: /home/user/.ssh
      - path: /home/user/.aws
      - path: /etc/shadow

  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

### Pre-built templates

Instead of writing policy from scratch, use a template:

```bash
$ rf policy validate --template coding-assistant
```

Available templates: `coding-assistant`, `production-read-only`, `security-investigator`, `ci-cd-agent`, `database-query-agent`. Templates compose with deny-wins conflict resolution.

---

## Prompt Injection Defense

RavenFabric detects injection attempts at the policy layer — independent of the agent's reasoning:

| Defense | How it works |
|---------|-------------|
| Command heuristics | Detect base64/hex-encoded payloads, unicode homoglyphs |
| Pattern library | Known injection markers (instruction overrides, role-play triggers) |
| Evasion detection | String concatenation, variable indirection, eval patterns |
| Suspicion scoring | Cumulative per-session score; threshold triggers capability reduction |
| Configurable response | `block` (deny + audit), `flag` (allow + alert), `log` (allow + record) |

The policy layer doesn't care *why* the agent generated the command — only whether the command is permitted. Prompt injection that produces a denied command is blocked regardless.

---

## Example Workflows

### Agent audit trail

```bash
# All actions by the agent are recorded in structured audit log
# Each entry: timestamp, identity, command, policy decision, result, reasoning
$ cat ~/.local/share/ravenfabric/audit.jsonl | jq .

{"ts":"2026-05-05T14:32:01Z","identity":"claude-session-a3f2",
 "action":"exec","command":"cargo test","decision":"allowed",
 "exit_code":0,"reason":"User asked to run tests"}

{"ts":"2026-05-05T14:34:02Z","identity":"claude-session-a3f2",
 "action":"exec","command":"git push origin main","decision":"denied",
 "rule":"approval.required","reason":"Pushing to main after fix"}
```

### Approval workflow

```bash
# Agent tries to push — policy requires human approval
$ rf exec local "git push origin main"
DENIED: requires approval (rule: approval_required for git push)

# Agent calls rf_request_approval via MCP
# User sees notification, approves
# Agent retries — now allowed
```

### Incident reconstruction

```bash
# What did the agent do in the last hour?
# Audit log contains full chain: command → policy decision → result
# Including agent reasoning if the `reason` field was provided
# EU AI Act traceability: per-agent decision log with human oversight records
```

---

## Multi-Agent Patterns

| Pattern | Use case | Identity lifecycle |
|---------|----------|-------------------|
| Per-session ephemeral | Developer coding assistant | Auto-expires after session |
| Long-lived operational | Production monitoring agent | Revocable, no expiry |
| Per-user in shared env | SaaS coding tool (multi-user) | Per-user isolation |
| Delegated | "My agent can deploy to staging for 2h" | Time-bounded, attenuated |

---

## Compliance

RavenFabric's AI agent audit trail addresses:

| Framework | What RavenFabric provides |
|-----------|--------------------------|
| EU AI Act | Per-agent decision log with reasoning, human oversight records |
| NIS2 / DORA | Per-identity traceability, tamper-evident audit |
| HIPAA | Access logs for systems handling PHI |
| SOX | Traceability for systems affecting financial reporting |

---

## Comparison with Alternatives

| Feature | Container sandbox | OS sandbox | API-only access | RavenFabric |
|---------|-------------------|------------|-----------------|-------------|
| Command-level policy | No | Syscall only | No | Yes |
| Same policy local + remote | No | No | N/A | Yes |
| Cryptographic identity | No | No | Token-based | Yes |
| Approval workflow | No | No | No | Yes |
| Prompt injection detection | No | No | No | Yes |
| Replay-grade audit | No | Partial | Partial | Yes |
| Policy templates | No | No | N/A | Yes |
| Human-in-loop | No | No | No | Yes |

---

## Implementation Status

| Feature | Status | Notes |
|---------|--------|-------|
| Cryptographic agent identity | Done | Per-session Curve25519 keys |
| Policy-validated execution | Done | Deny-by-default, regex patterns |
| Structured audit logging | Done | JSON-lines, signed |
| Noise XX encryption | Done | Uniform local + remote |
| Unix socket transport | Done | `UnixSocketDriver` for local-to-local |
| Stdio transport | Done | `StdioDriver` for MCP stdio mode |
| `rf-mcp-server` binary | Done | Production-hardened, fuzz-tested |
| MCP tools (exec, query, approval, capabilities, file) | Done | 6 tools |
| Per-session identity | Done | Short-lived keys per MCP session |
| Agent reasoning capture | Done | Optional `reason` in audit |
| Rate limiting per session | Done | Sliding window throttle |
| Prompt injection detection | Done | Heuristics, patterns, suspicion scoring |
| Policy templates | Done | 5 templates with composition |
| Capability delegation (Biscuit) | Done | Attenuation, offline-verifiable |
| Anomaly / suspicion scoring | Done | Auto capability reduction on threshold |
| EU AI Act traceability | Done | Decision log, human oversight records |
| Incident reconstruction | Done | Timeline view with reasoning |
| Claude Code integration | Done | `claude mcp add` one-liner |
| Cursor integration | Done | MCP server config |
| MCP client SDKs | Done | Rust, Python, TypeScript |
| Network destination policy | Planned | Per-host allow/deny |
| Environment variable policy | Planned | Key-level allow/deny |

---

## See Also

- [CloudNativePG](cloudnativepg.md) — Database access in Kubernetes
- [Air-Gapped ICS](airgapped-ics.md) — Strict access control for sensitive environments
- [MSP Multi-Tenant](msp-multitenant.md) — Per-client isolation
- [Edge & IoT Fleet Management](edge-iot-fleet.md) — Large-scale device fleet access
