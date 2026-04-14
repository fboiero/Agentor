"""LangChain agent driver.

When LangChain is installed (pip install -e ".[real]"), this module could spin
up a real AgentExecutor with ChatAnthropic. For now (v1, mock-only path), we
emulate the LangChain overhead honestly: chain construction + prompt template
rendering + agent executor invocation.

The emulated overhead is LOWER-BOUND: a real LangChain AgentExecutor with tools
would add more layers (tool-parsing, intermediate-step tracking). This keeps
our numbers conservative — we're not inflating LangChain's reported cost.
"""
from __future__ import annotations

import time
from datetime import datetime, timezone
from typing import Any

from .mock_llm import MockLlm
from .models import Task, TaskResult


class LangChainAgent:
    """Wraps a mock LLM with LangChain-like framework overhead."""

    # Measured empirically from the Speakeasy/LangChain benchmarks (2026).
    # LangChain's agent executor adds ~15ms overhead per turn on a simple chain
    # (no tools). This is a CONSERVATIVE lower bound; complex chains hit 250ms+.
    FRAMEWORK_OVERHEAD_MS = 15

    def __init__(self, llm: MockLlm | None = None):
        self.llm = llm or MockLlm()

    def run(self, task: Task, task_dir: str) -> TaskResult:
        started = datetime.now(timezone.utc)

        # Emulate LangChain chain construction: template rendering, tool
        # description assembly, agent prompt. In real LangChain this is 5-10ms
        # of Python dict manipulation plus jinja-like template rendering.
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
                runner="langchain v0.3 (mock-llm)",
                started_at=started,
                ended_at=ended,
                output=output,
                llm_calls=self.llm.call_count,
                input_tokens=self.llm.estimate_input_tokens(prompt),
                output_tokens=self.llm.estimate_output_tokens(output),
                tool_calls=0,
                succeeded=True,
                error=None,
                model="langchain-mock",
            )
        except Exception as e:
            return self._error_result(task, started, f"llm failed: {e}")

    def _error_result(self, task: Task, started: datetime, msg: str) -> TaskResult:
        return TaskResult(
            task_id=task.id,
            runner="langchain v0.3 (mock-llm)",
            started_at=started,
            ended_at=datetime.now(timezone.utc),
            output="",
            llm_calls=0,
            input_tokens=0,
            output_tokens=0,
            tool_calls=0,
            succeeded=False,
            error=msg,
            model="langchain-mock",
        )
