# SPDX-License-Identifier: AGPL-3.0-only
"""Claude Agent SDK — minimal Hello World agent.

Net LOC: 7
Requires: pip install anthropic
NOTE: Requires ANTHROPIC_API_KEY in environment.

The Anthropic SDK gives the most direct path to the Claude API.
There is no "agent" abstraction — you call the messages API directly.
This is simultaneously the simplest and most explicit option.
"""
import anthropic

client = anthropic.Anthropic()

response = client.messages.create(
    model="claude-sonnet-4-5",
    max_tokens=256,
    system="You are a helpful assistant.",
    messages=[{"role": "user", "content": "Hello, what can you do?"}],
)

print(response.content[0].text)

# --- LOC count (net, no blanks/comments) ---
# import anthropic                                            1
# client = anthropic.Anthropic()                             1
# response = client.messages.create(                         1
#     model="claude-sonnet-4-5",                             1
#     max_tokens=256,                                        1
#     system="You are a helpful assistant.",                  1
#     messages=[{"role": "user", "content": "..."}],         1
# )                                                          1
# print(response.content[0].text)                            1
# TOTAL: 9 net LOC
#
# Honest note: this is a raw API call, not an "agent".
# The Claude Agent SDK does not provide high-level agent abstractions
# (AgentRunner, tool registry, session management). Those are exactly
# what Argentor provides on top of this baseline.
