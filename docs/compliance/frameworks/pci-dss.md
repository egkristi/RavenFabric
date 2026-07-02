# PCI-DSS — Compliance Mapping

> This document maps RavenFabric's capabilities to the requirements of the
> Payment Card Industry Data Security Standard (PCI-DSS v4.0).

**RavenFabric version:** v1.0.0-rc.5
**Standard:** PCI Data Security Standard v4.0
**Applicability:** Any organization that stores, processes, or transmits cardholder data
**Last updated:** 2026-07-18

---

## Context

The PCI-DSS requires organizations that handle payment card data to implement
specific security controls. RavenFabric is a secure remote execution and mesh
networking agent — it is not a payment processing system. However, organizations
that use RavenFabric to manage cardholder data environments (CDE) must ensure
their deployment meets PCI-DSS requirements.

**Important:** RavenFabric is a tool that helps organizations achieve PCI-DSS
compliance for their remote management infrastructure — it is not itself a
"PCI-DSS validated" product. Compliance requires organizational policies,
quarterly scans, and annual assessments by a QSA.

---

## Requirement 1 — Install and Maintain Network Security Controls

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| 1.1 — Network security controls | Deny-by-default policy engine, allow/deny rules for network targets by CIDR and port | `NetworkRule` in `rf-policy`, `check_network_target()` |
| 1.2 — Secure network configuration | End-to-end encrypted channels, no plaintext network paths | Noise XX handshake, ChaCha20-Poly1305 |
| 1.3 — CDE access restrictions | Policy rules restrict which agents can access which network targets | `NetworkRule::allow` with CIDR + port patterns |
| 1.4 — Network connections | Per-IP rate limiting (20 connections/min), connection authentication | `RateLimiter` in `rf-relay`, Noise XX mutual auth |

**How RavenFabric helps:** Organizations define explicit network policies that
restrict which agents can connect to which targets. All traffic is encrypted
and mutually authenticated.

---

## Requirement 2 — Apply Secure Configurations

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| 2.1 — Change vendor defaults | No default passwords or credentials — identity is cryptographic key material | `StaticKey` generation, no hardcoded secrets |
| 2.2 — Secure configuration | YAML-based policy configuration, deny-by-default, least privilege | `RpcPolicy::load()` from YAML |
| 2.2.1 — Only necessary services | Agent runs only configured transports, no unnecessary listeners | `TransportConfig` in `raven.toml` |
| 2.2.2 — Secure crypto configuration | FIPS mode for HSM operations, strong algorithms only | `HsmConfig::fips_mode`, Noise XX with X25519 |

**How RavenFabric helps:** The system ships with no default credentials, uses
cryptographic identity, and enforces least-privilege through explicit policy
configuration.

---

