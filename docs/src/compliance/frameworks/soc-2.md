# SOC 2 — Compliance Mapping

> This document maps RavenFabric's capabilities to the SOC 2 Trust Services Criteria.

**RavenFabric version:** v1.0.0-beta.6
**Standard:** SOC 2 (System and Organization Controls 2) — Trust Services Criteria
**Applicability:** Service organizations that handle customer data
**Last updated:** 2026-07-18

---

## Context

SOC 2 reports evaluate a service organization's controls against the Trust Services
Criteria (TSC). RavenFabric is a secure remote execution and mesh networking agent —
it is not itself a "SOC 2 certified" product. However, organizations deploying
RavenFabric can leverage its capabilities to meet SOC 2 control objectives.

**Important:** RavenFabric is a tool that helps organizations achieve SOC 2 compliance
for their remote management infrastructure. A SOC 2 report requires an independent
auditor's assessment of the organization's controls — RavenFabric provides the
technical foundation for many of those controls.

---

## Common Criteria (CC) Series

### CC1 — Control Environment

| Criterion | RavenFabric Capability | Evidence |
|-----------|------------------------|----------|
| CC1.1 — Integrity and ethical values | Deny-by-default policy engine enforces ethical boundaries — no operation occurs without authorization | `RpcPolicy::check_command()` |
| CC1.2 — Board oversight | Audit log provides complete record for governance review | `FileAuditLogger`, compliance reports |
| CC1.3 — Organizational structure | RBAC with admin/operator/viewer/auditor roles maps to organizational structure | `rbac.rs` in `rf-policy` |
| CC1.4 — Competence | Comprehensive documentation, architecture guides, compliance mappings | `docs/` directory |
| CC1.5 — Accountability | Every action logged with caller identity — full accountability | `AuditEntry::caller_key` |

**How RavenFabric helps:** The policy engine operationalizes the organization's
ethical and integrity standards into enforceable technical controls.

---

### CC2 — Communication and Information

| Criterion | RavenFabric Capability | Evidence |
|-----------|------------------------|----------|
| CC2.1 — Information objectives | Structured audit logging captures all relevant operational data | JSON-lines audit format |
| CC2.2 — Internal communication | Audit log accessible to authorized reviewers, SIEM integration | SIEM exporters (Splunk, Elasticsearch, Datadog) |
| CC2.3 — External communication | Compliance report generation for stakeholders | `ReportGenerator` with JSON/CSV export |
| CC2.4 — Communication of roles | Policy YAML files define roles and responsibilities in code | RBAC configuration |

**How RavenFabric helps:** Comprehensive audit logging and reporting ensures
relevant information reaches the right stakeholders.

---

### CC3 — Risk Assessment

| Criterion | RavenFabric Capability | Evidence |
|-----------|------------------------|----------|
| CC3.1 — Risk identification | Anomaly detection (velocity, novelty, timing, escalation), tamper detection | Anomaly detection module |
| CC3.2 — Risk analysis | Security self-audit (17 tests), fuzz testing (4 targets) | Test suite, fuzz targets |
| CC3.3 — Risk response | Automatic transport migration on compromise detection, alert rules | `ConnectionManager::tamper_detected()`, alert webhooks |
| CC3.4 — Risk monitoring | Health check probes, Prometheus metrics, heartbeat status | `/metrics` endpoint, `HeartbeatStatus` |

**How RavenFabric helps:** Continuous risk monitoring through anomaly detection,
health probes, and automated response mechanisms.

---

### CC4 — Monitoring Activities

| Criterion | RavenFabric Capability | Evidence |
|-----------|------------------------|----------|
| CC4.1 — Monitoring | Prometheus metrics endpoint, health check probes, structured logging | `/metrics`, health checks |
| CC4.2 — Evaluation | Compliance report generation, audit log review, SIEM integration | `ReportGenerator`, SIEM exporters |
| CC4.3 — Remediation | Alert rules with webhook destinations, automatic capability reduction | Alert module, anomaly response |

**How RavenFabric helps:** Continuous monitoring with automated alerting and
remediation capabilities.

---

### CC5 — Control Activities

