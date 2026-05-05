# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

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
- Policy enforcement bypasses
- Key material leakage
- Path traversal via symlinks
- Remote code execution without policy approval
- Relay decryption of end-to-end payloads
- Audit log tampering
