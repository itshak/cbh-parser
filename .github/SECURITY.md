# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1.0 | :x:                |

## Reporting a Vulnerability

If you discover a security vulnerability within **cbvault**, please report it privately:

1. **Email:** Send details to [aistreltsov@outlook.com](mailto:aistreltsov@outlook.com) with the subject `[SECURITY] cbvault Vulnerability`.
2. **GitHub Security Advisory:** Alternatively, submit a private advisory through GitHub's Security tab at `https://github.com/itshak/cbvault/security/advisories`.

Please do **not** open public issues for sensitive security vulnerabilities. We will respond within 48 hours to assess and patch the issue.

cbvault parses untrusted database files by design. If you find an out-of-bounds read, unbounded allocation, or panic triggered by a malformed `.cbh`/`.c2cbh`/archive, that is a security bug: please report it rather than opening a public issue with the file attached.