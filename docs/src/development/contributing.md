# Contributing

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork: `git clone https://github.com/YOUR_NAME/RavenFabric.git`
3. Create a branch: `git checkout -b feat/my-feature`
4. Make changes, ensuring tests pass
5. Submit a pull request

## Requirements

- Rust 1.88+ (Edition 2024)
- All changes must pass: `cargo test --all && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
- Commit before creating a PR — the CI will validate your code

## Code Standards

- **Edition**: Rust 2024, MSRV 1.88
- **Error handling**: `thiserror` in libraries, `anyhow` only in binaries
- **Async**: Tokio runtime, `async-trait` for async trait methods
- **Logging**: `tracing` crate (`info!`, `warn!`, `error!`) — never `println!` in libraries
- **Serialization**: msgpack (wire), YAML (config/policy), JSON (audit)
- **No `unwrap()`** in library code — use `?` or `expect()` with justification
- **Platform portability**: use `#[cfg(target_os)]` for OS-specific code

## Commit Conventions

```
feat: add QUIC transport driver #5
fix: prevent symlink traversal in path policy
refactor: extract codec into separate module
docs: update architecture overview
test: add negative tests for OTP validation
```

Reference GitHub Issues in commits where applicable.

## Issue Tracking

- All planned changes are tracked as GitHub Issues before work begins
- If you discover work that should be done but is out of scope, create an Issue
- After every push, check GitHub Actions for pipeline failures

## Security

Security is the top priority. Every contribution must:

- Respect deny-by-default policy engine
- Not introduce `unsafe` code (workspace-level `forbid(unsafe_code)` — exceptions require discussion)
- Include tests for security-critical paths (positive AND negative)
- Not bypass validation, authentication, or authorization

## License

RavenFabric is licensed under AGPLv3. By contributing, you agree that your contributions will be licensed under the same terms.

## Reporting Vulnerabilities

See [SECURITY.md](../../../SECURITY.md) for responsible disclosure process.
