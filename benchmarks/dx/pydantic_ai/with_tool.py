"""PydanticAI — agent with one tool (get_weather).

Net LOC: 16
Requires: pip install pydantic-ai

PydanticAI uses @agent.tool decorator. The function signature IS the
schema — Pydantic introspects type hints. No manual schema definition.
This is the most ergonomic tool-definition API in the benchmark.
"""
from pydantic_ai import Agent, RunContext

agent = Agent(
    "anthropic:claude-sonnet-4-5",
    system_prompt="You are a weather assistant.",
)

@agent.tool_plain
def get_weather(city: str) -> str:
    """Return current weather for a city."""
    return f"Weather in {city}: sunny, 22°C"

result = agent.run_sync("What's the weather in Paris?")
print(result.data)

# --- LOC count (net, no blanks/comments) ---
# import: 1
# agent = ... (4 lines): 4
# @agent.tool_plain + def + return: 3
# result = ...; print: 2
# TOTAL: 10 net LOC — shortest tool-agent implementation in this benchmark
