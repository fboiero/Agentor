# SPDX-License-Identifier: AGPL-3.0-only
"""Bug 3: Malformed prompt template — unclosed {{variable}} placeholder.

PydanticAI error quality analysis:
PydanticAI system prompts accept plain strings or Jinja2-style templates
(via TemplateStr). For plain string system_prompt, no template parsing
occurs — the string is sent verbatim to the LLM.
  (No error raised for plain strings. Behaviour: LLM sees broken template.)
Score: file/line — 0 (no error raised)
       names problem — 0 (no feedback)
       suggests fix — 0 (silent pass-through)
Total diagnostic score: 0/30 → 0.0/10

Same result as Argentor and Claude SDK: raw strings are not validated.
PydanticAI would catch this IF the user used a TemplateStr annotation,
but plain string system_prompt skips all template parsing.
This is an honest limitation shared by multiple frameworks.
"""
from pydantic_ai import Agent

agent = Agent(
    "anthropic:claude-sonnet-4-5",
    # BUG: unclosed template variable — {{name} missing closing brace
    # For a plain string system_prompt, PydanticAI sends this verbatim.
    system_prompt="You are an assistant for {{name}. Answer their question.",
)

# No error at construction. The malformed template reaches the LLM.
result = agent.run_sync("Hello")
print(result.data)
