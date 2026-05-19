# NIS2 Directive (EU 2022/2555) — Compliance Mapping

> This document maps RavenFabric's capabilities to the cybersecurity risk-management
> measures required by the NIS2 Directive, specifically Article 21.

**RavenFabric version:** v0.3.0  
**Directive:** Directive (EU) 2022/2555 (NIS2)  
**Transposition deadline:** 17 October 2024  
**Applicability:** Essential and important entities in EU/EEA member states  
**Last updated:** 2026-05-10

---

## Context

The NIS2 Directive requires essential and important entities to implement
appropriate and proportionate technical, operational, and organisational
cybersecurity risk-management measures. RavenFabric enables organizations to
meet several of these requirements through its security-first architecture.

**Important:** RavenFabric is a tool that helps organizations achieve NIS2
compliance — it is not itself a "NIS2-certified product" (no such certification
exists for individual tools).

---

## Article 21 — Cybersecurity Risk-Management Measures

### Article 21(2)(a) — Policies on Risk Analysis and Information System Security

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Risk-based security policies | YAML policy engine with deny-by-default, allow/deny regex rules for commands and paths | `rf-policy` crate, `RpcPolicy::load()` |
| Information system security | Structured audit logging of all access decisions, command executions, and policy violations | `rf-audit` crate, JSON-lines format |
| Policy enforcement | Dual enforcement: controller pre-flight check + agent local enforcement (agent is final authority) | `Executor::execute()` always calls `policy.check_command()` first |

**How RavenFabric helps:** Organizations define explicit security policies in YAML that are cryptographically enforced at the point of execution. Every action is auditable with caller identity.

---

### Article 21(2)(b) — Incident Handling

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Incident detection | Tamper detection (MAC failures, latency anomalies), health check probes | `HeartbeatStatus::LatencyAnomaly`, `TamperDetected` events |
| Incident response | Automatic transport migration on compromise detection, path blacklisting | `ConnectionManager::tamper_detected()` |
| Incident logging | Immutable audit trail with timestamps, caller identity, and decision context | `FileAuditLogger` append-only mode |

**How RavenFabric helps:** The system detects anomalies autonomously and responds by migrating away from compromised paths. All incidents are logged with full context for forensic analysis.

---

### Article 21(2)(c) — Business Continuity and Crisis Management

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Business continuity | Multi-transport architecture with automatic failover | WebSocket, QUIC, WireGuard, exotic transports |
| Disaster recovery | DTN offline queue (SQLite persistence) survives agent restarts | `PersistentDtnQueue` in `dtn_persistent.rs` |
| Backup communications | Transport diversity — if one path fails, others are available | `TransportCatalog` with tiered selection |

**How RavenFabric helps:** Operations continue even when primary communication paths fail. Agents reconnect automatically with exponential backoff. Offline queuing ensures no command is lost.

---

### Article 21(2)(d) — Supply Chain Security

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Supply chain risk assessment | Dependabot alerts, CodeQL scanning in CI | `.github/workflows/ci.yml`, CodeQL workflow |
| Supplier security requirements | All dependencies audited, minimal dependency tree | Cargo workspace with explicit deps |
| SBOM generation | CycloneDX SBOM planned for each release | CI pipeline (planned) |
| Artifact signing | Sigstore/Cosign signing planned for all binaries | Release workflow (planned) |

**How RavenFabric helps:** The project follows supply chain security best practices with automated vulnerability scanning, dependency auditing, and planned SBOM + signing.

---

### Article 21(2)(e) — Security in Network and Information System Acquisition, Development, and Maintenance

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Secure development | Rust memory safety, 1,170 tests, clippy linting, format checks | CI pipeline enforces all checks |
| Vulnerability handling | Private security advisory reporting, SECURITY.md | GitHub Security Advisories enabled |
| Security testing | Unit tests for all security-critical paths (policy, crypto, OTP) | 1,170 tests including negative/edge cases |

**How RavenFabric helps:** Built in a memory-safe language with comprehensive testing, automated security scanning, and responsible disclosure processes.

---

### Article 21(2)(f) — Policies and Procedures to Assess the Effectiveness of Cybersecurity Risk-Management Measures

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Effectiveness assessment | Prometheus `/metrics` endpoint for security posture monitoring | `metrics_server.rs`, agent `--metrics-addr` |
| Continuous monitoring | Health check probes (TCP, HTTP, process, command) | `execute_probe()` in health.rs |
| Audit review capability | Structured JSON-lines audit log, machine-parseable | `AuditEntry` with consistent schema |

