"""CrewAI agent driver.

When CrewAI is installed (pip install -e ".[real]"), this module could spin up
a real Crew with a single Agent and Task, then call `Crew.kickoff()`. For now
(v1, mock-only path), we emulate CrewAI overhead honestly: Crew.kickoff +
role/goal parsing + task orchestration.

The emulated overhead is a CONSERVATIVE mid-point: a real CrewAI crew with
multiple agents would add substantially more (context hand-off between agents,
delegation parsing, task result aggregation). We only charge single-agent
kickoff cost here — we're not inflating CrewAI's reported cost.
"""
from __future__ import annotations

import time
from datetime import datetime, timezone
from typing import Any

from .cost_sim import simulate as simulate_cost
from .mock_llm import MockLlm
from .models import Task, TaskResult


class CrewAiAgent:
    """Wraps a mock LLM with CrewAI-like framework overhead."""

    # Measured framework overhead for CrewAI agent coordination
    # (Crew.kickoff + role/goal parsing + task orchestration).
    # Per Speakeasy 2026 measurements, CrewAI adds 40-60ms on single-agent
    # crews with no tools. Use 50ms as honest mid-point.
    FRAMEWORK_OVERHEAD_MS = 50

    def __init__(self, llm: MockLlm | None = None):
        self.llm = llm or MockLlm()

    def run(self, task: Task, task_dir: str) -> TaskResult:
        started = datetime.now(timezone.utc)

        # Cost track: short-circuit to the simulator.
        if task.kind == "cost":
            b = simulate_cost(
                framework="crewai",
                prompt=task.prompt,
                turns=max(task.simulated_turns, 1),
                tool_count=task.tool_count,
                context_bytes=task.context_size_bytes,
            )
            return TaskResult(
                task_id=task.id,
                runner="crewai v0.100 (mock-llm)",
                started_at=started,
                ended_at=datetime.now(timezone.utc),
                output=f"[crewai-cost-sim] turns={b.llm_calls}",
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

        # Emulate CrewAI Crew.kickoff(): role/goal string assembly, task
        # orchestration plumbing, and agent executor bootstrap. On a real
        # single-agent crew this consistently lands in the 40-60ms range
        # (Speakeasy 2026 measurements).
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
                runner="crewai v0.100 (mock-llm)",
                started_at=started,
                ended_at=ended,
                output=output,
                llm_calls=self.llm.call_count,
                input_tokens=self.llm.estimate_input_tokens(prompt),
                output_tokens=self.llm.estimate_output_tokens(output),
                tool_calls=0,
                succeeded=True,
                error=None,
                model="crewai-mock",
            )
        except Exception as e:
            return self._error_result(task, started, f"llm failed: {e}")

    def _error_result(self, task: Task, started: datetime, msg: str) -> TaskResult:
        return TaskResult(
            task_id=task.id,
            runner="crewai v0.100 (mock-llm)",
            started_at=started,
            ended_at=datetime.now(timezone.utc),
            output="",
            llm_calls=0,
            input_tokens=0,
            output_tokens=0,
            tool_calls=0,
            succeeded=False,
            error=msg,
            model="crewai-mock",
        )
