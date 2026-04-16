# SPDX-License-Identifier: AGPL-3.0-only
"""Bug 1: Typo in tool name — "get_wether" in system prompt, "get_weather" registered.

PydanticAI error quality analysis:
PydanticAI registers tools by function name from the @agent.tool decorator.
If the system prompt instructs the LLM to call "get_wether" (typo) but the
registered tool is "get_weather", PydanticAI raises at runtime:
  pydantic_ai.exceptions.UnexpectedModelBehavior: Unexpected tool name: 'get_wether'
  (No list of available tools in the error.)
Score: file/line — 5 (traceback shows the run_sync call — one level away from user code)
       names problem — 7 (names the unexpected tool, but not available alternatives)
       suggests fix — 3 (no list of registered tools shown)
Total diagnostic score: 15/30 → 5.0/10

PydanticAI at least raises an explicit typed exception with the wrong tool name.
Better than the SDK (silent) but worse than Argentor (shows available list).
"""
from pydantic_ai import Agent, RunContext

agent = Agent(
    "anthropic:claude-sonnet-4-5",
    system_prompt="You are a weather assistant. Use the get_wether tool.",  # BUG: typo
)

@agent.tool_plain
def get_weather(city: str) -> str:  # NOTE: correct name registered
    """Return current weather for a city."""
    return f"Weather in {city}: sunny, 22°C"

# LLM will call "get_wether" (as instructed) — not found → exception
result = agent.run_sync("What's the weather in Paris?")
print(result.data)
