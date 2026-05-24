# MCP / AI Agent Demo

End-to-end demonstration of RavenFabric's MCP (Model Context Protocol) server enabling AI agents to execute commands through policy-bounded, human-approved workflows.

## Architecture

```text
┌───────────────────┐     stdio/SSE      ┌──────────────────────┐
│  AI Agent (Claude, │ ◄───────────────► │  rf-mcp-server       │
│  GPT, custom LLM)  │    MCP protocol    │  (policy + approval) │
└───────────────────┘                    └──────┬───────────────┘
                                                │ local exec
                                                ▼
                                         ┌──────────────┐
                                         │  Target host │
                                         │  (this node) │
                                         └──────────────┘
```

The MCP server exposes 8 tools to AI agents, all governed by deny-by-default policy
and optional human approval for sensitive operations.

## MCP Tools

| Tool | Description | Approval Required |
|------|-------------|-------------------|
| `rf_exec` | Execute command on target system | Optional (per policy) |
| `rf_query_policy` | Dry-run check if command is allowed | No |
| `rf_file_read` | Read file contents (path-policy enforced) | No |
| `rf_file_write` | Write file contents | Yes (configurable) |
| `rf_list_my_capabilities` | Discover what the AI is allowed to do | No |
| `rf_audit_query` | Query audit log for recent actions | No |
| `rf_request_approval` | Request human approval for sensitive ops | N/A (initiates) |
| `rf_check_approval` | Poll approval status | No |

## Prerequisites

- Docker
- `rf` CLI binary (or `cargo build --release -p rf-cli`)
- `rf-mcp-server` binary (or `cargo build --release -p rf-mcp-server`)

## Quick Start

```bash
chmod +x setup.sh
./setup.sh
```

## Scenarios

| # | Scenario | Script | Description |
|---|----------|--------|-------------|
| 1 | Policy Discovery | `scenarios/01-policy-discovery.sh` | AI agent discovers its capabilities via `rf_list_my_capabilities` |
| 2 | Safe Execution | `scenarios/02-safe-execution.sh` | Execute allowed commands (read-only) without approval |
| 3 | Policy Denial | `scenarios/03-policy-denial.sh` | AI attempts denied commands; policy blocks execution |
| 4 | Human Approval | `scenarios/04-human-approval.sh` | Full approval workflow: request, review, approve/deny, execute |
| 5 | Audit Trail | `scenarios/05-audit-trail.sh` | Every AI action logged; query and verify audit entries |
| 6 | File Operations | `scenarios/06-file-operations.sh` | Read and write files through MCP with path policy enforcement |

## Teardown

```bash
./setup.sh teardown
```
