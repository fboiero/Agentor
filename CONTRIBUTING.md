# Contributing to Agentor

Thank you for your interest in contributing to Agentor! This document provides guidelines for contributing to the project.

## Code of Conduct

Be respectful, constructive, and inclusive. We welcome contributors from all backgrounds and experience levels.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/Agentor.git`
3. Create a feature branch: `git checkout -b feature/your-feature`
4. Make your changes
5. Run tests: `cargo test --workspace`
6. Run lints: `cargo clippy --workspace`
7. Format code: `cargo fmt --all`
8. Commit and push
9. Open a Pull Request

## Development Setup

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs))
- For WASM skills: `rustup target add wasm32-wasip1`

### Build & Test

```bash
cargo build --workspace       # Build all 13 crates
cargo test --workspace        # Run all tests
cargo clippy --workspace      # Zero warnings policy
cargo fmt --all -- --check    # Check formatting
```

## Project Structure

```
crates/
  agentor-core/          # Base types (Message, ToolCall, etc.)
  agentor-security/      # Capability-based security
  agentor-session/       # Session management
  agentor-skills/        # Skill trait + WASM runtime
  agentor-agent/         # AgentRunner (agentic loop)
  agentor-channels/      # Messaging adapters
  agentor-gateway/       # HTTP/WebSocket server
  agentor-builtins/      # Built-in skills
  agentor-memory/        # Vector memory
  agentor-mcp/           # MCP client + proxy
  agentor-orchestrator/  # Multi-agent orchestration
  agentor-compliance/    # GDPR, ISO, DPGA
  agentor-cli/           # CLI binary
```

## Coding Standards

### Rust
- Use `rustfmt` defaults — do not customize formatting
- Zero `clippy` warnings — CI enforces this
- Write tests for all new public functions
- Use `thiserror` for error types, `anyhow` only in the CLI
- Prefer `async` with `tokio` runtime

### Security
- All skills must use capability-based permissions
- Never allow unbounded resource access
- Validate inputs at system boundaries
- Use `agentor_security::Sanitizer` for untrusted content

### Tests
- Unit tests go in the same file (`#[cfg(test)]`)
- Use `#[tokio::test]` for async tests
- Name tests descriptively: `test_<what>_<scenario>`

## Pull Request Process

1. **One PR per feature/fix** — keep PRs focused
2. **Describe what and why** — not just what changed, but why
3. **Include tests** — new features must have tests
4. **Update docs** — if you change public APIs, update documentation
5. **Pass CI** — all tests, clippy, and fmt checks must pass

## Areas Where We Need Help

- Additional channel adapters (Matrix, WhatsApp)
- MCP server integrations
- WASM skill examples
- Documentation translations (especially Spanish)
- Security audits and penetration testing
- Performance benchmarks
- DPGA nomination process support

## Reporting Issues

- Use GitHub Issues
- Include: steps to reproduce, expected vs actual behavior, Rust version, OS
- For security vulnerabilities, please email directly instead of opening a public issue

## License

By contributing, you agree that your contributions will be licensed under the AGPL-3.0-only license.
