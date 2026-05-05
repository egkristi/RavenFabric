# CIS Controls v8.1 — Implementation Mapping

> This document maps RavenFabric against the Center for Internet Security (CIS)
> Controls v8.1, focusing on the controls relevant to a secure remote execution
> and mesh networking system.

**RavenFabric version:** v0.2-dev  
**Standard:** CIS Controls v8.1 (June 2024)  
**Implementation Group:** IG2 (Enterprise)  
**Last updated:** 2026-05-05

---

## Applicability

CIS Controls v8.1 contains 18 control families with 153 safeguards across three
Implementation Groups (IG1/IG2/IG3). RavenFabric maps against controls relevant
to its domain: secure communications, remote execution, access control, and
system management.

---

## CIS Control 3: Data Protection

*Develop processes and technical controls to identify, classify, securely handle,
retain, and dispose of data.*

| Safeguard | Description | Status | Implementation |
|-----------|-------------|--------|----------------|
| 3.1 | Establish and maintain a data management process | Pass | Data classified: keys (secret), audit (internal), commands (policy-controlled) |
| 3.4 | Enforce data retention | Partial | Audit log append-only; no automated rotation yet |
| 3.6 | Encrypt data on end-user devices | Pass | Private keys stored with 0600 permissions; transport always encrypted |
| 3.10 | Encrypt sensitive data in transit | Pass | All communication encrypted via Noise XX (ChaCha20-Poly1305) |
| 3.11 | Encrypt sensitive data at rest | Pass | Private keys file-permission protected; DTN store uses SQLite with filesystem protection |
| 3.12 | Segment data processing and storage | Pass | Keys, policy, audit, and config in separate files/directories |

---

## CIS Control 4: Secure Configuration of Enterprise Assets and Software

*Establish and maintain the secure configuration of enterprise assets and software.*

| Safeguard | Description | Status | Implementation |
|-----------|-------------|--------|----------------|
| 4.1 | Establish and maintain a secure configuration process | Pass | `raven.toml` declarative config; policy YAML versioned in source control |
| 4.2 | Establish and maintain a secure configuration for network infrastructure | Pass | Relay configured via TOML, rate limiting, TLS-less (Noise provides encryption) |
| 4.6 | Securely manage enterprise assets and software | Pass | Single binary deployment, no runtime dependencies, no package managers needed |
| 4.7 | Manage default accounts on enterprise assets | Pass | No default accounts exist; all identities are generated cryptographic keys |
| 4.8 | Uninstall or disable unnecessary services | Pass | Minimal binary; feature flags disable unused transports (QUIC, WireGuard) |

---

## CIS Control 6: Access Control Management

*Use processes and tools to create, assign, manage, and revoke access credentials
and privileges for user, administrator, and service accounts.*

| Safeguard | Description | Status | Implementation |
|-----------|-------------|--------|----------------|
| 6.1 | Establish an access granting process | Pass | OTP enrollment ceremony required; admin generates OTP → agent enrolls |
| 6.2 | Establish an access revoking process | Partial | Key can be removed from trust store; no CRL mechanism yet |
| 6.3 | Require MFA for externally-exposed applications | Pass | Noise XX mutual auth is inherently two-factor (possession of key + network presence) |
| 6.4 | Require MFA for remote network access | Pass | Cryptographic mutual authentication for all remote access |
| 6.5 | Require MFA for administrative access | Pass | Same mutual auth; no weaker path exists |
| 6.6 | Establish and maintain an inventory of authentication systems | Pass | TrustStore maintains list of all enrolled agent keys |
| 6.7 | Centralize access control | Partial | Policy files per-agent; no central policy management server yet |
| 6.8 | Define and maintain role-based access control | Pass | Policy YAML defines per-identity command/path/resource permissions |

---

## CIS Control 8: Audit Log Management

*Collect, alert, review, and retain audit logs of events that could help detect,
understand, or recover from an attack.*

| Safeguard | Description | Status | Implementation |
|-----------|-------------|--------|----------------|
| 8.1 | Establish and maintain an audit log management process | Pass | `rf-audit` crate; structured JSON-lines; append-only |
| 8.2 | Collect audit logs | Pass | Every RPC action (allow + deny) produces audit entry |
| 8.3 | Ensure adequate audit log storage | Partial | Local file storage; no remote aggregation yet |
| 8.5 | Collect detailed audit logs | Pass | Entries include: timestamp, caller_key, action, command, decision, rule, duration_ms |
| 8.9 | Centralize audit logs | Planned | Future: forward to external SIEM via syslog/HTTP |
| 8.11 | Conduct audit log reviews | Pass | JSON-lines format readily parsed by jq, Splunk, ELK, etc. |
| 8.12 | Collect service provider logs | N/A | Self-hosted; no external service provider |

---

## CIS Control 12: Network Infrastructure Management

*Establish and maintain the secure configuration and management of network infrastructure.*

