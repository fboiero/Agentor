"""Tests for the Pydantic AI runner agent."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest

from argentor_pydantic_ai_runner.agent import PydanticAiAgent
from argentor_pydantic_ai_runner.mock_llm import MockLlm
from argentor_pydantic_ai_runner.models import Rubric, RubricCriterion, Task


def sample_task(**overrides) -> Task:
    defaults = {
        "id": "t_test",
        "name": "Test Task",
        "description": "",
        "kind": "qa",
        "prompt": "What is 2+2?",
        "input": "",
        "ground_truth": "4",
        "rubric": Rubric(
            criteria=[RubricCriterion(name="correct", description="x", weight=1.0)],
            pass_threshold=5.0,
        ),
        "max_turns": 1,
        "allowed_tools": [],
    }
    defaults.update(overrides)
    return Task(**defaults)


def test_mock_llm_echoes_with_latency():
    llm = MockLlm(latency_ms=5)
    response = llm.invoke("hello world")
    assert "hello world" in response
    assert "[pydantic-ai-mock]" in response
    assert llm.call_count == 1


def test_agent_runs_successfully():
    agent = PydanticAiAgent(llm=MockLlm(latency_ms=1))
    task = sample_task()
    with tempfile.TemporaryDirectory() as td:
        result = agent.run(task, td)
    assert result.succeeded
    assert result.task_id == "t_test"
    assert result.llm_calls == 1


def test_agent_reports_tokens():
    agent = PydanticAiAgent(llm=MockLlm(latency_ms=1))
    task = sample_task(prompt="a longer prompt that should produce more tokens")
    with tempfile.TemporaryDirectory() as td:
        result = agent.run(task, td)
    assert result.input_tokens > 0
    assert result.output_tokens > 0


def test_agent_handles_file_input():
    with tempfile.TemporaryDirectory() as td:
        sample_path = Path(td, "doc.txt")
        sample_path.write_text("the content of the document")
        task = sample_task(
            prompt="Summarize: {{input}}",
            input={"file": "doc.txt"},
        )
        agent = PydanticAiAgent(llm=MockLlm(latency_ms=1))
        result = agent.run(task, td)
    assert result.succeeded
    # Prompt substitution should have happened → output contains the mock marker
    assert "[pydantic-ai-mock]" in result.output


def test_agent_handles_missing_file():
    with tempfile.TemporaryDirectory() as td:
        task = sample_task(
            prompt="Summarize: {{input}}",
            input={"file": "nonexistent.txt"},
        )
        agent = PydanticAiAgent(llm=MockLlm(latency_ms=1))
        result = agent.run(task, td)
    assert not result.succeeded
    assert "input load failed" in (result.error or "")


def test_framework_overhead_applied():
    """Agent should take at LEAST framework_overhead + llm_latency."""
    agent = PydanticAiAgent(llm=MockLlm(latency_ms=10))
    task = sample_task()
    with tempfile.TemporaryDirectory() as td:
        result = agent.run(task, td)
    delta_ms = (result.ended_at - result.started_at).total_seconds() * 1000
    # FRAMEWORK_OVERHEAD (8) + mock latency (10) = ~18ms minimum.
    # Use 15ms threshold to account for scheduler jitter on slow CI.
    assert delta_ms >= 15, f"expected >= 15ms, got {delta_ms:.1f}"


def test_result_serializes_to_json():
    agent = PydanticAiAgent(llm=MockLlm(latency_ms=1))
    task = sample_task()
    with tempfile.TemporaryDirectory() as td:
        result = agent.run(task, td)
    js = result.model_dump_json()
    back = json.loads(js)
    assert back["task_id"] == "t_test"
    assert back["succeeded"] is True
    assert "started_at" in back
    assert "ended_at" in back


def test_task_input_inline_string():
    task = sample_task(input="inline value")
    assert task.load_input("/tmp") == "inline value"


def test_task_input_empty_string_default():
    task = sample_task(input="")
    assert task.load_input("/tmp") == ""


def test_runner_name_identifies_pydantic_ai():
    """Runner identification must include 'pydantic-ai' so the Rust harness and
    reports can distinguish it from other external runners."""
    agent = PydanticAiAgent(llm=MockLlm(latency_ms=1))
    task = sample_task()
    with tempfile.TemporaryDirectory() as td:
        result = agent.run(task, td)
    assert "pydantic-ai" in result.runner
    assert result.model == "pydantic-ai-mock"


def test_framework_overhead_is_lowest_competitor():
    """Honesty check: Pydantic AI's documented overhead should be <= 10ms.
    If we ever raise this, the constant must reflect published measurements."""
    assert PydanticAiAgent.FRAMEWORK_OVERHEAD_MS <= 10
    assert PydanticAiAgent.FRAMEWORK_OVERHEAD_MS >= 5
