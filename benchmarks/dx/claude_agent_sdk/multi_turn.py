# SPDX-License-Identifier: AGPL-3.0-only
"""Claude Agent SDK — multi-turn conversation with history.

Net LOC: 20
Requires: pip install anthropic

The SDK requires you to manually build the messages list and append
each assistant response before the next user turn. No built-in session
or history manager — that's the developer's job.
"""
import anthropic

client = anthropic.Anthropic()

messages = []

def turn(user_input: str) -> str:
    messages.append({"role": "user", "content": user_input})
    response = client.messages.create(
        model="claude-sonnet-4-5",
        max_tokens=512,
        system="You are a helpful coding assistant.",
        messages=messages,
    )
    assistant_text = response.content[0].text
    messages.append({"role": "assistant", "content": assistant_text})
    return assistant_text

r1 = turn("What is a closure in Rust?")
print(f"Turn 1: {r1}")

r2 = turn("Can you give me a code example?")
print(f"Turn 2: {r2}")

r3 = turn("How does that differ from Python closures?")
print(f"Turn 3: {r3}")

# --- LOC count (net, no blanks/comments) ---
# import anthropic: 1
# client = ...: 1
# messages = []: 1
# def turn (8 lines): 8
# r1/print + r2/print + r3/print: 6
# TOTAL: 17 net LOC
#
# Honest note: manual history management is ~10 lines of boilerplate.
# All other frameworks abstract this. Argentor provides InMemorySessionStore
# so the caller calls run_turn() without touching the messages list.
