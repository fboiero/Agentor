"""Bug 2: Missing API key — ANTHROPIC_API_KEY not set.

LangChain error quality analysis:
LangChain defers API key validation to the underlying SDK.
The error from langchain-anthropic:
  anthropic.AuthenticationError: Error code: 401 - {'type': 'error',
    'error': {'type': 'authentication_error', 'message': 'invalid x-api-key'}}
  (No mention of which env var to set. No suggestion.)
Score: file/line — 0 (network call, not construction-time)
       names problem — 4 (mentions "api-key" but not the env var name)
       suggests fix — 0 (no guidance on where to set the key)
Total diagnostic score: 4/30 → 1.3/10

LangChain does NOT validate env vars eagerly at construction time.
You only discover the missing key when the first API call fires.
"""
from langchain_anthropic import ChatAnthropic
from langchain_core.messages import HumanMessage
import os

# BUG: unset API key
os.environ.pop("ANTHROPIC_API_KEY", None)

llm = ChatAnthropic(model="claude-sonnet-4-5")  # No error here yet

# Error fires here, at invoke time — not at construction
response = llm.invoke([HumanMessage(content="Hello")])
print(response.content)
