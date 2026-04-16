# SPDX-License-Identifier: AGPL-3.0-only
"""Bug 2: Missing API key — ANTHROPIC_API_KEY not set.

PydanticAI error quality analysis:
PydanticAI defers API key validation to the underlying SDK (anthropic).
The error surfaces at run time (run_sync), not at Agent construction:
  anthropic.AuthenticationError: Error code: 401 - ...
  (No mention of which env var to set in the PydanticAI layer itself.)
Score: file/line — 0 (runtime error, not construction-time)
       names problem — 5 (error mentions "api-key" but not the env var name)
       suggests fix — 2 (no guidance from PydanticAI layer; SDK error is opaque)
Total diagnostic score: 7/30 → 2.3/10

PydanticAI does NOT validate env vars at Agent construction.
You only discover the missing key on first run_sync() call.
Slightly better than LangChain (same timing) because the error message
comes from the Anthropic SDK which at least names "api-key".
"""
import os
from pydantic_ai import Agent

# BUG: unset API key
os.environ.pop("ANTHROPIC_API_KEY", None)

agent = Agent(
    "anthropic:claude-sonnet-4-5",
    system_prompt="You are a helpful assistant.",
)
# No error here at Agent construction

# Error fires here, at run time
result = agent.run_sync("Hello, what can you do?")
print(result.data)
