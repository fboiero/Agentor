# SPDX-License-Identifier: AGPL-3.0-only
"""PydanticAI — multi-turn conversation with history.

Net LOC: 14
Requires: pip install pydantic-ai

PydanticAI handles multi-turn by passing message_history from the
previous result. The API is explicit but minimal — no session store
needed; the caller maintains history through the result object.
"""
from pydantic_ai import Agent

agent = Agent(
    "anthropic:claude-sonnet-4-5",
    system_prompt="You are a helpful coding assistant.",
)

result1 = agent.run_sync("What is a closure in Rust?")
print(f"Turn 1: {result1.data}")

result2 = agent.run_sync("Can you give me a code example?", message_history=result1.new_messages())
print(f"Turn 2: {result2.data}")

result3 = agent.run_sync("How does that differ from Python closures?", message_history=result2.new_messages())
print(f"Turn 3: {result3.data}")

# --- LOC count (net, no blanks/comments) ---
# import: 1
# agent = ... (4 lines): 4
# result1 = ...; print: 2
# result2 = ... (message_history=result1.new_messages()); print: 2
# result3 = ...; print: 2
# TOTAL: 11 net LOC
#
# Honest note: PydanticAI's message_history API requires passing
# result.new_messages() manually per turn — slightly more explicit than
# LangChain's RunnableWithMessageHistory but much less ceremony overall.
# Joint-best for multi-turn simplicity alongside Claude Agent SDK.
