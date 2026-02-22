---
name: test_guidelines
description: Testing guidelines for Agentor
group: coding
prompt_injection: true
---

Follow these testing guidelines:

- Write unit tests for every public function
- Use `#[tokio::test]` for async functions
- Test both success and error paths
- Use `tempfile` for tests that need filesystem access
- Mock external dependencies (LLM calls, network)
- Name tests descriptively: `test_<function>_<scenario>_<expected>`
- Verify error types, not just that an error occurred
- Keep test data minimal and focused
- Use `assert_eq!` with descriptive messages when possible
