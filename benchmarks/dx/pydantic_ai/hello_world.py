"""PydanticAI — minimal Hello World agent.

Net LOC: 8
Requires: pip install pydantic-ai
NOTE: Requires ANTHROPIC_API_KEY in environment.

PydanticAI is refreshingly minimal. An Agent wraps the model directly.
run_sync is the synchronous entry point.
"""
from pydantic_ai import Agent

agent = Agent(
    "anthropic:claude-sonnet-4-5",
    system_prompt="You are a helpful assistant.",
)

result = agent.run_sync("Hello, what can you do?")
print(result.data)

# --- LOC count (net, no blanks/comments) ---
# from pydantic_ai import Agent                           1
# agent = Agent(                                          1
#     "anthropic:claude-sonnet-4-5",                     1
#     system_prompt="You are a helpful assistant.",       1
# )                                                       1
# result = agent.run_sync("Hello, what can you do?")     1
# print(result.data)                                      1
# TOTAL: 7 net LOC — joint-shortest with Claude Agent SDK
