# SPDX-License-Identifier: AGPL-3.0-only
"""Bug 3: Malformed prompt template — unclosed {{variable}} placeholder.

Claude Agent SDK error quality analysis:
The SDK takes system prompts as raw strings. No template engine.
A malformed placeholder like {{name} (unclosed) is passed verbatim to
the LLM. The SDK performs no validation — the string is sent as-is.
  (No error raised. Behaviour: LLM sees the broken template literally.)
Score: file/line — 0 (no error raised)
       names problem — 0 (no feedback at all)
       suggests fix — 0 (developer must find the broken template manually)
Total diagnostic score: 0/30 → 0.0/10

Same result as Argentor: raw strings are not validated.
Neither framework uses a template engine, so both share this weakness.
Frameworks that DO catch this: LangChain (raises ValueError at construction).
"""
import anthropic

client = anthropic.Anthropic()

# BUG: unclosed template placeholder — {{name} missing closing brace
# The SDK sends this string verbatim to Claude. No error is raised.
malformed_system = "You are an assistant for {{name}. Answer their question."

response = client.messages.create(
    model="claude-sonnet-4-5",
    max_tokens=256,
    system=malformed_system,  # sent as-is — no validation
    messages=[{"role": "user", "content": "Hello"}],
)
print(response.content[0].text)
