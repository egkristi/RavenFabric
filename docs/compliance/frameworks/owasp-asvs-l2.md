# OWASP ASVS 4.0.3 — Application Security Verification (Level 2)

> This document maps RavenFabric against the OWASP Application Security
> Verification Standard (ASVS) 4.0.3, targeting Level 2 compliance.

**RavenFabric version:** v0.3.0  
**Standard:** OWASP ASVS 4.0.3  
**Target level:** Level 2 (Standard)  
**Last updated:** 2026-05-10

---

## Applicability Note

OWASP ASVS is designed for web applications. RavenFabric is a systems-level
tool (CLI + agent + relay). Many web-specific controls (CSRF, session cookies,
HTTP headers) do not apply. This mapping covers the applicable sections.

---

## V1: Architecture, Design and Threat Modeling

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 1.1.1 | Identify all application components | Pass | Cargo workspace with 13 explicit crates, dependency graph documented |
| 1.1.2 | High-level architecture defined | Pass | Architecture in README.md, ROADMAP.md, copilot-instructions.md |
| 1.2.1 | Authentication mechanism identified | Pass | Noise XX mutual auth, OTP enrollment |
| 1.4.1 | All trust boundaries identified | Pass | Relay boundary (zero-knowledge), agent boundary (final authority), policy boundary |
| 1.6.1 | Cryptographic architecture documented | Pass | `standards/cryptographic-standards.md` |
| 1.11.1 | Security-relevant components defined | Pass | rf-crypto, rf-policy, rf-audit, rf-bootstrap |

---

## V2: Authentication

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 2.1.1 | No credential stuffing possible | Pass | No passwords — cryptographic keys only |
| 2.2.1 | Anti-automation for auth | Pass | Rate limiting (20/IP/min), OTP single-use + TTL |
| 2.5.1 | Secrets not exposed in error messages | Pass | Errors return typed enums, no key material in messages |
| 2.7.1 | OTP has time limit | Pass | Configurable TTL (default 3600s), enforced in `otp.rs` |
| 2.7.2 | OTP single-use | Pass | `used` flag set atomically on validation |
| 2.8.1 | Cryptographic authentication | Pass | Noise XX (X25519 key agreement, mutual proof of identity) |
| 2.10.1 | No service-to-service hardcoded creds | Pass | Keys generated per-agent, stored with file permissions |

---

## V3: Session Management

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 3.1.1 | Session token never exposed in URL | Pass | No URLs — binary RPC over encrypted channel |
| 3.3.1 | Session timeout | Pass | Noise sessions bounded by connection lifetime |
| 3.5.1 | Token-based sessions use signed tokens | N/A | Not token-based; cryptographic session via Noise |
| 3.7.1 | Session invalidation on logout | Pass | Connection close zeroes session state |

---

## V4: Access Control

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 4.1.1 | Least privilege enforced | Pass | Deny-by-default, explicit allow rules per command/path |
| 4.1.2 | Access control at trusted layer | Pass | Agent enforces locally — cannot be bypassed by controller |
| 4.1.3 | Deny by default | Pass | `RpcPolicy` returns Deny if no allow rule matches |
| 4.2.1 | Sensitive data access requires auth | Pass | Every RPC requires completed Noise handshake |
| 4.3.1 | Administrative interfaces protected | Pass | No admin web UI; all access via authenticated RPC |

---

## V5: Validation, Sanitization and Encoding

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 5.1.1 | Input validation at trusted boundary | Pass | Policy regex validates command strings before execution |
| 5.1.3 | Structured data strongly typed | Pass | msgpack deserialization with Rust type system (serde) |
| 5.2.1 | Untrusted HTML not rendered | N/A | No HTML output |
| 5.3.1 | OS command injection prevented | Pass | Commands checked against policy regex before execution. No string interpolation from untrusted input into commands. |
| 5.5.1 | Deserialization of untrusted data safe | Pass | rmp-serde with typed structs, max message size (8 MB) |

---

## V6: Stored Cryptography

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 6.1.1 | Regulated data encrypted at rest | Pass | Private keys file-permission protected (0600), memory-zeroed on drop |
| 6.2.1 | Approved algorithms used | Pass | X25519, ChaCha20-Poly1305, BLAKE2s (all IETF-standardized) |
| 6.2.2 | Industry-proven implementations | Pass | `snow` crate (audited Noise implementation) |
| 6.2.5 | Cryptographic keys properly managed | Pass | Generated from CSPRNG, atomic file write, zeroed on drop |
| 6.4.1 | Key generation uses CSPRNG | Pass | `rand` crate with OS entropy source |
| 6.4.2 | Keys stored securely | Pass | File permissions 0600, no plaintext in logs/errors |

