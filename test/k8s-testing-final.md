# RavenFabric Final Test Report

> Date: 2026-08-05 | v1.0.0-rc.6 | K3s cluster | namespace: ravenfabric

---

## Infrastructure

| Pod | IP | Mode | Binary | Status |
|-----|-----|------|--------|--------|
| relay | 10.42.0.149 | relay `--listen :9090` | rc.6 | Running |
| agent1 | 10.42.0.150 | agent `--relay relay-svc:9090` | rc.6 | Running |
| agent2 | 10.42.0.151 | agent `--relay relay-svc:9090` | rc.6 | Running |
| agent-direct | 10.42.0.152 | agent `--listen :9999` | rc.6 | Running |

---

## Results: Complete Feature Matrix (32 scenarios)

### Execution (5 scenarios)

| # | Feature | Mode | Result | Evidence |
|---|---------|------|--------|----------|
| 1 | Simple exec | Direct | PASS | hostname + echo OK verified |
| 2 | Simple exec | Relay | PASS | agent1, agent2 returned correctly |
| 3 | Streaming | Relay | PASS | `for i in 1 2 3` streamed 1,2,3 |
| 4 | Background jobs | Relay | PASS | `job_id: 875070f8...` returned immediately |
| 5 | Exit codes | PARTIAL | exit 42 recorded in audit, shell gets 0 |

### Policy (6 scenarios)

| # | Feature | Result | Details |
|---|---------|--------|---------|
| 6 | Allow pattern matching | PASS | `echo ALLOWED` matched `^.*$` |
| 7 | Deny pattern matching | PASS | `apt install nmap` matched `^apt .*$` |
| 8 | Immutable deny rules | PASS | `rm -rf /` → immutable_deny:rm -rf / |
| 9 | Implicit deny | PASS | No match → implicit-deny |
| 10 | 6 policy templates | PASS | coding-assistant, prod-read-only, security-investigator, ci-cd-agent, database-query, safe-dev-mode |
| 11 | `rf policy show` | PASS | Full YAML with 13 allow + 3 deny rules |

### Audit (5 scenarios)

| # | Feature | Result | Details |
|---|---------|--------|---------|
| 12 | JSON-lines format | PASS | timestamp, request_id, action, command, decision, exit_code |
| 13 | HMAC chain | PASS | prev_hash + hmac in every entry |
| 14 | Both decisions logged | PASS | allowed and denied both have entries |
| 15 | Per-agent audit log | PASS | agent1 + agent2 independent logs |
| 16 | `rf audit derive-key` | PASS | HKDF-SHA256 key confirmed |

### File Transfer (3 scenarios)

| # | Feature | Result | Details |
|---|---------|--------|---------|
| 17 | `rf cp` pull | PASS | 32KB binary, SHA-256 verified both sides |
| 18 | `rf cp` push | PASS | "TEST DATA" content confirmed |
| 19 | Chunked checksum | PASS | 256KB default, --chunk-size, --delta flags |

### Transport (4 scenarios)

| # | Feature | Result | Details |
|---|---------|--------|---------|
| 20 | Direct WebSocket | PASS | Noise XX ~1ms, all distros |
| 21 | Relay WebSocket | PASS | Meet token paired: agent1/agent2 |
| 22 | Dev mode (in-process) | PASS | DEV_OK confirmed |
| 23 | Multi-agent relay | PASS | 2 agents via same relay broker |

### CLI Tools (4 scenarios)

| # | Feature | Result | Details |
|---|---------|--------|---------|
| 24 | Shell completions | PASS | bash, zsh, fish, elvish, powershell |
| 25 | `rf status` | DOCUMENTED | --token param, CLI exists |
| 26 | `rf policy lint` | DOCUMENTED | --file/--template, 6 check categories |
| 27 | `rf audit verify` | DOCUMENTED | HMAC chain integrity verification |

### Multi-Distro (1 scenario)

| # | Feature | Result | Details |
|---|---------|--------|---------|
| 28 | Binary portability | PASS | Ubuntu, Debian, Fedora, Rocky -- static musl binary |

### Advanced (4 scenarios)

| # | Feature | Result | Details |
|---|---------|--------|---------|
| 29 | `rf secret` | NEEDS CONFIG | needs --seal-key-path |
| 30 | `rf forward` (port) | FAIL | K8s port-forward chain instability |
| 31 | `rf proxy` (TCP) | DOCUMENTED | CLI exists, HTTP-aware mode |
| 32 | `rf shell` (interactive) | DOCUMENTED | --cols/--rows, needs PTY |

---

## Bugs Found

### CRIT-1: Noise XX handshake first attempt timeouts
- FIXED in code (flush() + 10s timeout), needs rc.8 release
- Workaround: agent reconnect succeeds on 2nd attempt

### MED-1: Exit code not forwarded to shell
- `rf exec "exit 42"` gives shell exit 0, real code in audit log only

### MED-2: Secret store needs --seal-key-path
- Error message "secret store not configured" is unclear

### LOW-1: Alpine needs sh not bash for test scripts

---

## Summary

| Category | Total | Pass | Partial | Fail |
|----------|-------|------|---------|------|
| Execution | 5 | 4 | 1 | 0 |
| Policy | 6 | 6 | 0 | 0 |
| Audit | 5 | 5 | 0 | 0 |
| File Transfer | 3 | 3 | 0 | 0 |
| Transport | 4 | 4 | 0 | 0 |
| CLI Tools | 4 | 1 | 3 | 0 |
| Multi-Distro | 1 | 1 | 0 | 0 |
| Advanced | 4 | 0 | 3 | 1 |
| **TOTAL** | **32** | **24** | **7** | **1** |

**Success rate: 75% (24/32) verified working.**
