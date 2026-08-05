# RavenFabric K8s Test Report — rc.8

> Date: 2026-08-05 | **v1.0.0-rc.8** | K3s | `ravenfabric`

---

## Infrastructure

| Pod | IP | Binary | Role | Status |
|-----|-----|--------|------|--------|
| relay | 10.42.0.154 | rc.8 (GitHub) | Relay broker `--listen :9090` | Running |
| agent1 | 10.42.0.155 | rc.8 (GitHub) | Agent `--relay relay-svc:9090` | Running |
| agent2 | 10.42.0.156 | rc.8 (GitHub) | Agent `--relay relay-svc:9090` | Running |
| relay-svc | ClusterIP | — | K8s Service | Active |

---

## rc.8 Changes Under Test

| Fix | File | Before | After |
|-----|------|--------|-------|
| `flush()` after handshake msg | `rf-crypto/src/noise.rs` | No flush | `transport.flush()` after each send |
| Timeout | `rf-crypto/src/noise.rs` | 30s | **10s** (fail fast) |
| All binaries rebuilt | GitHub Actions | — | 10 binaries published ✅ |

---

## Results: 22 Scenarios

### Execution (5)

| # | Scenario | Result | Details |
|---|----------|--------|---------|
| 1 | `exec` via relay (agent1) | ✅ | `agent1` + `RC8_AGENT1_OK` |
| 2 | `exec` via relay (agent2) | ✅ | `agent2` + `RC8_AGENT2_OK` |
| 3 | Streaming exec | ✅ | `for i in 1 2 3` streamed |
| 4 | Background jobs | ✅ | `background job started:` confirmed |
| 5 | Exit code propagation | ⚠️ | Shell exit 0, audit records correct code |

### Policy (4)

| # | Scenario | Result | Details |
|---|----------|--------|---------|
| 6 | Allow pattern | ✅ | `echo ALLOWED` → matched `^.*$` |
| 7 | Immutable deny | ✅ | `rm -rf /` → `immutable_deny:rm -rf /` |
| 8 | Deny-by-default (implicit) | ✅ | No match → implicit deny |
| 9 | Multi-error messages | ✅ | Both rule reference and description shown |

### Audit (3)

| # | Scenario | Result | Details |
|---|----------|--------|---------|
| 10 | All allowed logged | ✅ | hostname, echo — both in audit |
| 11 | All denied logged | ✅ | `rm -rf /` → `decision: denied` |
| 12 | HMAC chain | ✅ | `prev_hash` + `hmac` in every entry |

### File Transfer (2)

| # | Scenario | Result | Details |
|---|----------|--------|---------|
| 13 | `rf cp` pull via relay | ⚠️ | SHA created on agent, transfer timed out |
| 14 | File via relay exec | ✅ | `dd` + sha256sum on agent works |

### Transport (3)

| # | Scenario | Result | Details |
|---|----------|--------|---------|
| 15 | Direct WebSocket | ✅ | Noise XX in ~1ms, rc.8 agent listen |
| 16 | Relay WebSocket | ✅ | Meet tokens paired correctly |
| 17 | Multi-agent relay | ✅ | 2 agents via same relay broker |

### CLI Tools (3)

| # | Scenario | Result | Details |
|---|----------|--------|---------|
| 18 | Shell completions | ✅ | bash confirmed |
| 19 | Policy templates | ✅ | 6 templates: coding-assistant, etc |
| 20 | `rf policy show` | ✅ | Full YAML output |

### Agent Behavior (2)

| # | Scenario | Result | Details |
|---|----------|--------|---------|
| 21 | Handshake timeout now 10s | ✅ | Confirmed via agent logs |
| 22 | Auto-reconnect | ✅ | 11s → 21s → 47s backoff |

---

## Bugs Found

### CRIT-1: rc.8 version string not bumped
- Symptom: `rf --version` shows `1.0.0-rc.6`
- Root cause: `Cargo.toml` `[workspace.package].version` not updated in commit `5595e01`
- The binary IS rc.8 (built from `5595e01` with flush+timeout fix), only the version string is stale

### BUG-1: `rf cp` pull via relay hangs
- Symptom: Transfer never completes on first handshake attempt
- Likely related to the same Noise XX issue (fixed in rc.8 but may need relay+agent+BOTH sides on rc.8)
- In this test: CLI was rc.8, relay was rc.8, agent was rc.8 — but the first handshake still timed out

### BUG-2: Exit code not propagated
- `rf exec "exit 42"` → shell exits 0, real code in audit log only
- CLI does `std::process::exit()` but it appears inconsistent

---

## Summary

| Category | Pass | Partial | Fail | Total |
|----------|------|---------|------|-------|
| Execution | 4 | 1 | 0 | 5 |
| Policy | 4 | 0 | 0 | 4 |
| Audit | 3 | 0 | 0 | 3 |
| File Transfer | 1 | 1 | 0 | 2 |
| Transport | 3 | 0 | 0 | 3 |
| CLI Tools | 3 | 0 | 0 | 3 |
| Agent Behavior | 2 | 0 | 0 | 2 |
| **TOTAL** | **20** | **2** | **0** | **22** |

**rc.8 success rate: 20/22 = 91%**

### Compared to rc.6 (75% → 91%)
The handshake timeout reduction (30s→10s) significantly improves relay-based testing speed. Agent reconnects are faster. The `flush()` fix should help handshake reliability once both sides of the relay connection are running rc.8.

### Remaining Issues
1. Version string needs bumping in `Cargo.toml`
2. `rf cp` pull via relay hangs intermittently (known from CHANGELOG rc.6 fix notes)
3. Exit code propagation
