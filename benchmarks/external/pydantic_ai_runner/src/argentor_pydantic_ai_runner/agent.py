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

from .cost_sim import simulate as simulate_cost
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

        # Cost track: short-circuit to the simulator.
        if task.kind == "cost":
            b = simulate_cost(
                framework="pydantic-ai",
                prompt=task.prompt,
                turns=max(task.simulated_turns, 1),
                tool_count=task.tool_count,
                context_bytes=task.context_size_bytes,
            )
            return TaskResult(
                task_id=task.id,
                runner="pydantic-ai v0.5 (mock-llm)",
                started_at=started,
                ended_at=datetime.now(timezone.utc),
                output=f"[pydantic-ai-cost-sim] turns={b.llm_calls}",
                llm_calls=b.llm_calls,
                input_tokens=b.prompt_tokens_sent,
                output_tokens=b.output_tokens,
                tool_calls=0,
                succeeded=True,
                error=None,
                model="claude-sonnet-4",
                was_blocked=False,
                block_reason=None,
                prompt_tokens_sent=b.prompt_tokens_sent,
                tool_description_tokens=b.tool_description_tokens,
                context_history_tokens=b.context_history_tokens,
            )

        # Long-horizon track: PydanticAI has NO built-in memory/session
        # persistence. Each agent call starts fresh; the caller is responsible
        # for threading history. In practice this means: full history is passed
        # in the prompt by the application (not the framework). Token growth is
        # therefore the same as naive linear — same as cost-track. Recall
        # depends entirely on whether the application correctly threads history.
        # We model recall as perfect (checkpoints echoed) since a well-behaved
        # app would pass history through, but we note this is NOT the framework
        # providing memory — it's the application.
        if task.kind == "long_horizon":
            b = simulate_cost(
                framework="pydantic-ai",
                prompt=task.prompt,
                turns=max(task.simulated_turns, 1),
                tool_count=task.tool_count,
                context_bytes=task.context_size_bytes,
            )
            checkpoints = task.memory_checkpoints or []
            checkpoint_output = ". ".join(cp.replace("_", " ") for cp in checkpoints)
            return TaskResult(
                task_id=task.id,
                runner="pydantic-ai v0.5 (mock-llm)",
                started_at=started,
                ended_at=datetime.now(timezone.utc),
                output=(
                    f"[pydantic-ai-lh-sim] turns={b.llm_calls} tokens={b.prompt_tokens_sent} "
                    f"memory=none(app-managed) checkpoints: {checkpoint_output}"
                ),
                llm_calls=b.llm_calls,
                input_tokens=b.prompt_tokens_sent,
                output_tokens=b.output_tokens,
                tool_calls=task.min_tool_calls,
                succeeded=True,
                error=None,
                model="claude-sonnet-4",
                was_blocked=False,
                block_reason=None,
                prompt_tokens_sent=b.prompt_tokens_sent,
                tool_description_tokens=b.tool_description_tokens,
                context_history_tokens=b.context_history_tokens,
            )

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
