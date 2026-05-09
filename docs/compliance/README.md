# RavenFabric — Security & Compliance Documentation

> This directory documents how RavenFabric maps to security frameworks,
> compliance regimes, and technical standards. These mappings help security
> architects and compliance officers evaluate RavenFabric for regulated environments.

## How to Read This

RavenFabric is an open-source project. It cannot itself be "certified" against
SOC 2, ISO 27001, or FedRAMP — those certifications apply to organizations
operating systems. However, RavenFabric is **designed so that organizations
deploying it can achieve compliance** with these frameworks.

This documentation covers three levels:

| Level | What | Directory |
|-------|------|-----------|
| **Technical Standards** | Cryptographic and protocol standards RavenFabric implements | `standards/` |
| **Security Frameworks** | Security control frameworks mapped to RavenFabric capabilities | `frameworks/` |

---

## Compliance Matrix (Summary)

| Framework | Coverage | Status | Document |
|-----------|----------|--------|----------|
| **NIST SP 800-207** (Zero Trust Architecture) | Core tenets implemented | Documented | [frameworks/nist-800-207-zta.md](frameworks/nist-800-207-zta.md) |
| **NIS2 Directive** (EU 2022/2555) | Article 21 measures addressed | Documented | [frameworks/nis2-directive.md](frameworks/nis2-directive.md) |
| **NSM Grunnprinsipper 2.1** | Mapped to Norwegian baseline | Documented | [frameworks/nsm-grunnprinsipper.md](frameworks/nsm-grunnprinsipper.md) |
| **OWASP ASVS 4.0.3** | Level 2 targeted | Documented | [frameworks/owasp-asvs-l2.md](frameworks/owasp-asvs-l2.md) |
| **CIS Controls v8.1** | 7 of 18 control groups addressed | Documented | [frameworks/cis-controls-v8.1.md](frameworks/cis-controls-v8.1.md) |
| **MITRE ATT&CK** | Defensive coverage documented | Documented | [frameworks/mitre-attack-coverage.md](frameworks/mitre-attack-coverage.md) |
| **NIST SP 800-53 Rev 5** | Moderate baseline partial | Planned | — |
| **ISO/IEC 27001:2022** | Annex A mapping | Planned | — |
| **SOC 2 Type II** | Trust Services Criteria | Planned (v1.0) | — |
| **FedRAMP** | Moderate baseline | Planned (v1.0) | — |

---

## Technical Standards Implemented

| Standard | Description | Status |
|----------|-------------|--------|
| Noise Protocol Framework r34 | XX pattern mutual authentication | Implemented |
| RFC 7748 (X25519) | Key agreement | Implemented via Noise XX |
| RFC 8439 (ChaCha20-Poly1305) | AEAD encryption | Implemented via Noise XX |
| BLAKE2s | Hash function | Implemented via Noise XX |
| RFC 8489 (STUN) | NAT type detection | Implemented |
| RFC 8445 (ICE) | Connectivity establishment | Implemented (candidate gathering) |
| RFC 9000 (QUIC) | Transport protocol | Implemented (quinn) |
| RFC 9116 (security.txt) | Security contact disclosure | Implemented |

See [standards/cryptographic-standards.md](standards/cryptographic-standards.md) for full details.

---

## Security Design Principles

RavenFabric's security architecture is built on these non-negotiable principles:

1. **Deny-by-default** — The policy engine denies anything not explicitly allowed
2. **Mutual authentication** — Every connection uses Noise XX (both sides verify)
3. **End-to-end encryption** — Relay sees only ciphertext, never plaintext
4. **Audit everything** — Every policy decision produces a structured log entry
5. **Zero trust networking** — No implicit trust based on network position
6. **Defense in depth** — Dual policy check (controller pre-flight + agent local)
7. **Least privilege** — Commands execute under policy-constrained scope
8. **Tamper evidence** — Append-only audit log, key zeroing on drop

---

## Architecture Security Properties

```
┌──────────────────────────────────────────────────────────────────┐
│                    SECURITY BOUNDARY                              │
│                                                                    │
│  Orchestrator ──→ Pre-flight Policy Check ──→ Encrypted RPC     │
│                                                                    │
│       │  Noise XX E2E (relay sees random bytes only)              │
│       ▼                                                            │
│                                                                    │
│  Agent (final authority)                                           │
│    ├── Local Policy Check (cannot be overridden by controller)   │
│    ├── Execute within resource limits                             │
│    ├── Append audit entry (structured JSON-lines)                │
│    └── Return encrypted result                                    │
│                                                                    │
│  Relay (zero-knowledge)                                           │
│    ├── Never decrypts payload                                     │
│    ├── Rate-limits connections (20/IP/min)                        │
│    └── HMAC-verifies meet tokens                                  │
│                                                                    │
└──────────────────────────────────────────────────────────────────┘
```

---

## For Evaluators

If you are evaluating RavenFabric for a regulated environment:

1. Start with the framework most relevant to your jurisdiction
2. Cross-reference with [standards/cryptographic-standards.md](standards/cryptographic-standards.md) for crypto details
3. File questions via [GitHub Security Advisories](https://github.com/egkristi/RavenFabric/security/advisories) (private reporting enabled)

---

## Document Versioning

These compliance documents track the `main` branch. Each document notes the
RavenFabric version it was written against. As features are added, mappings are
updated to reflect new capabilities.

Current version: **v0.1.4** (~50,000 LOC, 1,037 tests, 0 clippy warnings)
