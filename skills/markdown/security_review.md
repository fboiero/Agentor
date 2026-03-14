---
name: security_review
description: Security review checklist for code review
group: coding
---

When called, provide this security review checklist for the given code:

## OWASP Top 10 Checks
1. **Injection** — Are there any places where user input flows into commands, queries, or file paths without sanitization?
2. **Broken Authentication** — Are credentials stored securely? Are sessions managed properly?
3. **Sensitive Data Exposure** — Is sensitive data encrypted at rest and in transit?
4. **XXE** — Is XML parsing disabled or configured securely?
5. **Broken Access Control** — Are capability-based permission checks enforced?
6. **Security Misconfiguration** — Are default credentials or debug modes left enabled?
7. **XSS** — Is output properly escaped?
8. **Insecure Deserialization** — Is deserialization from untrusted sources avoided?
9. **Known Vulnerabilities** — Are dependencies up to date? (`cargo audit`)
10. **Insufficient Logging** — Are security-relevant events logged via `AuditLog`?

## Rust-Specific
- No `unsafe` blocks unless absolutely necessary and documented
- No `unwrap()` on user-controlled data
- Use `secrecy` crate for sensitive values
- Validate all inputs at system boundaries
- Use constant-time comparison for secrets
