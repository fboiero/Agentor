# SPDX-License-Identifier: AGPL-3.0-only
"""Bug 1: Typo in tool name — "get_wether" in tool definition, "get_weather" in dispatch.

Claude Agent SDK error quality analysis:
With the SDK you own the dispatch loop. A typo means the dispatch never
fires and the loop either hangs (waiting for tool_result) or you get
an unhandled else branch. If you omit the else, the tool call silently
produces no result — the LLM receives an empty content block.
  (No error raised. Behaviour: infinite loop or silent empty result.)
Score: file/line — 0 (no error at all — silent failure)
       names problem — 0 (no error message, no suggestion)
       suggests fix — 0 (developer must debug manually)
Total diagnostic score: 0/30 → 0.0/10

This is the worst case in the benchmark for this scenario: silent failure.
When the LLM calls the tool and the dispatch doesn't match, the agentic
loop simply skips the tool_result, sending an empty response, which may
cause the LLM to retry or produce a confused answer — with no indication
to the developer that anything went wrong.
"""
import anthropic

client = anthropic.Anthropic()

tools = [
    {
        "name": "get_wether",  # BUG: typo — should be "get_weather"
        "description": "Return current weather for a city.",
        "input_schema": {
            "type": "object",
            "properties": {"city": {"type": "string"}},
            "required": ["city"],
        },
    }
]

def get_weather(city: str) -> str:  # NOTE: correct name in dispatch
    return f"Weather in {city}: sunny, 22°C"

messages = [{"role": "user", "content": "What's the weather in Paris?"}]

# The LLM will call "get_wether" (as listed in tools).
# The dispatch below checks "get_weather" (correct spelling) — no match.
# Result: tool_results is empty, loop continues with no useful content.
while True:
    response = client.messages.create(
        model="claude-sonnet-4-5",
        max_tokens=256,
        tools=tools,
        messages=messages,
    )
    if response.stop_reason == "end_turn":
        break

    tool_results = []
    for block in response.content:
        if block.type == "tool_use":
            if block.name == "get_weather":  # BUG: won't match "get_wether"
                result = get_weather(**block.input)
                tool_results.append({"type": "tool_result", "tool_use_id": block.id, "content": result})
            # No else — silent skip. Developer has no idea the call was missed.

    messages.append({"role": "assistant", "content": response.content})
    messages.append({"role": "user", "content": tool_results})
