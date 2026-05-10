# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.2.x   | Yes       |
| 0.1.x   | No        |

## Reporting a Vulnerability

If you discover a security vulnerability in RavenFabric, please report it responsibly.

**Do not open a public GitHub issue for security vulnerabilities.**

Instead, use one of the following methods:

1. **GitHub Private Vulnerability Reporting**: Go to the [Security Advisories](https://github.com/egkristi/RavenFabric/security/advisories) page and click "Report a vulnerability".
2. **Email**: Contact the maintainer directly at erling@rognsund.no.

### What to include

- Description of the vulnerability
- Steps to reproduce
- Affected versions
- Potential impact
- Suggested fix (if any)

### Response timeline

- **Acknowledgment**: Within 48 hours
- **Assessment**: Within 7 days
- **Fix release**: Within 30 days for critical issues

### Scope

Security issues of particular interest:

- Authentication bypasses (Noise XX handshake)
- Policy enforcement bypasses (deny-by-default circumvention)
- Key material leakage (private keys, sealed secrets)
- Path traversal via symlinks
- Remote code execution without policy approval
- Relay decryption of end-to-end payloads
- Audit log tampering or deletion
- Prompt injection bypassing the injection detector
- Approval workflow bypass (hash binding, TTL, single-use)
- CRDT policy convergence leading to unintended permission grants
- Cross-session information leakage in MCP server
- Rate limiting bypass

---

## Security Architecture

### Cryptography

- **Protocol**: Noise_XX_25519_ChaChaPoly_BLAKE2s (same core as WireGuard)
- **Key agreement**: X25519 (RFC 7748)
- **AEAD**: ChaCha20-Poly1305 (RFC 8439)
- **Hash**: BLAKE2s (Noise), SHA-256 (content addressing, OTP hashing)
- **Post-quantum**: Hybrid KEM (ML-KEM + X25519 via HKDF-SHA256) for harvest-now-decrypt-later resistance
- **Secrets at rest**: ChaCha20-Poly1305 sealed secret store, keys zeroed on drop via `write_volatile`
- **Session resumption**: 0-RTT with ticket store, use-count replay protection

### Policy Engine

- **Deny-by-default**: No command executes without an explicit allow rule
- **Dual enforcement**: Controller pre-flight check + agent local re-check (agent is final authority)
- **Immutable deny**: Catastrophic commands (rm -rf, mkfs, dd, fork bomb) blocked by `SecurityPolicy` — cannot be overridden by any RBAC role
- **Injection detection**: Base64/hex obfuscation, homoglyphs, shell evasion, exfiltration markers
- **Anomaly detection**: Per-identity behavioral baseline with velocity, novelty, timing, escalation scoring
- **Symlink resolution**: All paths canonicalized before policy check (prevent traversal)

### Build Security

- `unsafe_code = "forbid"` at workspace level (individual crates opt in with justification)
- `clippy::unwrap_used = "warn"`, `clippy::pedantic = "warn"`
- CodeQL scanning on every push
- Dependabot alerts enabled
- CI: cargo clippy, cargo test, cargo fmt, MSRV check, cross-compile, binary size gate, coverage
- Release builds: LTO, single codegen-unit, stripped, `panic = "abort"`

### Supply Chain

- Cargo.lock committed (reproducible builds)
- Dependabot configured for weekly checks
- No pre-built binaries from untrusted sources in build pipeline
- Static musl linking on Linux (no dynamic library dependencies)
- See [docs/compliance/standards/supply-chain.md](docs/compliance/standards/supply-chain.md) for full details

---

## Compliance Documentation

Detailed security framework mappings are maintained in [docs/compliance/](docs/compliance/):

| Framework | Document |
|-----------|----------|
| NIST SP 800-207 (Zero Trust) | [frameworks/nist-800-207-zta.md](docs/compliance/frameworks/nist-800-207-zta.md) |
| NIS2 Directive (EU) | [frameworks/nis2-directive.md](docs/compliance/frameworks/nis2-directive.md) |
| NSM Grunnprinsipper (Norway) | [frameworks/nsm-grunnprinsipper.md](docs/compliance/frameworks/nsm-grunnprinsipper.md) |
| OWASP ASVS 4.0.3 Level 2 | [frameworks/owasp-asvs-l2.md](docs/compliance/frameworks/owasp-asvs-l2.md) |
| CIS Controls v8.1 | [frameworks/cis-controls-v8.1.md](docs/compliance/frameworks/cis-controls-v8.1.md) |
| MITRE ATT&CK | [frameworks/mitre-attack-coverage.md](docs/compliance/frameworks/mitre-attack-coverage.md) |
| Cryptographic Standards | [standards/cryptographic-standards.md](docs/compliance/standards/cryptographic-standards.md) |
| Supply Chain | [standards/supply-chain.md](docs/compliance/standards/supply-chain.md) |