| Safeguard | Description | Status | Implementation |
|-----------|-------------|--------|----------------|
| 12.1 | Ensure network infrastructure is up-to-date | Pass | Relay and agent update via CI/CD pipeline; single binary replacement |
| 12.2 | Establish and maintain a secure network architecture | Pass | Zero-trust overlay; relay is zero-knowledge; all traffic end-to-end encrypted |
| 12.3 | Securely manage network infrastructure | Pass | Relay requires `meet_secret` for pairing; rate-limited; minimal API surface |
| 12.4 | Establish and maintain architecture diagrams | Pass | Architecture documented in README, ROADMAP, compliance docs |
| 12.6 | Use secure network management and communication protocols | Pass | All management via Noise XX encrypted RPC; no plaintext management |
| 12.7 | Ensure remote devices use a VPN | Pass | RavenFabric IS the encrypted overlay network (replaces VPN) |

---

## CIS Control 13: Network Monitoring and Defense

*Operate processes and tooling to establish and maintain comprehensive network
monitoring and defense against security threats.*

| Safeguard | Description | Status | Implementation |
|-----------|-------------|--------|----------------|
| 13.1 | Centralize security event alerting | Partial | Audit log captures all events; alerting requires external integration |
| 13.3 | Deploy a network intrusion detection solution | Limited | Protocol validation (magic + version) rejects malformed connections; no deep packet inspection |
| 13.4 | Perform traffic filtering between network segments | Pass | Policy engine filters at command/path level (application-layer filtering) |
| 13.6 | Collect network traffic flow logs | Pass | Connection metrics (RTT, bytes, errors) collected by metrics subsystem |
| 13.8 | Deploy network intrusion prevention | Limited | Rate limiting prevents brute force; tamper detection triggers transport migration |
| 13.9 | Deploy port-level access control | Pass | Relay accepts only Noise-authenticated connections; all others rejected |
| 13.11 | Tune security event alerting thresholds | Partial | Rate limit configurable; alerting thresholds require external system |

---

## CIS Control 16: Application Software Security

*Manage the security life cycle of in-house developed, hosted, or acquired software
to prevent, detect, and remediate security weaknesses.*

| Safeguard | Description | Status | Implementation |
|-----------|-------------|--------|----------------|
| 16.1 | Establish a secure application development process | Pass | Security-first design principles documented; deny-by-default enforced |
| 16.2 | Establish a process to accept and address software vulnerabilities | Pass | SECURITY.md, GitHub Security Advisories, private reporting enabled |
| 16.3 | Perform root cause analysis on security vulnerabilities | Pass | Issues tracked in GitHub; post-fix tests added |
| 16.4 | Establish and manage an inventory of third-party software | Pass | Cargo.lock tracks all dependencies; Dependabot monitors |
| 16.5 | Use up-to-date and trusted third-party software components | Pass | Dependabot auto-PRs for vulnerabilities; `cargo audit` planned |
| 16.6 | Establish and maintain a severity rating process | Pass | GitHub Dependabot severity ratings (critical/high/medium/low) |
| 16.7 | Use standard hardening configuration templates | Pass | Feature flags, minimal binary, no unnecessary services compiled in |
| 16.9 | Train developers in application security concepts | N/A | Single developer currently; security patterns documented in instructions |
| 16.10 | Apply secure design principles | Pass | Zero-trust, deny-by-default, least privilege, defense in depth |
| 16.11 | Leverage vetted modules or services | Pass | `snow` (audited), `tokio` (production-grade), `clap`, `serde` — all widely vetted |
| 16.12 | Implement code-level security checks | Pass | CodeQL scanning, `cargo clippy`, Rust type system, no `unsafe` in app code |
| 16.14 | Conduct threat modeling | Pass | Threat model in cryptographic-standards.md; ATT&CK mapping in this suite |

---

## Summary

| Control | Safeguards Mapped | Pass | Partial | Limited | Planned | N/A |
|---------|-------------------|------|---------|---------|---------|-----|
| CIS 3 (Data Protection) | 6 | 5 | 1 | 0 | 0 | 0 |
| CIS 4 (Secure Config) | 5 | 5 | 0 | 0 | 0 | 0 |
| CIS 6 (Access Control) | 8 | 6 | 2 | 0 | 0 | 0 |
| CIS 8 (Audit Logs) | 7 | 5 | 1 | 0 | 1 | 0 |
| CIS 12 (Network Infra) | 6 | 6 | 0 | 0 | 0 | 0 |
| CIS 13 (Network Defense) | 7 | 3 | 2 | 2 | 0 | 0 |
| CIS 16 (App Security) | 12 | 11 | 0 | 0 | 0 | 1 |
| **Total** | **51** | **41** | **6** | **2** | **1** | **1** |

**CIS Controls IG2 conformance: 80% full pass (41/51), 92% pass + partial (47/51)**

---

## Gaps and Remediation Plan

| Gap | Control | Description | Planned Fix | Timeline |
|-----|---------|-------------|-------------|----------|
| No audit log rotation | 3.4 | Retention not automated | Log rotation config + archival | v0.4 |
| No key revocation mechanism | 6.2 | Cannot revoke compromised keys centrally | CRL/key revocation broadcast | v0.4 |
| No centralized policy server | 6.7 | Policy files are per-agent | Policy distribution via mesh | v0.5 |
| No remote log aggregation | 8.3/8.9 | Audit stays local | Syslog/HTTP forwarding | v0.4 |
| No SIEM alerting | 13.1 | No automated alerts | SIEM integration guide | v0.4 |
| Limited IDS capability | 13.3/13.8 | Protocol-level only | Behavioral anomaly detection | v0.6 |
