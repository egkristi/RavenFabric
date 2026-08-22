# GDPR — Compliance Mapping

> This document maps RavenFabric's capabilities to the requirements of the
> General Data Protection Regulation (GDPR, Regulation (EU) 2016/679).

**RavenFabric version:** v1.0.0-rc.13
**Regulation:** Regulation (EU) 2016/679 (General Data Protection Regulation)
**Applicability:** Any organization processing personal data of EU/EEA data subjects
**Last updated:** 2026-08-22

---

## Context

The GDPR requires organizations to implement appropriate technical and organisational
measures to protect personal data. RavenFabric is a secure remote execution and mesh
networking agent — it does not itself process personal data as a controller or processor.
However, organizations deploying RavenFabric **must** ensure their use of the system
complies with GDPR principles.

**Important:** RavenFabric is a tool that helps organizations achieve GDPR compliance
for their remote management and monitoring operations — it is not itself a "GDPR-compliant
product." Compliance depends on organizational policies, data processing agreements, and
operational procedures around the tool.

---

## Article 5 — Principles Relating to Processing of Personal Data

### Article 5(1)(a) — Lawfulness, Fairness and Transparency

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Lawful processing | Policy engine enforces explicit allow/deny rules — no operation occurs without authorization | `rf-policy` crate, `RpcPolicy::check_command()` |
| Transparent processing | Structured audit logging records every action with caller identity, timestamp, and decision | `FileAuditLogger`, JSON-lines format |
| Information to data subjects | Audit trail provides complete record of who accessed what and when | `AuditEntry` with `caller_key`, `action`, `timestamp` |

**How RavenFabric helps:** Every remote operation is explicitly authorized by policy and
recorded in an immutable audit trail. Organizations can demonstrate exactly what
operations were performed, by whom, and when.

---

### Article 5(1)(b) — Purpose Limitation

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Data collected for specified purposes | Policy rules can restrict commands to specific purposes via regex patterns | `CommandRule::allow` with purpose-scoped patterns |
| No further processing incompatible | Deny-by-default ensures no operation occurs outside explicitly allowed scope | `RpcPolicy::check_command()` returns `PolicyError::Denied` for unlisted commands |

**How RavenFabric helps:** Policy YAML files define exactly what operations are permitted,
preventing scope creep or unauthorized use of the system.

---

### Article 5(1)(c) — Data Minimisation

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Adequate, relevant, limited to what is necessary | Output size limiting (`maxOutputBytes`), command pattern filtering, audit buffer retention | `ResourceLimits::max_output_bytes`, `CollectorConfig::max_age` |
| No excessive data collection | Audit entries capture only operational metadata — no PII by default | `AuditEntry` fields are operational (command, exit code, duration) |

**How RavenFabric helps:** The system limits what data is collected and how long it is
retained. Organizations should configure audit retention policies to match their
data minimisation requirements.

---

### Article 5(1)(d) — Accuracy

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Personal data accurate and kept up to date | N/A — RavenFabric does not maintain a database of personal data | — |
| Every reasonable step to ensure erasure/rectification | Audit log is append-only (tamper-evident) — corrections are recorded as new entries | `FileAuditLogger` append-only mode |

**How RavenFabric helps:** If audit entries contain inaccurate data, corrections are
recorded as new entries with references to the original, preserving the integrity
of the audit trail.

---

### Article 5(1)(e) — Storage Limitation

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Kept no longer than necessary | Configurable audit retention (`max_age` in `CollectorConfig`), buffer capacity limits | `BufferedAuditCollector` with configurable `max_age` (default 24h) |
| Erasure or anonymisation | `purge_entries_before()` API for programmatic deletion of old audit entries | `FileAuditLogger::purge_entries_before()` |
| Periodic review mechanisms | Retention policy configurable per deployment | `CollectorConfig` fields |

**How RavenFabric helps:** Organizations configure audit retention periods to match
their data retention policies. Old entries are automatically purged based on age.

---

### Article 5(1)(f) — Integrity and Confidentiality

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Appropriate security | Noise XX mutual authentication, end-to-end encryption, deny-by-default policy | `rf-crypto` crate, `SecureChannel` |
| Unauthorised processing prevention | Policy engine enforces at both controller (pre-flight) and agent (local) | Dual policy check architecture |
| Personal data breach prevention | Tamper detection, automatic transport migration, rate limiting | `ConnectionManager::tamper_detected()`, per-IP rate limiter |
| Confidentiality | End-to-end encryption — relay sees only ciphertext, never plaintext | Noise XX handshake, ChaCha20-Poly1305 |

**How RavenFabric helps:** All communication is mutually authenticated and end-to-end
encrypted. The relay is cryptographically incapable of accessing plaintext data.