---

## V7: Error Handling and Logging

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 7.1.1 | No sensitive info in error messages | Pass | Typed error enums, no key material exposed |
| 7.1.2 | No stack traces to users | Pass | Errors converted to RPC error responses |
| 7.2.1 | Security events logged | Pass | All policy decisions logged to audit (allow + deny) |
| 7.2.2 | Auth failures logged | Pass | Denied attempts logged with caller key |
| 7.3.1 | Logs contain relevant context | Pass | Timestamp, caller_key, action, command, decision, rule, duration |
| 7.3.4 | Logs include source identification | Pass | `caller_key` (Noise public key) identifies caller cryptographically |
| 7.4.1 | Logs protected from tampering | Pass | Append-only file mode + HMAC-SHA256 signed policy log with SHA-256 hash chain |

---

## V8: Data Protection

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 8.1.1 | Sensitive data identified | Pass | Private keys, OTP tokens, audit entries classified |
| 8.2.1 | Sensitive data not in logs | Pass | Command output not stored in audit log; only metadata |
| 8.3.1 | Sensitive data encrypted in transit | Pass | All communication Noise XX encrypted |
| 8.3.4 | Sensitive data cached minimized | Pass | No caching layer; direct execution model |

---

## V9: Communication

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 9.1.1 | TLS for all connections | Pass | Noise XX encryption (equivalent or stronger than TLS 1.3) |
| 9.1.2 | Strong cipher suites | Pass | ChaCha20-Poly1305 (256-bit), X25519 (128-bit security level) |
| 9.1.3 | Only current protocols | Pass | Single protocol version, no downgrade path |
| 9.2.1 | Mutual authentication | Pass | Noise XX (both sides verify static keys) |

---

## V10: Malicious Code

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 10.1.1 | No malicious code in source | Pass | Open source, CodeQL scanning, code review |
| 10.2.1 | No backdoors | Pass | Open source, all auth paths documented |
| 10.3.1 | Update mechanism secure | Partial | Release binaries signed (planned); no auto-update mechanism |

---

## V13: API and Web Service

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 13.1.1 | All requests authenticated | Pass | Noise handshake required before any RPC |
| 13.1.3 | Strong service authentication | Pass | Mutual cryptographic authentication (X25519) |
| 13.2.1 | Malformed content rejected | Pass | msgpack deserialization fails on invalid format |
| 13.2.2 | Message size limited | Pass | 8 MB max message, 65KB max frame |

---

## V14: Configuration

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 14.2.1 | Dependencies up to date | Pass | Dependabot enabled, CI runs weekly |
| 14.2.2 | No unnecessary features | Pass | Feature flags for QUIC, WireGuard, SQLite |
| 14.3.1 | No secrets in source code | Pass | Keys generated at runtime, secrets via env vars |
| 14.4.1 | HTTP security headers | N/A | No HTTP interface (metrics endpoint is internal-only) |

---

## Summary

| Section | Applicable Items | Pass | Partial | Fail | N/A |
|---------|-----------------|------|---------|------|-----|
| V1 Architecture | 6 | 6 | 0 | 0 | 0 |
| V2 Authentication | 7 | 7 | 0 | 0 | 0 |
| V3 Session | 4 | 3 | 0 | 0 | 1 |
| V4 Access Control | 5 | 5 | 0 | 0 | 0 |
| V5 Validation | 5 | 4 | 0 | 0 | 1 |
| V6 Cryptography | 6 | 6 | 0 | 0 | 0 |
| V7 Error/Logging | 6 | 6 | 0 | 0 | 0 |
| V8 Data Protection | 4 | 4 | 0 | 0 | 0 |
| V9 Communication | 4 | 4 | 0 | 0 | 0 |
| V10 Malicious Code | 3 | 2 | 1 | 0 | 0 |
| V13 API | 4 | 4 | 0 | 0 | 0 |
| V14 Configuration | 4 | 3 | 0 | 0 | 1 |
| **Total** | **58** | **54** | **1** | **0** | **3** |

**ASVS Level 2 conformance: 93% (54/58 applicable requirements pass)**

---

## Gaps

| Gap | Requirement | Status |
|-----|-------------|--------|
| ~~No cryptographic log signing~~ | V7.4.1 | Done — HMAC-SHA256 signed policy log with SHA-256 hash chain |
| No auto-update with integrity check | V10.3.1 | Open — Sigstore-verified updates planned |
