"""argentor_client — SSE streaming helpers."""

import json
from typing import Iterator, AsyncIterator, Any


class SSEStream:
    """Parses a Server-Sent Events stream from raw lines."""

    def __init__(self, lines: Iterator[str]):
        self._lines = lines

    def __iter__(self) -> Iterator[dict]:
        for line in self._lines:
            if line.startswith("data: "):
                data = line[len("data: "):]
                stripped = data.strip()
                if stripped == "[DONE]":
                    return
                yield json.loads(stripped)


class AsyncSSEStream:
    """Parses a Server-Sent Events stream from async lines."""

    def __init__(self, lines: AsyncIterator[str]):
        self._lines = lines

    async def __aiter__(self) -> AsyncIterator[dict]:
        async for line in self._lines:
            if line.startswith("data: "):
                data = line[len("data: "):]
                stripped = data.strip()
                if stripped == "[DONE]":
                    return
                yield json.loads(stripped)
