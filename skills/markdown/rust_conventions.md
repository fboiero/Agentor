---
name: rust_conventions
description: Rust coding conventions for Agentor
group: coding
prompt_injection: true
---

Follow these Rust conventions when generating code:

- Use `AgentorResult<T>` for all fallible operations
- Never use `unwrap()` in production code — use `?` or explicit error handling
- Use `tracing` macros (`info!`, `warn!`, `error!`) for logging, not `println!`
- Prefer `Arc<dyn Trait>` for shared ownership of trait objects
- All public items must be documented with `///` doc comments
- Use `#[derive(Debug, Clone, Serialize, Deserialize)]` where appropriate
- Every skill must declare its required capabilities
- Tests go in `#[cfg(test)] mod tests` at the bottom of each file
- Use `tokio::test` for async tests
- Run `cargo clippy --workspace` with zero warnings before committing
