# SPDX-License-Identifier: AGPL-3.0-only
"""Claude Agent SDK — agent with one tool (get_weather).

Net LOC: 35
Requires: pip install anthropic

The SDK requires manual tool definition (JSON schema), manual tool dispatch
(if/elif on tool name), and a manual agentic loop. No decorator magic.
This is the most explicit (and verbose) approach in the benchmark — but
also the most transparent: every step is visible.
"""
import anthropic
import json

client = anthropic.Anthropic()

tools = [
    {
        "name": "get_weather",
        "description": "Return current weather for a city.",
        "input_schema": {
            "type": "object",
            "properties": {"city": {"type": "string", "description": "City name"}},
            "required": ["city"],
        },
    }
]

def get_weather(city: str) -> str:
    return f"Weather in {city}: sunny, 22°C"

messages = [{"role": "user", "content": "What's the weather in Paris?"}]

# Agentic loop: keep going until stop_reason is "end_turn"
while True:
    response = client.messages.create(
        model="claude-sonnet-4-5",
        max_tokens=256,
        system="You are a weather assistant.",
        tools=tools,
        messages=messages,
    )

    if response.stop_reason == "end_turn":
        for block in response.content:
            if hasattr(block, "text"):
                print(block.text)
        break

    # Collect tool calls and dispatch
    tool_results = []
    for block in response.content:
        if block.type == "tool_use":
            if block.name == "get_weather":
                result = get_weather(**block.input)
            else:
                result = f"Unknown tool: {block.name}"
            tool_results.append({"type": "tool_result", "tool_use_id": block.id, "content": result})

    messages.append({"role": "assistant", "content": response.content})
    messages.append({"role": "user", "content": tool_results})

# --- LOC count (net, no blanks/comments) ---
# import anthropic; import json: 2
# client = ...: 1
# tools = [...] (8 lines): 8
# def get_weather: 2
# messages = [...]: 1
# while True: + response = ... (5 lines): 6
# if stop_reason: ... break (4): 4
# for block / dispatch (6): 6
# messages.append x2: 2
# TOTAL: 32 net LOC
#
# Honest note: this is the VERBOSEST tool-agent in the benchmark.
# The agentic loop, manual JSON schema, and manual dispatch are all
# developer responsibility. LangChain/PydanticAI automate all of this.
# Argentor automates the loop and dispatch while keeping type safety.
