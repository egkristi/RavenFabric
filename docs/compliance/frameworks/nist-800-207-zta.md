# NIST SP 800-207 — Zero Trust Architecture Mapping

> This document maps RavenFabric's architecture to the tenets and logical
> components defined in NIST SP 800-207 (Zero Trust Architecture), August 2020.

**RavenFabric version:** v0.3.0  
**Standard version:** NIST SP 800-207 (Final, August 2020)  
**Last updated:** 2026-05-10

---

## Section 2: Zero Trust Basics

### 2.1 Tenets of Zero Trust

| # | Tenet | RavenFabric Implementation |
|---|-------|----------------------------|
| 1 | All data sources and computing services are considered resources | Every agent is a resource. No distinction between "internal" and "external" — all connections require mutual authentication regardless of network position. |
| 2 | All communication is secured regardless of network location | Noise XX E2E encryption on every connection. The relay (even if co-located) never sees plaintext. No "trusted network" bypass. |
| 3 | Access to individual enterprise resources is granted on a per-session basis | Each RPC request is independently policy-checked. Session establishment (Noise handshake) authenticates, but every action within the session is re-validated against policy. |
| 4 | Access to resources is determined by dynamic policy | YAML-based policy engine with SIGHUP hot-reload. Policy can be updated without restarting agents. Deny-by-default with explicit allow rules. |
| 5 | The enterprise monitors and measures the integrity and security posture of all owned and associated assets | Health check probes (TCP, HTTP, process, command), system metrics collection, structured audit logging of every decision. Prometheus `/metrics` endpoint for continuous monitoring. |
| 6 | All resource authentication and authorization is dynamic and strictly enforced before access is allowed | Noise XX mutual authentication completes before any RPC. Policy check occurs before every command execution. Dual check: controller pre-flight + agent local enforcement. |
| 7 | The enterprise collects as much information as possible about the current state of assets, network infrastructure and communications, and uses it to improve its security posture | Structured JSON-lines audit log captures every decision with caller identity, timestamp, command, matched rule, exit code, and duration. Tamper detection triggers automatic transport migration. |

---

## Section 3.2: Logical Components

### Policy Engine (PE)

**NIST Definition:** The component responsible for the ultimate decision to grant access to a resource.

**RavenFabric Implementation:**

- `rf-policy` crate (`RpcPolicy` struct)
- Loads YAML policy files defining allow/deny rules for commands, filesystem paths, and resource limits
- Decision output: `Decision { allowed: bool, reason: String, matched_rule: String }`
- Deny rules always take precedence over allow rules
- Default decision when no rule matches: **DENY**

### Policy Administrator (PA)

**NIST Definition:** The component responsible for establishing and/or shutting down the communication path between subject and resource.

**RavenFabric Implementation:**

- The agent binary (`rf-agent`) acts as PA — it loads policy, instantiates the executor, and controls the RPC session lifecycle
- SIGHUP handler enables policy hot-reload (atomic swap via RwLock)
- Meet-token authentication at relay controls initial session establishment
- OTP enrollment flow (`rf-bootstrap`) controls which agents are admitted to the trust store

### Policy Enforcement Point (PEP)

**NIST Definition:** The system responsible for enabling, monitoring, and eventually terminating connections between a subject and an enterprise resource.

**RavenFabric Implementation:**

- **Network PEP:** Relay broker (`rf-relay`) enforces rate limiting (20 conn/IP/min), validates meet tokens (HMAC-SHA256), and pairs only authenticated sessions
- **Execution PEP:** Executor (`rf-executor`) enforces policy before every command, applies timeout limits, caps output size, and kills runaway processes
- **Transport PEP:** SecureChannel validates wire magic (`RVNF`), version byte, and completes Noise XX before any RPC

---

## Section 3.3: Trust Algorithm

RavenFabric's trust model:

