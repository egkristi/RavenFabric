# Supply Chain Security

> This document describes RavenFabric's supply chain security practices,
> targeting SLSA Level 3 and providing transparency for downstream consumers.

**RavenFabric version:** v0.2.0  
**Last updated:** 2026-05-10

---

## SLSA Framework (Supply-chain Levels for Software Artifacts)

### Current Level: SLSA Level 2 (Build Service)

| SLSA Requirement | Level | Status | Implementation |
|------------------|-------|--------|----------------|
| Source — Version controlled | L1 | Implemented | Git (GitHub), all history preserved |
| Source — Verified history | L2 | Implemented | Branch protection, signed commits encouraged |
| Source — Two-person reviewed | L3 | Partial | PRs required, single-maintainer currently |
| Build — Scripted build | L1 | Implemented | `cargo build --release` in CI |
| Build — Build service | L2 | Implemented | GitHub Actions (hosted runners) |
| Build — Ephemeral environment | L3 | Implemented | GitHub-hosted runners are ephemeral |
| Build — Isolated | L3 | Implemented | Each job in clean container |
| Provenance — Available | L1 | Implemented | GitHub Actions logs public |
| Provenance — Authenticated | L2 | Planned | Sigstore attestation |
| Provenance — Non-falsifiable | L3 | Planned | Rekor transparency log |

### Target: SLSA Level 3 (Hardened Build)

Remaining work for Level 3:
- [ ] Sigstore signing of all release artifacts
- [ ] in-toto attestation generation in CI
- [ ] Provenance published to Rekor transparency log
- [ ] Two-person review policy enforced

---

## Software Bill of Materials (SBOM)

### Format: CycloneDX 1.5

| Property | Value |
|----------|-------|
| Format | CycloneDX 1.5 (JSON) |
| Generator | `cargo-cyclonedx` (planned) |
| Scope | All direct and transitive dependencies |
| Frequency | Generated per release |
| Distribution | Attached to GitHub Release assets |

### Current Dependency Profile

RavenFabric uses a minimal, audited dependency tree:

| Category | Key Dependencies |
|----------|-----------------|
| Crypto | `snow` (Noise), `rand` |
| Async | `tokio`, `tokio-util` |
| Transport | `tokio-tungstenite`, `quinn`, `rustls` |
| Serialization | `rmp-serde`, `serde`, `serde_json`, `serde_yaml` |
| CLI | `clap` |
| Observability | `tracing`, `tracing-subscriber` |
| Policy | `regex` |
| System info | `sysinfo` |
| Config | `toml` |

### Dependency Security

| Practice | Status |
|----------|--------|
| Dependabot alerts | Enabled (auto-PRs for vulnerabilities) |
| `cargo audit` in CI | Planned |
| Minimal dependency policy | Active (no unnecessary deps) |
| No `unsafe` in application code | Enforced (only in vetted deps) |
| Feature-gated heavy deps | Active (QUIC, WireGuard behind features) |

---

## Artifact Signing

### Planned: Sigstore / Cosign

| Property | Value |
|----------|-------|
| Signing method | Keyless (Sigstore OIDC identity) |
| Identity | GitHub Actions OIDC token |
| Transparency | Rekor public log |
| Verification | `cosign verify-blob` |
| Scope | All release binaries, containers, SBOMs |

### Verification (once implemented)

```bash
# Verify a release binary
cosign verify-blob \
  --certificate-identity-regexp 'https://github.com/egkristi/RavenFabric' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
  --signature ravenfabric-linux-amd64.sig \
  ravenfabric-linux-amd64
```

---

## Build Reproducibility

| Property | Status |
|----------|--------|
| Deterministic builds | Partial (Cargo.lock pinned, but not bit-for-bit reproducible yet) |
| Locked dependencies | `Cargo.lock` committed and used in CI |
| Pinned toolchain | `rust-toolchain.toml` with exact version |
| Pinned CI actions | SHA-pinned (e.g., `actions/checkout@v4`) |

---

## Vulnerability Disclosure

| Property | Value |
|----------|-------|
| Reporting method | GitHub Security Advisories (private) |
| Security policy | `SECURITY.md` in repository root |
| security.txt | `website/.well-known/security.txt` (RFC 9116) |
| Response SLA | Acknowledge within 72 hours |
| CVE assignment | Via GitHub (CNA) |
| Disclosure timeline | 90 days coordinated disclosure |

---

## CI/CD Security

| Control | Implementation |
|---------|----------------|
| Branch protection | `main` branch protected |
| Required checks | Format, clippy, test, check (all must pass) |
| No force push | Enforced on `main` |
| Secrets management | GitHub Encrypted Secrets, no plaintext in code |
| Runner isolation | GitHub-hosted (ephemeral, clean VM per job) |
| Dependency caching | Cargo cache (read-only for PRs from forks) |
| CodeQL scanning | Enabled (Rust analysis) |
| Secret scanning | Enabled (alerts on exposed secrets) |

---

## Compliance Relevance

| Framework | Relevant Requirements |
|-----------|----------------------|
| **NIS2 Art. 21(2)(d)** | Supply chain security |
| **NIST SP 800-218 (SSDF)** | Secure Software Development Framework |
| **EO 14028 Sec. 4** | Software supply chain security (US federal) |
| **SLSA** | Build integrity and provenance |
| **CIS Control 16** | Application Software Security |
| **ISO 27001 A.14** | System acquisition, development, and maintenance |
