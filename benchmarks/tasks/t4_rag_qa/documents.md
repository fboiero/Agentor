# Argentor Credential Vault Specification

## Overview

The Argentor credential vault stores API tokens and sensitive secrets at rest
with strong cryptographic guarantees. All credentials are encrypted before
being written to disk, and decryption requires the operator-provided master
passphrase combined with a per-record random salt.

## Key Derivation

Argentor derives the encryption key from the operator passphrase using
**PBKDF2-HMAC-SHA256**. PBKDF2 is a deliberately slow, iterative key derivation
function designed to frustrate brute-force attacks against the passphrase.

The current minimum iteration count is **100,000**, matching the Argentor v1.0
implementation. However, OWASP's 2025 guidance recommends **600,000 iterations**
for PBKDF2 with SHA-256 to maintain security against modern GPU-based attacks.
We are tracking an issue to raise the default in v1.2.

## Encryption

After key derivation, the vault uses **AES-256-GCM** (Galois/Counter Mode) for
authenticated encryption. AES-GCM provides both confidentiality and integrity:
any tampering with the ciphertext is detected at decryption time via the GCM
authentication tag.

Each record uses a fresh random 12-byte nonce, preventing nonce reuse across
records even if the same key is used to encrypt multiple credentials.

## Security Boundaries

The vault does NOT protect against:

- Memory disclosure while credentials are decrypted (use OS-level process
  isolation for that)
- An attacker who obtains both the ciphertext AND the passphrase
- Side-channel attacks on the hardware running the agent

## References

- NIST SP 800-132: Recommendation for Password-Based Key Derivation
- OWASP Password Storage Cheat Sheet (2025 revision)
- RFC 5869 (HKDF, for completeness — Argentor uses PBKDF2, not HKDF)
