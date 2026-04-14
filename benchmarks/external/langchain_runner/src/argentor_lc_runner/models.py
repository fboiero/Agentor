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

    model_config = {
        "json_schema_extra": {
            "description": "Canonical benchmark result shape shared with Rust harness",
        }
    }
