# Changelog

All notable changes to RavenFabric will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Crypto**: Noise XX handshake (Noise_XX_25519_ChaChaPoly_BLAKE2s) with wire protocol
- **Crypto**: SecureChannel with concurrent read/write via split Mutex pattern
- **Crypto**: StaticKey management (generate, load, save, zeroed on drop)
- **Transport**: Driver trait with AsyncStream abstraction
- **RPC**: Request/Response types with msgpack serialization
- **Audit**: Structured JSON-lines audit logging (FileAuditLogger)
- **Policy**: YAML policy loading with deny-by-default enforcement
- **Policy**: Command pattern matching (regex) and filesystem path checks with symlink resolution
- **Executor**: Command execution with policy enforcement, timeout, output limiting
- **Bootstrap**: OTP generation and validation (single-use, hash-stored, TTL-enforced)
- **CLI**: Skeleton with exec/dev/status subcommands
- **CI**: GitHub Actions (check, fmt, clippy, test, coverage, MSRV)
- **CI**: Cross-platform release workflow (Linux, macOS, ARM64)
- **CI**: CodeQL security scanning
- **CI**: Dependabot for Cargo and GitHub Actions
