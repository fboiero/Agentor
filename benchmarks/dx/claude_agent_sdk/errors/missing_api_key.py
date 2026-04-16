# SPDX-License-Identifier: AGPL-3.0-only
"""Bug 2: Missing API key — ANTHROPIC_API_KEY not set.

Claude Agent SDK error quality analysis:
The SDK checks for the API key at client construction time (Anthropic()).
If not set, it raises:
  anthropic.AuthenticationError: The api_key client option must be either
  passed in to instantiate the Anthropic object, or set via the
  ANTHROPIC_API_KEY environment variable
  (Construction-time check — not deferred to first call.)
Score: file/line — 0 (Python traceback shows client = Anthropic() line)
       names problem — 10 (exact env var name spelled out in error message)
       suggests fix — 9 (tells you exactly how to fix: env var or constructor arg)
Total diagnostic score: 19/30 → 6.3/10

The SDK is notably better than LangChain here: it fails fast at construction,
not at first API call. The error message names the env var AND the alternative
(pass key to constructor). This is the second-best error in the benchmark
for this scenario, behind Argentor's explicit "export ANTHROPIC_API_KEY=sk-ant-..."
"""
import os
import anthropic

# BUG: unset API key
os.environ.pop("ANTHROPIC_API_KEY", None)

# Error fires HERE at construction — SDK validates eagerly
client = anthropic.Anthropic()

response = client.messages.create(
    model="claude-sonnet-4-5",
    max_tokens=256,
    messages=[{"role": "user", "content": "Hello"}],
)
print(response.content[0].text)
