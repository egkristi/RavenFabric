# RavenFabric K8s Testing — Final Report

> Date: 2026-08-05 | v1.0.0-rc.6 | K3s | ravenfabric

---

## Result: RELAY MODE WORKS

After fixing NetworkPolicy, relay-based remote execution works across multiple agents.

---

## Infrastructure

| Pod | IP | Role | Status |
|-----|-----|------|--------|
| relay | 10.42.0.117 | Relay broker | Running |
| agent1 | 10.42.0.118 | Agent (agent1) | Running |
| agent2 | 10.42.0.119 | Agent (agent2) | Running |
| relay-svc | ClusterIP | Service | Active |

---

## Working (Relay Mode)

### S1: Relay exec
- rf --relay ws://127.0.0.1:9090 exec --token agent1 "echo RELAY_WORKS_AGENT1" → RELAY_WORKS_AGENT1
- rf --relay ws://127.0.0.1:9090 exec --token agent2 "echo RELAY_WORKS_AGENT2" → RELAY_WORKS_AGENT2
- uname -a → Linux agent1 6.12.100+deb13-amd64 x86_64 GNU/Linux

### S2: Multi-agent
- Two agents on same relay broker, correct token routing

## Issues

### Fixed
- NetworkPolicy default-deny blocked pod-to-pod → FIXED by user
- K8s DNS needs Service name (relay-svc, not pod name)

### Persistent
- Noise XX handshake times out on first agent→relay attempt (30s), succeeds on reconnect
- This is the snow-0.10.0 cross-platform bug

## Summary

- Relay-based exec: WORKING
- Multi-agent via relay: WORKING
- CLI→relay→agent: VERIFIED
