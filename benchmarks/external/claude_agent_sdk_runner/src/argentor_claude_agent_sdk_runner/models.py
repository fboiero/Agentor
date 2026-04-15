"""Task and TaskResult Pydantic models — mirror the Rust crate's shapes."""
from __future__ import annotations

from datetime import datetime
from typing import Any, Optional, Union

from pydantic import BaseModel, Field


class RubricCriterion(BaseModel):
    name: str
    description: str
    weight: float = 1.0


class Rubric(BaseModel):
    criteria: list[RubricCriterion]
    pass_threshold: float = 6.0


class TaskInputFile(BaseModel):
    file: str


class Task(BaseModel):
    """Mirrors the Rust `Task` struct from `benchmarks/src/task.rs`."""

    id: str
    name: str
    description: str = ""
    kind: str = "qa"
    prompt: str
    # input is either a string (inline) or {"file": "..."}
    input: Union[str, TaskInputFile] = ""
    ground_truth: Optional[str] = None
    rubric: Rubric
    max_turns: int = 10
    allowed_tools: list[str] = Field(default_factory=list)
    # Security benchmark label: True = adversarial (should be blocked),
    # False = legitimate control (must NOT be blocked), None = non-security task.
    expected_blocked: Optional[bool] = None
    # Cost benchmark inputs (Phase 2b)
    simulated_turns: int = 1
    tool_count: int = 0
    context_size_bytes: int = 0

    def load_input(self, task_dir: str) -> str:
        """Resolve `input`: return inline string or read the referenced file."""
        if isinstance(self.input, str):
            return self.input
        from pathlib import Path
        return Path(task_dir, self.input.file).read_text()


class TaskResult(BaseModel):
    """Mirrors the Rust `TaskResult` struct. Must serialize to the same shape."""

    task_id: str
    runner: str
    started_at: datetime
    ended_at: datetime
    output: str
    llm_calls: int
    input_tokens: int
    output_tokens: int
    tool_calls: int
    succeeded: bool
    error: Optional[str] = None
    model: str
    # Security benchmark fields. The Claude Agent SDK relies on Anthropic's
    # server-side safety policies and does NOT ship client-side input
    # guardrails. This runner always reports was_blocked=False,
    # block_reason=None to reflect the default out-of-the-box posture.
    was_blocked: bool = False
    block_reason: Optional[str] = None
    # Cost benchmark outputs (Phase 2b)
    prompt_tokens_sent: int = 0
    tool_description_tokens: int = 0
    context_history_tokens: int = 0

    model_config = {
        "json_schema_extra": {
            "description": "Canonical benchmark result shape shared with Rust harness",
        }
    }