## Requirement 3 — Protect Stored Account Data

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| 3.1 — Data retention | Configurable audit retention (`max_age`), `purge_entries_before()` API | `CollectorConfig::max_age`, `FileAuditLogger::purge_entries_before()` |
| 3.2 — Data disposal | Secure deletion of audit entries, key zeroing on drop | `Zeroize` on private keys, deletion API |
| 3.3 — Sensitive data display | Output size limiting prevents excessive data exposure | `ResourceLimits::max_output_bytes` |
| 3.4 — Encryption at rest | Private keys stored with 0600 permissions, sealed secret store | File permissions, `SealedSecretStore` |
| 3.5 — Key management | HSM-backed key storage (PKCS#11, YubiHSM2, TPM 2.0), key generation in hardware | `HsmKeyProvider`, `TpmKeyProvider` |
| 3.6 — Key rotation | Secret rotation hooks with grace period, health-check after rotation | `SecretRotationConfig`, rotation audit trail |
| 3.7 — Key zeroing | Private keys zeroed from memory on drop via `Zeroize` | `Drop` impl on `StaticKey` |

**How RavenFabric helps:** Cryptographic keys are stored securely (HSM, TPM, or
file-permission-protected), zeroed on drop, and support rotation with grace periods.

---

## Requirement 4 — Protect Cardholder Data in Transit

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| 4.1 — Strong cryptography | Noise XX with X25519 + ChaCha20-Poly1305 + BLAKE2s | `rf-crypto` crate, `SecureChannel` |
| 4.2 — No plaintext PAN | End-to-end encryption — relay sees only ciphertext | Noise XX handshake, wire protocol |
| 4.3 — Secure protocols | WebSocket (wss://), QUIC (TLS 1.3), all transports encrypted | `WebSocketDriver`, `QuicDriver` |

**How RavenFabric helps:** All data in transit is encrypted with strong
cryptography. The relay is cryptographically incapable of accessing plaintext.

---

## Requirement 5 — Protect All Systems from Malware

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| 5.1 — Anti-malware mechanisms | Command pattern filtering prevents known malicious patterns | `CommandRule::deny` with regex patterns |
| 5.2 — Unauthorized software | Policy engine restricts which commands and binaries can execute | `check_command()` enforces allow/deny rules |
| 5.3 — Integrity verification | HMAC-chained audit log, binary integrity via SHA-256 + Ed25519 signatures | `AuditEntry::verify_hmac()`, auto-update verification |

**How RavenFabric helps:** The policy engine prevents execution of unauthorized
commands. Audit log integrity ensures tampering is detectable.

---

## Requirement 6 — Develop and Maintain Secure Systems

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| 6.1 — Security patching | Auto-update mechanism with staged rollout, canary, health-check gates | `AutoUpdateConfig`, staged rollout |
| 6.2 — Secure development | Comprehensive test suite (1,423+ tests), fuzz testing (4 targets), clippy-clean | CI pipeline, `cargo test`, `cargo clippy` |
| 6.3 — Security vulnerabilities | Dependabot alerts, CodeQL scanning, security policy (`SECURITY.md`) | GitHub security features |
| 6.4 — Change management | Policy hot-reload via SIGHUP, versioned YAML config, audit trail of changes | `RpcPolicy::reload()`, audit logging |

**How RavenFabric helps:** Automated updates with staged rollout minimize risk.
Comprehensive testing and security scanning catch vulnerabilities early.

---

## Requirement 7 — Restrict Access by Need-to-Know

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| 7.1 — Access control | RBAC with admin/operator/viewer/auditor roles | `rbac.rs` in `rf-policy` |
| 7.2 — Need-to-know | Policy rules restrict commands, paths, and network targets per caller | `RpcPolicy::check_command()`, `check_path()`, `check_network_target()` |
| 7.3 — Access reviews | Audit log provides complete record of who accessed what | `AuditEntry` with `caller_key` |

**How RavenFabric helps:** Access is granted on a need-to-know basis through
explicit policy rules. Every access is logged for review.

---

## Requirement 8 — Identify Users and Authenticate Access

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| 8.1 — Unique IDs | Cryptographic identity via Noise XX static keys — each agent has unique key | `StaticKey` per agent |
| 8.2 — Strong authentication | Mutual authentication via Noise XX handshake | XX pattern (both sides prove identity) |
| 8.3 — Multi-factor authentication | HSM + PIN for key access, TPM 2.0 platform attestation | `HsmKeyProvider`, `TpmKeyProvider` |
| 8.4 — Password management | No passwords — cryptographic key material only | No password-based auth in protocol |
| 8.5 — Session termination | Configurable session timeouts, idle timeout for proxy tunnels | `ProxyConfig::idle_timeout`, `max_duration` |

**How RavenFabric helps:** Authentication is cryptographic, not password-based.
Each agent has a unique identity verified through mutual authentication.

---

## Requirement 9 — Restrict Physical Access

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| 9.1 — Physical access controls | N/A — software tool, physical security is organizational responsibility | — |
| 9.2 — Media disposal | Key zeroing on drop, secure deletion of audit data | `Zeroize`, deletion API |

**How RavenFabric helps:** Cryptographic material is securely erased when no
longer needed.

---

## Requirement 10 — Log and Monitor All Access

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| 10.1 — Audit trails | Structured JSON-lines audit logging for all actions | `FileAuditLogger` |
| 10.2 — Audit log contents | Timestamp, caller identity, action, decision, result | `AuditEntry` with all required fields |
| 10.3 — Audit log protection | HMAC-chained entries (tamper-evident), append-only, 0600 permissions | `prev_hash` + `hmac` chain |
| 10.4 — Audit log review | Compliance report generation, SIEM export (Syslog, Splunk, Elasticsearch, Datadog) | `ReportGenerator`, SIEM exporters |
| 10.5 — Audit log retention | Configurable retention via `max_age`, `purge_entries_before()` | `CollectorConfig`, deletion API |
| 10.6 — Clock synchronisation | All entries timestamped with `DateTime<Utc>` | `AuditEntry::timestamp` |

**How RavenFabric helps:** Comprehensive audit logging with tamper-evident chain,
SIEM integration, and configurable retention meets PCI-DSS logging requirements.

---

## Requirement 11 — Test Security Regularly

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| 11.1 — Vulnerability scans | Dependabot alerts, CodeQL scanning, regular dependency updates | GitHub security features |
| 11.2 — Penetration testing | Security self-audit (17 tests), fuzz testing (4 targets) | `tests/` directories, fuzz targets |
| 11.3 — Detection mechanisms | Tamper detection, anomaly detection, health check probes | `HeartbeatStatus`, `TamperDetected` |
| 11.4 — Intrusion detection | Behavioral anomaly detection (velocity, novelty, timing, escalation) | Anomaly detection module |
| 11.5 — Change verification | Policy hot-reload, versioned configuration, audit trail | `RpcPolicy::reload()` |

**How RavenFabric helps:** Regular security testing through automated scanning,
fuzz testing, and anomaly detection.

---

## Requirement 12 — Support Information Security with Policies

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| 12.1 — Security policy | YAML policy engine with deny-by-default, explicit allow rules | `rf-policy` crate |
| 12.2 — Risk assessment | Compliance report generation, security self-audit | `ReportGenerator`, security tests |
| 12.3 — Usage policies | Policy rules define acceptable commands, paths, and network targets | `RpcPolicy` with allow/deny lists |
| 12.4 — Incident response | Alert rules with webhook destinations, tamper detection | Alert module in `rf-audit` |
| 12.5 — Security awareness | Documentation, architecture guides, compliance mappings | `docs/` directory |

**How RavenFabric helps:** The policy engine operationalizes security policies
into enforceable rules with full audit trails.

---

## Implementation Guidance

### Recommended Configuration for PCI-DSS Compliance

```toml
[agent]
# Enable FIPS-compatible mode
fips_mode = true

[audit]
# PCI-DSS requires 12-month audit log retention
max_age_days = 365
# Buffer capacity
buffer_capacity = 8192
# Flush interval
flush_interval_secs = 5

[policy]
# Restrict commands to approved operations only
[policy.commands.allow]
patterns = [
    "^systemctl status .*",
    "^journalctl.*",
    "^tail -n 100 .*",
]

[policy.network.allow]
# Restrict network access to CDE boundaries
patterns = [
    { cidr = "10.0.0.0/8", ports = ["443", "8443"] },
]

[transport]
# Use only encrypted transports
driver = "websocket"
# Force WSS only
use_tls = true
```

### Operational Procedures

1. **Audit log retention** — Configure `max_age` to at least 12 months for PCI-DSS
   compliance. Use `purge_entries_before()` for scheduled purging of older data.
2. **Key management** — Use HSM-backed keys with FIPS mode enabled for production
   CDE environments. Rotate keys annually at minimum.
3. **Access reviews** — Regularly review audit logs for unauthorized access attempts.
   Use SIEM integration for automated monitoring.
4. **Change management** — All policy changes are logged. Review policy changes
   through the change management process.
5. **Quarterly scans** — Run `rf policy lint` to validate policy configuration.
   Integrate with vulnerability scanning tools.

---

## Limitations

| Area | Limitation | Mitigation |
|------|------------|------------|
| FIPS 140-3 validation | Current crypto uses `snow` crate (not FIPS-validated module) | Use HSM-backed keys with FIPS mode for key management; evaluate FIPS-validated TLS for transport |
| CDE segmentation | No automatic CDE traffic isolation | Configure network policies to restrict CDE access to authorized agents only |
| Quarterly scan integration | No built-in vulnerability scanning | Integrate with external scanning tools; use `rf policy lint` for policy validation |
| Multi-factor auth | No built-in MFA for agent-to-agent connections | Use HSM + PIN for key access as second factor |
| PA-DSS validation | RavenFabric is not a PA-DSS validated application | RavenFabric is infrastructure software, not a payment application |