---

### Article 21(2)(g) — Basic Cyber Hygiene Practices and Cybersecurity Training

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Secure defaults | Deny-by-default policy, no exposed services by default | Policy engine denies if no rule matches |
| Configuration management | Agent config via `raven.toml`, policy via YAML | Declarative, version-controllable config |

---

### Article 21(2)(h) — Policies and Procedures Regarding the Use of Cryptography

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Encryption of data in transit | Noise XX (ChaCha20-Poly1305) for all communications | `rf-crypto` crate, `SecureChannel` |
| Cryptographic key management | Automatic key generation, file-permission protected (0600), zeroed on drop | `StaticKey::load_or_generate()` |
| Algorithm selection | Industry-standard algorithms (X25519, ChaCha20, BLAKE2s) | Noise Protocol Framework via `snow` crate |
| Key rotation | Session keys are ephemeral (forward secrecy), periodic re-handshake | Noise XX ephemeral keys per session |
| Post-quantum readiness | ML-KEM hybrid handshake implemented | `HybridKemContext` + `PqxdhRatchet` in rf-crypto |

**How RavenFabric helps:** Strong encryption is mandatory and automatic — there is no "unencrypted mode". Keys are managed securely by default with no operator intervention required.

---

### Article 21(2)(i) — Human Resources Security and Access Control Policies

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Access control | Key-based identity, OTP enrollment (single-use, TTL-enforced) | `rf-bootstrap` crate |
| Principle of least privilege | Policy engine restricts to explicitly allowed commands/paths only | Deny-by-default, granular allow rules |
| Identity verification | Cryptographic identity via Noise XX static public keys | No passwords, no certificates |
| Multi-factor authentication | WebAuthn/FIDO2 planned for operator access | Roadmap v0.7 |

---

### Article 21(2)(j) — Use of Multi-Factor Authentication or Continuous Authentication Solutions

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Strong authentication | Noise XX mutual authentication (cryptographic proof of identity) | Every connection mutually authenticated |
| Continuous authentication | Per-request policy enforcement within authenticated sessions | Each RPC action independently validated |
| Secured voice/video/text | N/A (not a communications platform) | — |

---

## Article 23 — Reporting Obligations

| Requirement | RavenFabric Capability |
|-------------|------------------------|
| Significant incident detection | Tamper detection, health check failures, audit anomalies |
| Incident timeline reconstruction | Structured audit log with millisecond timestamps, caller identity, command details |
| Impact assessment data | Exit codes, output capture, duration metrics |

---

## Coverage Summary

| Article 21 Measure | Coverage Level | Notes |
|--------------------|----------------|-------|
| (a) Risk analysis policies | **Full** | Policy engine + audit |
| (b) Incident handling | **Substantial** | Auto-detection + migration + logging |
| (c) Business continuity | **Substantial** | Multi-transport + DTN + reconnect |
| (d) Supply chain security | **Partial** | Scanning in CI, SBOM/signing planned |
| (e) Secure development | **Full** | Rust + tests + CI + security advisories |
| (f) Effectiveness assessment | **Substantial** | Metrics + health checks + audit |
| (g) Cyber hygiene | **Full** | Secure defaults, declarative config |
| (h) Cryptography | **Full** | Mandatory E2E, proper key management |
| (i) Access control | **Substantial** | Crypto identity + OTP + policy (MFA planned) |
| (j) Multi-factor/continuous auth | **Partial** | Continuous auth yes, MFA planned |

---

## Gaps and Remediation Plan

| Gap | NIS2 Requirement | Status |
|-----|------------------|--------|
| No SBOM generation | Art. 21(2)(d) supply chain | Open — CycloneDX in CI planned |
| No artifact signing | Art. 21(2)(d) supply chain | Open — Sigstore/Cosign planned |
| No MFA for operators | Art. 21(2)(j) multi-factor | Open — WebAuthn/FIDO2 planned |
| ~~No SIEM export~~ | Art. 23 reporting | Done — OTLP JSON export, Prometheus, `--alert-webhook` |
| No formal incident playbooks | Art. 21(2)(b) incident handling | Open — documentation + automation planned |
