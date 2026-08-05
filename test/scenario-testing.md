# RavenFabric Complete Scenario Testing

> Date: 2026-08-05 | v1.0.0-rc.6 | K3s | `ravenfabric`

---

## Test Infrastructure

4 distro pods, agents in direct-listen mode (`--listen 0.0.0.0:9999`, permissive policy):

| Pod | OS | CPU | Agent |
|-----|-----|-----|-------|
| t-ubuntu | Ubuntu 24.04 LTS | amd64 | ✅ |
| t-debian | Debian 12 | amd64 | ✅ |
| t-fedora | Fedora 41 | amd64 | ✅ |
| t-rocky | Rocky Linux 9.3 | amd64 | ✅ |

CLI connects via `kubectl port-forward` → `--connect ws://127.0.0.1:9999`.

---

## Complete Feature Matrix (29 scenarios)

### Execution

| # | Feature | Status | Details |
|---|---------|--------|---------|
| 1 | `rf exec` simple | ✅ | 0-3ms across all distros |
| 2 | `rf exec` stream | ✅ | `for i in 1 2 3; do echo $i; done` confirmed |
| 3 | `rf exec` background | ✅ | Returns job ID, runs in background (PID tracked) |
| 4 | `rf exec` exit code | ⚠️ | Agent log records exit_code:42, shell gets 0 |
| 5 | `rf shell` | ⚠️ | CLI exists. PTY not available in K8s test chain |

### File Operations

| # | Feature | Status | Details |
|---|---------|--------|---------|
| 6 | `rf cp` pull | ✅ | 50KB binary, SHA-256 verified both sides |
| 7 | `rf cp` push | ✅ | "TEST DATA" confirmed on agent |
| 8 | `rf cp` chunked | ✅ | 256KB default, --chunk-size, --recursive, --delta |

### Networking

| # | Feature | Status | Details |
|---|---------|--------|---------|
| 9 | `rf forward` | ❌ | K8s port-forward chain instability |
| 10 | `rf proxy` TCP | ⚠️ | CLI available (TCP + HTTP mode, audit per request) |

### Orchestration

| # | Feature | Status | Details |
|---|---------|--------|---------|
| 11 | `rf playbook` | ⚠️ | YAML schema in CLI. Blocked by relay handshake bug |
| 12 | Multi-agent exec | ❌ | Blocked by relay mode (snow-0.10.0) |

### Secrets

| # | Feature | Status | Details |
|---|---------|--------|---------|
| 13 | `rf secret push` | ⚠️ | Requires --seal-key-path on agent |
| 14 | `rf secret list` | ⚠️ | Requires --seal-key-path on agent |

### CLI & Developer Tools

| # | Feature | Status | Details |
|---|---------|--------|---------|
| 15 | `rf status` | ⚠️ | CLI exists, not runtime tested |
| 16 | `rf completions` | ✅ | bash, fish, zsh, elvish, powershell |
| 17 | `rf policy list` | ✅ | 6 templates: coding-assistant, prod-read-only, security-investigator, ci-cd-agent, database-query, safe-dev-mode |
| 18 | `rf policy show` | ✅ | Full YAML per template |
| 19 | `rf policy lint` | ✅ | 6 check categories via --file or --template |
| 20 | `rf policy validate` | ✅ | Schema validation for policy YAML |

### Audit & Security

| # | Feature | Status | Details |
|---|---------|--------|---------|
| 21 | `rf audit derive-key` | ✅ | HKDF-SHA256, confirmed output |
| 22 | `rf audit verify` | ⚠️ | CLI exists, needs HMAC-chained log |
| 23 | Policy deny-by-default | ✅ | 3 allowed, 3 denied, 1 implicit-deny |
| 24 | Immutable deny rules | ✅ | `immutable_deny:rm -rf /` cannot be overridden |
| 25 | Audit logging | ✅ | JSON-lines, HMAC-chained, buffered writer |
| 26 | `--reason` flag | ⚠️ | CLI option exists, not tested |

### Transport & Platform

| # | Feature | Status | Details |
|---|---------|--------|---------|
| 27 | Direct WebSocket | ✅ | All 4 distros, 0-3ms |
| 28 | Relay mode | ❌ | snow-0.10.0 handshake timeout (critical) |
| 29 | Multi-distro portability | ✅ | Ubuntu, Debian, Fedora, Rocky all pass |

---

## Audit Log Sample (Restrictive Policy)

```
allowed  | hostname                | ^hostname$
allowed  | echo ALLOWED_TEST       | ^echo .*$
denied   | rm -rf /                | immutable_deny:rm -rf /
denied   | curl http://example.com | ^curl .*$
denied   | apt install nmap        | ^apt .*$
denied   | ls /tmp/                | implicit-deny
allowed  | echo OK                 | ^echo .*$
```

---

## Policy Templates (6 built-in)

| Template | Use Case |
|----------|----------|
| `coding-assistant` | AI coding tools — git, npm, cargo, python, docker |
| `production-read-only` | Status checks, logs, metrics. Denies all writes |
| `security-investigator` | Forensics, log analysis, broad read access |
| `ci-cd-agent` | Build, test, deploy. Production push requires approval |
| `database-query` | SELECT only. DML denied, schema changes require approval |
| `safe-dev-mode` | Drop-in safe mode for Claude Code, Cursor, Aider |

---

## Bugs Found

**PERSISTENT — relay handshake:** snow-0.10.0 Noise XX timeout blocks playbooks, multi-agent exec, mesh VPN.

**NEW — exit code:** `rf exec "exit 42"` → shell exit code is 0. Real exit code only in audit log.

**NEW — secret store:** Requires `--seal-key-path` on agent. Not documented in website/downloads.

**NEW — policy lint:** `rf policy lint /path` fails, needs `--file` flag. Default arg position misleading.

---

## Summary

| Category | Count | Pass |
|----------|-------|------|
| Execution | 5 | 3 ✅, 2 ⚠️ |
| File Ops | 3 | 3 ✅ |
| Networking | 2 | 0 ✅, 1 ⚠️, 1 ❌ |
| Orchestration | 2 | 0 ✅, 1 ⚠️, 1 ❌ |
| Secrets | 2 | 0 ✅, 2 ⚠️ |
| CLI Tools | 6 | 5 ✅, 1 ⚠️ |
| Audit/Security | 6 | 4 ✅, 2 ⚠️ |
| Transport | 3 | 2 ✅, 1 ❌ |
| **TOTAL** | **29** | **17 passed, 8 partial, 4 blocked** |
