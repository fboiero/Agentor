# argentor-python

Python bindings for the **Argentor** AI agent framework, powered by [PyO3](https://pyo3.rs/) and built with [maturin](https://www.maturin.rs/).

Prototype in Python, deploy in Rust.

## Installation

```bash
pip install maturin
cd crates/argentor-python
maturin develop
```

## Quick Start

```python
import argentor

# Framework version
print(argentor.version())  # "0.1.0"

# List all available built-in skills
print(argentor.available_skills())
# ['browser', 'calculator', 'code_analysis', 'data_validator', 'datetime',
#  'diff', 'dns_lookup', 'encode_decode', 'file_read', 'file_write', 'git',
#  'hash', 'http_fetch', 'human_approval', 'json_query', 'prompt_guard',
#  'regex', 'rss_reader', 'secret_scanner', 'shell', 'summarizer',
#  'test_runner', 'text_transform', 'uuid_generator', 'web_scraper',
#  'web_search']
```

## Skill Registry

Execute any registered skill by name with JSON arguments:

```python
registry = argentor.SkillRegistry()
print(f"{registry.skill_count()} skills loaded")
print(registry.list_skills())

# Calculator
result = registry.execute("calculator", '{"operation": "add", "a": 2, "b": 3}')
print(result.content)   # "5"
print(result.is_error)  # False

# Hash
result = registry.execute("hash", '{"operation": "sha256", "input": "hello"}')
print(result.content)   # "2cf24dba5fb0a30e..."

# JSON query
result = registry.execute("json_query", '{"operation": "get", "data": {"a": {"b": 1}}, "path": "a.b"}')
print(result.content)   # "1"
```

## Direct Skill Wrappers

For frequently used skills, direct Python classes provide a more ergonomic API:

### Calculator

```python
calc = argentor.Calculator()
print(calc.evaluate("2 + 3 * 4"))  # "14"
```

### HashTool

```python
h = argentor.HashTool()
print(h.sha256("hello world"))
print(h.sha512("hello world"))
print(h.hmac_sha256("message", "secret-key"))
```

### JsonQuery

```python
jq = argentor.JsonQuery()
print(jq.get('{"users": [{"name": "Alice"}]}', "users.0.name"))  # "Alice"
print(jq.keys('{"a": 1, "b": 2}'))
```

## Guardrails

Production-grade input/output validation with PII detection, prompt injection
prevention, and toxicity filtering:

```python
guard = argentor.GuardrailEngine()

# Check input for PII
result = guard.check_input("My SSN is 123-45-6789")
print(result.passed)          # False
print(result.violations)      # ['[BLOCK] pii_detection: PII detected: SSN ...']
print(result.sanitized_text)  # "My SSN is [SSN]"

# Check output for toxicity
result = guard.check_output("Here is a safe response.")
print(result.passed)  # True

# Static PII redaction
sanitized, matches_json = argentor.GuardrailEngine.redact_pii("Email me at test@example.com")
print(sanitized)     # "Email me at [EMAIL]"
print(matches_json)  # '[{"kind":"EMAIL","span":[12,28],"original":"test@example.com"}]'
```

## Sessions

Track conversation history with typed messages:

```python
session = argentor.Session()
print(session.id)          # UUID string
print(session.created_at)  # ISO-8601 timestamp

session.add_user_message("Hello!")
session.add_assistant_message("Hi there!")
session.add_system_message("You are a helpful assistant.")

print(session.message_count())  # 3

for msg in session.messages():
    print(f"[{msg.role}] {msg.content}")
```

## Architecture

This crate is a thin PyO3 bridge that wraps the Rust crates:

- `argentor-core` -- Message, ToolCall, ToolResult types
- `argentor-session` -- Session management
- `argentor-skills` -- Skill trait and registry
- `argentor-builtins` -- 25+ built-in skills
- `argentor-agent` -- Guardrail engine
- `argentor-security` -- Permission system

All computation runs natively in Rust. Python only handles the FFI boundary.

## License

AGPL-3.0-only
