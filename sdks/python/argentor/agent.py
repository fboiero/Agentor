"""
Argentor Agent SDK -- wrap the argentor CLI binary for agentic execution.

Like Claude Agent SDK wraps Claude Code, this wraps the ``argentor`` binary
and communicates via NDJSON over stdin/stdout.

Usage::

    from argentor.agent import query, AgentOptions

    async for event in query("Fix the bug in auth.py", AgentOptions(
        provider="claude",
        model="claude-sonnet-4-20250514",
        api_key="sk-...",
    )):
        print(event)
"""

import asyncio
import json
import shutil
from dataclasses import dataclass, field
from typing import AsyncIterator, Optional
from pathlib import Path


@dataclass
class AgentOptions:
    """Configuration for an agent query."""

    provider: str = "claude"
    model: str = "claude-sonnet-4-20250514"
    api_key: str = ""
    system_prompt: Optional[str] = None
    max_turns: int = 10
    temperature: float = 0.7
    tools: Optional[list[str]] = None  # None = builtins
    permission_mode: str = "default"  # "default", "strict", "permissive", "plan"
    working_directory: Optional[str] = None
    mcp_servers: Optional[list[dict]] = None
    include_streaming: bool = False
    argentor_binary: Optional[str] = None  # path to argentor binary

    @classmethod
    def claude(cls, api_key: str, **kwargs) -> "AgentOptions":
        """Preset for Anthropic Claude models."""
        return cls(provider="claude", model="claude-sonnet-4-20250514", api_key=api_key, **kwargs)

    @classmethod
    def openai(cls, api_key: str, **kwargs) -> "AgentOptions":
        """Preset for OpenAI models."""
        return cls(provider="openai", model="gpt-4o", api_key=api_key, **kwargs)

    @classmethod
    def gemini(cls, api_key: str, **kwargs) -> "AgentOptions":
        """Preset for Google Gemini models."""
        return cls(provider="gemini", model="gemini-2.0-flash", api_key=api_key, **kwargs)

    @classmethod
    def ollama(cls, model: str = "llama3", **kwargs) -> "AgentOptions":
        """Preset for local Ollama models (no API key required)."""
        return cls(provider="ollama", model=model, api_key="", **kwargs)


@dataclass
class AgentEvent:
    """An event emitted during agent execution.

    Events flow as NDJSON lines from the ``argentor --headless`` subprocess.
    Each event has a *type* and a free-form *data* dict.
    """

    type: str  # "system", "assistant", "tool_use", "tool_result", "stream", "result", "error", "guardrail"
    data: dict = field(default_factory=dict)

    @property
    def text(self) -> str:
        """Convenience accessor for the main text payload."""
        return self.data.get("text", self.data.get("output", ""))

    @property
    def is_done(self) -> bool:
        """True when this event signals the end of the conversation."""
        return self.type == "result"

    @property
    def is_error(self) -> bool:
        """True when this event signals an error."""
        return self.type == "error"


def _find_argentor_binary(custom_path: Optional[str] = None) -> str:
    """Locate the ``argentor`` CLI binary.

    Resolution order:
    1. Explicit *custom_path* if provided.
    2. ``argentor`` on ``$PATH``.
    3. Common Cargo build output directories.
    4. ``/usr/local/bin/argentor``.

    Raises ``FileNotFoundError`` if none of the above exist.
    """
    if custom_path:
        return custom_path

    found = shutil.which("argentor")
    if found:
        return found

    for path in [
        "./target/release/argentor",
        "./target/debug/argentor",
        "/usr/local/bin/argentor",
    ]:
        if Path(path).exists():
            return path

    raise FileNotFoundError(
        "argentor binary not found. Install it or set argentor_binary in AgentOptions."
    )


async def query(prompt: str, options: Optional[AgentOptions] = None) -> AsyncIterator[AgentEvent]:
    """Run an agent query and yield events as they arrive.

    This is the primary entry-point, analogous to ``claude_agent_sdk.query()``.
    It spawns ``argentor --headless`` as a subprocess, sends an init + query
    message over stdin (NDJSON), and yields ``AgentEvent`` objects parsed from
    the stdout NDJSON stream.

    Args:
        prompt: The user prompt / task description.
        options: Agent configuration.  Defaults to ``AgentOptions()`` which
            uses the Claude provider.

    Yields:
        ``AgentEvent`` objects as the agent works.

    Example::

        async for event in query("What files are here?", AgentOptions.claude("sk-...")):
            if event.type == "assistant":
                print(event.text)
            elif event.is_done:
                print(f"Done: {event.text}")
    """
    if options is None:
        options = AgentOptions()

    binary = _find_argentor_binary(options.argentor_binary)

    # Spawn argentor in headless mode
    process = await asyncio.create_subprocess_exec(
        binary,
        "--headless",
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        cwd=options.working_directory,
    )

    assert process.stdin is not None
    assert process.stdout is not None

    # ---- send init message ---------------------------------------------------
    init_msg = {
        "type": "init",
        "provider": options.provider,
        "model": options.model,
        "api_key": options.api_key,
        "system_prompt": options.system_prompt,
        "max_turns": options.max_turns,
        "temperature": options.temperature,
        "tools": options.tools,
        "permission_mode": options.permission_mode,
        "mcp_servers": options.mcp_servers,
        "working_directory": options.working_directory,
    }
    process.stdin.write((json.dumps(init_msg) + "\n").encode())
    await process.stdin.drain()

    # ---- send query message --------------------------------------------------
    query_msg = {
        "type": "query",
        "prompt": prompt,
        "include_streaming": options.include_streaming,
    }
    process.stdin.write((json.dumps(query_msg) + "\n").encode())
    await process.stdin.drain()

    # ---- read NDJSON responses -----------------------------------------------
    while True:
        line = await process.stdout.readline()
        if not line:
            break

        try:
            data = json.loads(line.decode().strip())
            event = AgentEvent(type=data.get("type", "unknown"), data=data)
            yield event

            if event.is_done or event.is_error:
                break
        except json.JSONDecodeError:
            continue

    # ---- cleanup -------------------------------------------------------------
    process.stdin.close()
    await process.wait()


async def query_simple(prompt: str, options: Optional[AgentOptions] = None) -> str:
    """Run a query and return just the final output string.

    This is a convenience wrapper around :func:`query` for callers that only
    need the final result and don't care about intermediate events.

    Raises ``RuntimeError`` if the agent returns an error event.
    """
    async for event in query(prompt, options):
        if event.is_done:
            return event.text
        if event.is_error:
            raise RuntimeError(f"Agent error: {event.data.get('message', 'Unknown error')}")
    return ""


# ---------------------------------------------------------------------------
# Convenience one-liners
# ---------------------------------------------------------------------------


async def ask_claude(prompt: str, api_key: str) -> str:
    """Run a prompt with Anthropic Claude and return the final text."""
    return await query_simple(prompt, AgentOptions.claude(api_key))


async def ask_openai(prompt: str, api_key: str) -> str:
    """Run a prompt with OpenAI and return the final text."""
    return await query_simple(prompt, AgentOptions.openai(api_key))


async def ask_gemini(prompt: str, api_key: str) -> str:
    """Run a prompt with Google Gemini and return the final text."""
    return await query_simple(prompt, AgentOptions.gemini(api_key))


async def ask_ollama(prompt: str, model: str = "llama3") -> str:
    """Run a prompt with a local Ollama model and return the final text."""
    return await query_simple(prompt, AgentOptions.ollama(model))