| Factor | How Evaluated |
|--------|---------------|
| **Subject identity** | Noise XX static public key (cryptographically verified during handshake) |
| **Subject credentials** | OTP for enrollment, public key for ongoing sessions |
| **Asset state** | Health probes, system metrics, connectivity health |
| **Request context** | Command string, working directory, environment variables — all checked against policy |
| **Resource sensitivity** | Filesystem path deny rules, command pattern restrictions |
| **Threat intelligence** | Tamper detection (MAC failure, latency anomaly) triggers transport blacklisting |

---

## Section 4: Deployment Scenarios

### 4.1 Device Agent/Gateway-Based Deployment

RavenFabric operates as a **device-resident agent** with a gateway (relay) for connectivity:

```text
Subject ──→ rf-cli (PEP client)
                │
                │ Noise XX encrypted
                ▼
            rf-relay (Gateway/PEP — rate limit, token auth)
                │
                │ Noise XX encrypted (end-to-end, relay is blind)
                ▼
            rf-agent (PEP + PE — local policy enforcement)
                │
                ▼
            Resource (command execution, file access)
```

This maps to NIST's "Agent/Gateway" model where:

- The relay acts as a network gateway (routes but cannot inspect)
- The agent acts as both PE and PEP (final authority on access decisions)
- The CLI acts as subject/requester

### 4.3 Resource Portal-Based Deployment

Not currently applicable — RavenFabric does not use a reverse proxy model.

### 4.5 Device Application Sandboxing

Partially applicable:

- Commands execute via `/bin/sh -c` with policy-controlled strings
- Output is bounded (max 10 MB default)
- Execution time is bounded (300s default)
- Resource limits prevent exhaustion attacks

---

## Section 7: Threats Associated with Zero Trust Architecture

| Threat | NIST Description | RavenFabric Mitigation |
|--------|------------------|------------------------|
| **Subversion of ZTA Decision Process** | Attacker targets PE/PA | Agent is the final authority — a compromised controller cannot override agent policy. Dual-check architecture prevents single point of compromise. |
| **Denial-of-Service or Network Disruption** | Starving PE/PA of connectivity | Multi-transport architecture (WebSocket, QUIC, WireGuard), automatic failover via ConnectionManager, DTN offline queue for disconnected operation. |
| **Stolen Credentials** | Compromised tokens | OTP is single-use and TTL-enforced. Static keys cannot be replayed (Noise XX ephemeral keys provide forward secrecy). No passwords in the system. |
| **Visibility on the Network** | Metadata analysis | All traffic encrypted. Relay cannot distinguish command types. Traffic obfuscation layer available. Multiple transport diversity (censorship resistance tier). |
| **Storage of System and Network Information** | PE/PA data compromise | Policy files are static YAML (no secrets). Trust store contains only public keys. Audit log contains decisions, not command output. Private keys are permission-protected and zeroed on drop. |
| **Reliance on Proprietary Data Formats** | Vendor lock-in | Wire protocol is documented (msgpack + Noise). Policy is YAML. Audit is JSON-lines. All formats are open and parseable. |
| **Use of Non-person Entities (NPE) in ZTA Administration** | Automated systems with elevated access | Agents authenticate identically to operators (same Noise XX). No implicit trust for automated callers. |

---

## Gaps and Planned Improvements

| Gap | NIST Requirement | Status |
|-----|------------------|--------|
| ~~No continuous device posture assessment~~ | Tenet 5 (continuous monitoring) | Done — Grains, health probes, desired-state drift detection |
| ~~No risk scoring in policy decisions~~ | Section 3.3 (trust algorithm inputs) | Done — Behavioral anomaly scoring per identity |
| ~~No SIEM/SOAR integration~~ | Section 3.2 (data sources) | Done — OTLP JSON export, Prometheus endpoint, `--alert-webhook` |
| No multi-factor for operators | Section 3.3 (MFA) | Open — WebAuthn/FIDO2 planned |
| ~~No device inventory integration~~ | Tenet 5 (asset monitoring) | Done — AgentRegistry with grains, heartbeat, label selection |
