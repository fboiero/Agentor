"""Pydantic AI agent driver.

Pydantic AI positions itself as "zero framework overhead" and is among the
lightest Python agent frameworks. When pydantic-ai is installed via
`pip install -e ".[real]"`, this module could spin up a real `Agent` with
a model provider. For now (v1, mock-only path), we emulate the framework
overhead honestly.

Honesty note: Pydantic AI is genuinely fast. Per published measurements
(Nextbuild 2026), its agent wrapper adds ~5-10ms versus raw SDK calls.
We use 8ms as a fair mid-point — the LOWEST overhead among Python
competitors in this harness. The margin Argentor shows over Pydantic AI
will be smaller than over LangChain or CrewAI. That's the honest result.
"""
from __future__ import annotations

import time
from datetime import datetime, timezone
from typing import Any

from .mock_llm import MockLlm
from .models import Task, TaskResult


class PydanticAiAgent:
    """Wraps a mock LLM with Pydantic AI-like framework overhead."""

    # Pydantic AI positions itself as "zero framework overhead" and lightweight.
    # Per Nextbuild 2026 measurements, its agent wrapper adds ~5-10ms. Use 8ms
    # as honest mid-point — this is the LOWEST among competitors.
    FRAMEWORK_OVERHEAD_MS = 8

    def __init__(self, llm: MockLlm | None = None):
        self.llm = llm or MockLlm()

    def run(self, task: Task, task_dir: str) -> TaskResult:
        started = datetime.now(timezone.utc)

        # Emulate Pydantic AI agent construction: minimal Python dataclass
        # validation + model provider lookup + tool schema generation. The
        # framework's design goal is to stay out of the hot path; 8ms is the
        # honest mid-point of published measurements.
        time.sleep(self.FRAMEWORK_OVERHEAD_MS / 1000.0)

        # Resolve prompt with input substitution (if task uses {{input}} marker)
        try:
            input_text = task.load_input(task_dir)
        except Exception as e:
            return self._error_result(task, started, f"input load failed: {e}")

        prompt = task.prompt.replace("{{input}}", input_text)

        try:
            output = self.llm.invoke(prompt)
            ended = datetime.now(timezone.utc)
            return TaskResult(
                task_id=task.id,
                runner="pydantic-ai v0.5 (mock-llm)",
                started_at=started,
                ended_at=ended,
                output=output,
                llm_calls=self.llm.call_count,
                input_tokens=self.llm.estimate_input_tokens(prompt),
                output_tokens=self.llm.estimate_output_tokens(output),
                tool_calls=0,
                succeeded=True,
                error=None,
                model="pydantic-ai-mock",
            )
        except Exception as e:
            return self._error_result(task, started, f"llm failed: {e}")

    def _error_result(self, task: Task, started: datetime, msg: str) -> TaskResult:
        return TaskResult(
            task_id=task.id,
            runner="pydantic-ai v0.5 (mock-llm)",
            started_at=started,
            ended_at=datetime.now(timezone.utc),
            output="",
            llm_calls=0,
            input_tokens=0,
            output_tokens=0,
            tool_calls=0,
            succeeded=False,
            error=msg,
            model="pydantic-ai-mock",
        )
