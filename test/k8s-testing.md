# RavenFabric K8s Testing Report

> Generated: 2026-08-05
> Namespace: `ravenfabric`
> Cluster: K3s (single node: `utviklerboks`)
> Test CLI: `rf 1.0.0-rc.6` (linux-amd64-musl from GitHub Releases)

---

## Test Infrastructure

```text
┌─────────────────────────────────────────────────────────┐
│ ravenfabric namespace (K3s cluster)                    │
│                                                         │
│  ┌───────────┐  ┌───────────┐  ┌───────────┐          │
│  │ ubuntu-pod │  │ rf-agent-1│  │ rf-agent-2│          │
│  │ 10.42.0.83 │  │10.42.0.90 │  │10.42.0.91 │          │
│  │ (netcat)   │  │(rf-agent) │  │(rf-agent) │          │
│  └───────────┘  └─────┬─────┘  └─────┬─────┘          │
│                       │               │                │
│                       └───────┬───────┘                │
│                               │ WebSocket              │
│                        ┌──────┴──────┐                 │
│                        │  rf-relay   │                 │
│                        │ 10.42.0.89  │                 │
│                        │ (rf-relay)  │                 │
│                        └──────┬──────┘                 │
│                               │ kubectl port-forward   │
│                               │ 127.0.0.1:9092→:9090  │
│                        ┌──────┴──────┐                 │
│                        │   rf CLI    │                 │
│                        │  (local)    │                 │
│                        └─────────────┘                 │
└─────────────────────────────────────────────────────────┘
```

### Deployed Resources

| Resource | Type | Status |
|----------|------|--------|
| `rf-relay` | Pod | Running - `rf-relay --listen 0.0.0.0:9090` |
| `rf-relay` | Service (ClusterIP) | 10.43.158.241:9090 |
| `rf-agent-1` | Pod | Running - `--token agent1 --compat-mode` |
| `rf-agent-2` | Pod | Running - `--token agent2 --compat-mode` |
| `rf-agent-policy` | ConfigMap | Permissive policy |
| `allow-ravenfabric-internal` | NetworkPolicy | Allow TCP 9090 pod-to-pod |
| `ubuntu-pod` | Pod | Test helper |

---

## Test Results

### Test 1: Infrastructure Setup — PASS

- kubectl access to K3s cluster: verified
- Namespace `ravenfabric`: Active
- All pods Running/Ready
- DNS resolution works: `rf-relay.ravenfabric.svc.cluster.local` → ClusterIP
- Binary distribution (GitHub Releases v1.0.0-rc.6): verified
- Relay binary starts, listens on port 9090
- Agent binary starts with correct version

### Test 2: CLI → Agent exec via relay — FAIL (BLOCKED)

**Attempted:** `rf --relay ws://127.0.0.1:9092 exec --token agent1 "hostname"`

**Result:** CLI connects to relay, relay pairs meet tokens (`paired meet token: agent1`), but agent relay Noise XX handshake never completes.

**Agent log:**
```
Noise XX handshake timed out after 30s - possible cross-platform relay issue
handshake timed out - relay may have cross-platform compatibility issue
```

**Attempted mitigations:**
- `--compat-mode` on agent: no effect
- Restart relay + agents: no effect

**Root cause:** Known snow-0.10.0 Noise XX handshake bug (documented in ROADMAP.md)

### Test 3: CLI Relay meet token pairing — PASS

Relay correctly receives and pairs meet tokens from CLI. Issue is downstream.

### Tests 4-10: BLOCKED

All blocked by Test 2 (Noise XX handshake):
- Policy denial enforcement
- Audit logging verification
- Port forwarding
- Dev mode
- Desired-state convergence
- Data collection
- MCP server
- Resilience/reconnect
- Direct connection

---

## Issues Found

### Issue #1: NetworkPolicy blocks pod-to-pod traffic — FIXED

**Symptoms:** Agents get `Connection refused (os error 111)`

**Root cause:** `default-deny` NetworkPolicy in namespace. Ingress only from `webtop`. No pod-to-pod egress.

**Fix:** Created `allow-ravenfabric-internal` NetworkPolicy allowing TCP 9090 between all pods.

### Issue #2: Noise XX handshake timeout via relay — PERSISTENT (CRITICAL)

**Symptoms:** Agent connects to relay (meet tokens paired), then:
```
Noise XX handshake timed out after 30s
```

**Reproduction:** 100%. Every handshake through relay fails.

**Impact:** Blocks all relay-based functionality.

**Fix needed:** Upgrade snow crate or add relay-side handshake message buffering. Fork alternative mentioned in ROADMAP under "Critical bugs blocking v1.0.0".

### Issue #3: compat-mode ineffective

`--compat-mode` documented for macOS→Linux relay cases, but does not resolve this K3s-specific handshake failure.

### Issue #4: Single-node K3s

All pods on `utviklerboks`. Multi-node cluster might behave differently but core Noise XX issue is protocol-level.

---

## Summary

| Area | Status |
|------|--------|
| K8s infrastructure | Working |
| Pod-to-pod networking | Working (after NetworkPolicy fix) |
| Binary distribution | Verified (1.0.0-rc.6) |
| Relay CLI connectivity | Working |
| Relay Agent TCP connectivity | Working |
| Noise XX handshake agent-relay | **BLOCKED** - known snow-0.10.0 bug |
| All downstream functionality | Blocked by handshake issue |

**Bottom line:** K8s deployment works. The single blocking issue is the Noise XX handshake timeout through relay. Until resolved, relay-based execution cannot be demonstrated. Direct-connect mode should be tested as a workaround.