| Criterion | RavenFabric Capability | Evidence |
|-----------|------------------------|----------|
| CC5.1 — Control selection | YAML policy engine with allow/deny rules, RBAC, resource limits | `rf-policy` crate |
| CC5.2 — Technology controls | Noise XX mutual auth, end-to-end encryption, deny-by-default | `rf-crypto`, `SecureChannel` |
| CC5.3 — Policy deployment | Policy hot-reload via SIGHUP, versioned YAML config | `RpcPolicy::reload()` |
| CC5.4 — Segregation of duties | RBAC prevents any single role from having all capabilities | Admin/operator/viewer/auditor roles |

**How RavenFabric helps:** Technical controls are defined in policy, enforced
at runtime, and auditable for review.

---

### CC6 — Logical and Physical Access

| Criterion | RavenFabric Capability | Evidence |
|-----------|------------------------|----------|
| CC6.1 — Logical access | Noise XX mutual authentication, cryptographic identity per agent | `StaticKey`, XX handshake |
| CC6.2 — Physical access | N/A — software tool, physical security is organizational | — |
| CC6.3 — Role-based access | RBAC with granular permissions per role | `rbac.rs` |
| CC6.4 — User access termination | Session timeouts, key revocation via policy | Configurable timeouts |
| CC6.5 — Physical security | Key file permissions (0600), HSM/TPM key storage | File permissions, `HsmKeyProvider` |
| CC6.6 — Authentication | Strong mutual authentication via Noise XX | XX pattern (both sides prove identity) |
| CC6.7 — Data encryption | End-to-end encryption, encryption at rest for keys | Noise XX, file permissions |

**How RavenFabric helps:** Cryptographic identity and mutual authentication
provide strong logical access controls.

---

### CC7 — System Operations

| Criterion | RavenFabric Capability | Evidence |
|-----------|------------------------|----------|
| CC7.1 — System operations | Health check probes, heartbeat status, Prometheus metrics | `/health`, `/ready`, `/metrics` |
| CC7.2 — Change detection | Policy hot-reload, versioned configuration, audit trail | `RpcPolicy::reload()` |
| CC7.3 — Incident management | Alert rules, webhook destinations, tamper detection | Alert module, `TamperDetected` |
| CC7.4 — Incident recovery | DTN offline queue, reconnect with backoff, transport failover | `PersistentDtnQueue`, exponential backoff |
| CC7.5 — Incident analysis | Compliance report generation, incident reconstruction | `ReportGenerator::IncidentReconstruction` |

**How RavenFabric helps:** Comprehensive system operations monitoring with
incident detection, alerting, and recovery capabilities.

---

### CC8 — Change Management

| Criterion | RavenFabric Capability | Evidence |
|-----------|------------------------|----------|
| CC8.1 — Change authorization | Policy changes require explicit YAML edits, audit trail of changes | Versioned YAML config |
| CC8.2 — Change testing | Comprehensive test suite (1,423+ tests), CI pipeline | `cargo test`, CI |
| CC8.3 — Change deployment | Auto-update with staged rollout, canary, health-check gates | `AutoUpdateConfig` |
| CC8.4 — Change tracking | Audit log records policy changes and configuration updates | `AuditEntry` for policy changes |
| CC8.5 — Emergency changes | Policy hot-reload without restart, emergency revocation | `RpcPolicy::reload()`, secret revocation |

**How RavenFabric helps:** Changes are tested, deployed in controlled rollouts,
and fully auditable.

---

### CC9 — Business Continuity

| Criterion | RavenFabric Capability | Evidence |
|-----------|------------------------|----------|
| CC9.1 — Business continuity | Multi-transport architecture with automatic failover | Transport diversity |
| CC9.2 — Disaster recovery | DTN offline queue (SQLite persistence), reconnect with backoff | `PersistentDtnQueue` |
| CC9.3 — Backup communications | Transport diversity — WebSocket, QUIC, WireGuard, exotic transports | `TransportCatalog` |
| CC9.4 — Recovery testing | Integration tests, soak tests (26 days on rpi5) | `rf-integration-tests` |

**How RavenFabric helps:** Multi-transport architecture ensures connectivity
even when primary paths fail.

---

## Availability Criteria (A Series)

### A1 — Availability

| Criterion | RavenFabric Capability | Evidence |
|-----------|------------------------|----------|
| A1.1 — Availability commitments | Health check probes, heartbeat status, Prometheus metrics | `/health`, `/ready`, `/metrics` |
| A1.2 — Capacity management | Resource limits (`maxOutputBytes`, `timeoutSeconds`), constrained mode | `ResourceLimits` |
| A1.3 — Incident handling | Alert rules, automatic failover, transport migration | Alert module, failover |
| A1.4 — Recovery | Reconnect with exponential backoff + jitter, DTN offline queue | Reconnect loop, `PersistentDtnQueue` |
| A1.5 — Monitoring | Prometheus metrics, health checks, structured logging | `/metrics`, health probes |