---

## Article 17 — Right to Erasure ('Right to be Forgotten')

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Erasure without undue delay | `purge_entries_before()` and `delete_entries_by_filter()` APIs for targeted deletion | `FileAuditLogger` deletion methods |
| Reasonable steps to inform controllers | Audit log records deletion operations as new entries | Deletion creates audit trail entries |
| Exceptions (legal obligations, public interest) | Policy can restrict deletion operations to authorized callers only | RBAC on deletion operations |

**How RavenFabric helps:** Organizations can delete audit entries on request, with
full audit trail of the deletion itself. Deletion operations are themselves logged
for accountability.

---

## Article 32 — Security of Processing

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Pseudonymisation and encryption | End-to-end encryption via Noise XX, no plaintext on wire | `SecureChannel` in `rf-crypto` |
| Confidentiality, integrity, availability, resilience | Multi-transport architecture with automatic failover, tamper-evident audit logs | Transport diversity, HMAC-chained audit |
| Ability to restore availability | DTN offline queue (SQLite persistence), reconnect with backoff | `PersistentDtnQueue`, exponential backoff + jitter |
| Regular testing | Comprehensive test suite (1,423+ tests), fuzz testing (4 targets) | CI pipeline, `cargo test`, fuzz targets |
| Encryption at rest | Private keys stored with 0600 permissions, zeroed on drop | `Zeroize` implementation, file permissions |

**How RavenFabric helps:** The system provides enterprise-grade security controls
including encryption, access control, audit logging, and resilience mechanisms.

---

## Article 33 — Personal Data Breach Notification

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| Breach detection | Tamper detection (MAC failures, latency anomalies), health check probes | `HeartbeatStatus::TamperDetected`, `LatencyAnomaly` |
| Notification to supervisory authority | Alert rules with webhook destinations (Slack, PagerDuty, OpsGenie, generic) | `rf-audit` alert module |
| Documentation of breaches | Structured audit logging of all security events | `AuditEntry` with full context |

**How RavenFabric helps:** Security events are detected, logged, and can trigger
automated alerts to configured destinations for rapid incident response.

---

## Article 35 — Data Protection Impact Assessment

| Requirement | RavenFabric Capability | Evidence |
|-------------|------------------------|----------|
| DPIA for high-risk processing | Compliance report generation for audit and incident reconstruction | `ReportGenerator` in `rf-audit/src/compliance.rs` |
| Systematic description of processing | Architecture documentation, data flow diagrams, security properties | `ARCHITECTURE.md`, `CONNECTIVITY.md`, `SECURITY.md` |
| Necessity and proportionality assessment | Policy engine enforces least-privilege, data minimisation via output limits | `ResourceLimits`, deny-by-default |

**How RavenFabric helps:** The comprehensive documentation and compliance reporting
capabilities support organizations in conducting DPIAs for their RavenFabric deployment.

---

## Implementation Guidance

### Recommended Configuration for GDPR Compliance

```toml
[agent]
# Enable constrained mode for data minimisation
constrained = true

[audit]
# Retention: 90 days for operational audit logs
max_age_days = 90
# Buffer capacity for in-memory queue
buffer_capacity = 4096
# Flush interval
flush_interval_secs = 5

[policy]
# Define purpose-specific command patterns
[policy.commands.allow]
patterns = [
    "^systemctl status .*",      # Service monitoring
    "^journalctl --unit=.*",     # Log inspection
    "^df -h",                    # Disk usage
    "^free -m",                  # Memory monitoring
]
```

### Operational Procedures

1. **Define retention schedules** — Configure `max_age` in `CollectorConfig` to match
   your data retention policy (e.g., 90 days for operational logs, 12 months for
   security audit logs).
2. **Regular purging** — Use `purge_entries_before()` in scheduled maintenance tasks
   to enforce retention limits.
3. **Access control** — Restrict who can perform deletion operations via policy RBAC.
4. **Audit the auditors** — All configuration changes and deletion operations are
   themselves logged in the audit trail.
5. **DPIA documentation** — Reference this mapping document in your Data Protection
   Impact Assessment for RavenFabric deployment.

---

## Limitations

| Area | Limitation | Mitigation |
|------|------------|------------|
| PII classification | RavenFabric does not automatically classify or tag PII in audit data | Organizations should configure command patterns to avoid capturing PII |
| Consent management | No built-in consent tracking mechanism | Consent management should be handled by the organization's existing systems |
| Data portability | No automated data portability export for data subjects | Use `export_json()` / `export_csv()` on filtered audit data for manual export |
| DPA framework | No built-in Data Processing Agreement management | DPA should be established between controller and processor separately |
