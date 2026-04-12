# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 1.0.x   | ✅ Active support  |
| < 1.0   | ❌ Not supported   |

Only the latest minor version of the 1.x line receives security updates. Users on older versions should upgrade.

## Reporting a Vulnerability

**Do NOT open a public GitHub issue for security vulnerabilities.**

Please report security issues privately to: **fboiero@xcapit.com**

Include in your report:
- Description of the vulnerability
- Steps to reproduce (proof of concept if possible)
- Affected version(s)
- Your assessment of severity (Low / Medium / High / Critical)
- Any suggested mitigation

### Response Timeline

| Phase | Timeline |
|-------|----------|
| Acknowledgment of report | Within 48 hours |
| Initial assessment | Within 7 days |
| Status updates | Every 14 days until resolved |
| Patch release | Critical: within 14 days. High: within 30 days. Medium: next minor release. |
| Public disclosure | After patch is released, with credit to reporter (unless they prefer to remain anonymous) |

### Disclosure Policy

We follow a **90-day responsible disclosure** timeline by default. After 90 days from initial report, the vulnerability will be publicly disclosed regardless of patch status, unless extension is mutually agreed.

## Security Hall of Fame

Researchers who responsibly disclose verified vulnerabilities will be credited here (with permission):

_No reports yet._

## Bug Bounty

We currently do not offer a paid bug bounty program. Recognition is limited to public credit in this file and the release notes.

## Argentor Security Architecture

Argentor is designed with security as a primitive, not an add-on:

- **WASM sandbox** for all skill plugins (wasmtime + WASI)
- **Capability-based permissions** — every skill declares what it can access
- **Network allowlist** — outbound HTTP restricted to approved hosts
- **Path traversal protection** with directory scoping
- **Input sanitization** strips control characters before processing
- **Audit logging** for every tool call (append-only JSONL)
- **Encrypted credential storage** (AES-256-GCM with PBKDF2)
- **TLS/mTLS** support for production deployments
- **Guardrails pipeline** — PII detection, prompt injection blocking, output validation

For details, see [docs/TECHNICAL_REPORT.md](docs/TECHNICAL_REPORT.md).

## Compliance

Argentor includes built-in compliance modules for:
- GDPR (consent tracking, right to erasure, data portability)
- ISO 27001 (access control, incident response)
- ISO 42001 (AI system inventory, transparency)
- DPGA (digital public goods alliance indicators)

If your security report relates to compliance violations, please mention which framework in your email.