**How RavenFabric helps:** Continuous availability monitoring with automated
failover and recovery mechanisms.

---

## Confidentiality Criteria (C Series)

### C1 — Confidentiality

| Criterion | RavenFabric Capability | Evidence |
|-----------|------------------------|----------|
| C1.1 — Confidentiality commitments | End-to-end encryption, relay sees only ciphertext | Noise XX, ChaCha20-Poly1305 |
| C1.2 — Data classification | Policy rules restrict access by data sensitivity | Path/network/command policies |
| C1.3 — Access restrictions | Mutual authentication, RBAC, deny-by-default | `RpcPolicy`, `rbac.rs` |
| C1.4 — Encryption | E2E encryption in transit, encryption at rest for keys | Noise XX, file permissions |
| C1.5 — Data masking | Output size limiting, command pattern filtering | `ResourceLimits::max_output_bytes` |

**How RavenFabric helps:** Strong encryption and access controls protect
confidential data throughout its lifecycle.

---

## Processing Integrity Criteria (PI Series)

### PI1 — Processing Integrity

| Criterion | RavenFabric Capability | Evidence |
|-----------|------------------------|----------|
| PI1.1 — Processing accuracy | Policy engine ensures only authorized operations execute | `check_command()` enforcement |
| PI1.2 — Data validation | Command pattern validation, path policy enforcement | `CommandRule`, `PathRule` |
| PI1.3 — Error handling | Structured error types (`PolicyError`, `AuditError`), no unwrap in library code | `thiserror` types |
| PI1.4 — Completeness | Audit logging captures every action — no silent failures | `FileAuditLogger` |
| PI1.5 — Timeliness | Execution timeout enforcement, configurable timeouts | `ResourceLimits::timeout_seconds` |

**How RavenFabric helps:** Processing integrity is ensured through policy
enforcement, validation, and comprehensive audit logging.

---

## Implementation Guidance

### Recommended Configuration for SOC 2 Compliance

```toml
[agent]
# Enable comprehensive audit logging
audit_path = "/var/log/ravenfabric/audit.jsonl"

[audit]
# SOC 2 typically requires 12-month retention
max_age_days = 365
# Large buffer for high-throughput environments
buffer_capacity = 8192
# Frequent flushing for near-real-time monitoring
flush_interval_secs = 2

[policy]
# Define roles with appropriate access levels
[policy.rbac]
roles = ["admin", "operator", "viewer", "auditor"]

# Restrict commands to approved operations
[policy.commands.allow]
patterns = [
    "^systemctl (status|start|stop|restart) .*",
    "^journalctl.*",
    "^tail -n [0-9]+ .*",
    "^df -h",
    "^free -m",
]

[monitoring]
# Enable Prometheus metrics
metrics_addr = "0.0.0.0:9090"

# Configure alert destinations
[monitoring.alerts]
destinations = ["splunk", "pagerduty"]
```

### Operational Procedures

1. **Access reviews** — Regularly review audit logs for unauthorized access attempts.
   Use SIEM integration for automated monitoring and alerting.
2. **Change management** — All policy changes are logged. Review policy changes
   through the change management process before deployment.
3. **Incident response** — Configure alert rules for security events. Test incident
   response procedures regularly.
4. **Availability monitoring** — Monitor health check endpoints and Prometheus metrics.
   Set up alerts for availability degradation.
5. **Vendor management** — Review RavenFabric dependencies and supply chain security
   (see `docs/compliance/standards/supply-chain.md`).

---

## Limitations

| Area | Limitation | Mitigation |
|------|------------|------------|
| SOC 2 audit | RavenFabric cannot produce a SOC 2 report — that requires an independent auditor | Use compliance reports and audit logs as evidence for the auditor |
| SLA measurement | No built-in SLA tracking or reporting | Integrate Prometheus metrics with external SLA monitoring tools |
| Formal change management | No built-in change request/approval workflow | Use external change management system; policy YAML changes are version-controlled |
| Vendor assessment | No automated vendor risk assessment | Review supply chain documentation (`supply-chain.md`) manually |
| Penetration testing | No built-in penetration testing | Conduct regular penetration tests as part of the SOC 2 program |
