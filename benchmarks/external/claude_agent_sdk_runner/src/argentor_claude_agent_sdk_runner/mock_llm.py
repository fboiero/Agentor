"""Mock LLM backend — no real API calls. Simulates 50ms latency to be
apples-to-apples with the Argentor benchmark's mock backend."""
from __future__ import annotations

import time
from typing import Any


class MockLlm:
    """Minimal interface: `.invoke(prompt) -> str`."""

    def __init__(self, latency_ms: int = 50, name: str = "mock-claude-agent-sdk"):
        self.latency_ms = latency_ms
        self.name = name
        self.call_count = 0

    def invoke(self, prompt: str, **kwargs: Any) -> str:
        """Echo the prompt with a canned prefix, after simulated latency."""
        time.sleep(self.latency_ms / 1000.0)
        self.call_count += 1
        truncated = prompt[:80] if prompt else ""
        return f"[claude-agent-sdk-mock] processed: {truncated}"

    def estimate_input_tokens(self, prompt: str) -> int:
        """~4 chars ≈ 1 token heuristic."""
        return len(prompt) // 4

    def estimate_output_tokens(self, output: str) -> int:
        return len(output) // 4
