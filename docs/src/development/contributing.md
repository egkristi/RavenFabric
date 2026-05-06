# Contributing

Thank you for your interest in contributing to RavenFabric.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USER/RavenFabric.git`
3. Create a feature branch: `git checkout -b feat/my-feature`
4. Make your changes
5. Run tests: `cargo test`
6. Run linter: `cargo clippy`
7. Format code: `cargo fmt`
8. Push and open a Pull Request

## Code Standards

- **Language**: All code, comments, and documentation in English
- **Edition**: Rust 2024, MSRV 1.88
- **Error handling**: `thiserror` in libraries, `anyhow` only in binaries
- **Async**: Use `async-trait` for async trait methods
- **Logging**: `tracing` crate only — never `println!` in libraries
- **Tests**: Every public function must have at least one test
- **Security**: Every policy check, key validation, and crypto operation must have tests

## Commit Messages

Use conventional commits:

```
feat: add QUIC transport driver
fix: resolve symlink before path policy check
refactor: extract connection manager from driver
docs: add relay setup guide
test: add roundtrip test for DTN bundle
```

Reference issues: `feat: add QUIC transport #5`

## Security

- Security is always the top priority
- Never trade security for convenience
- No `unwrap()` in library code — use `?` and proper error types
- All types must be `Send + Sync`
- Deny-by-default — if unsure, deny

## Reporting Security Issues

See [SECURITY.md](https://github.com/egkristi/RavenFabric/blob/main/SECURITY.md) for responsible disclosure guidelines.
