"""SSE (Server-Sent Events) streaming helpers for the Argentor SDK."""

from __future__ import annotations

import json
from typing import Any, AsyncIterator, Dict, Iterator


class SSEStream:
    """Parse a synchronous Server-Sent Events stream into Python dicts.

    Usage::

        with httpx.Client() as http:
            with http.stream("POST", url, json=payload) as resp:
                for event in SSEStream(resp.iter_lines()):
                    print(event)
    """

    def __init__(self, lines: Iterator[str]) -> None:
        self._lines = lines

    def __iter__(self) -> Iterator[Dict[str, Any]]:
        for line in self._lines:
            if line.startswith("data: "):
                data = line[len("data: ") :].strip()
                if data == "[DONE]":
                    return
                yield json.loads(data)


class AsyncSSEStream:
    """Parse an asynchronous Server-Sent Events stream into Python dicts.

    Usage::

        async with httpx.AsyncClient() as http:
            async with http.stream("POST", url, json=payload) as resp:
                async for event in AsyncSSEStream(resp.aiter_lines()):
                    print(event)
    """

    def __init__(self, lines: AsyncIterator[str]) -> None:
        self._lines = lines

    async def __aiter__(self) -> AsyncIterator[Dict[str, Any]]:
        async for line in self._lines:
            if line.startswith("data: "):
                data = line[len("data: ") :].strip()
                if data == "[DONE]":
                    return
                yield json.loads(data)
