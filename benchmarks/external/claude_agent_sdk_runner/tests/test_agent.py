"""Tests for the Claude Agent SDK runner agent."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest

from argentor_claude_agent_sdk_runner.agent import ClaudeAgentSdkAgent
from argentor_claude_agent_sdk_runner.mock_llm import MockLlm
from argentor_claude_agent_sdk_runner.models import Rubric, RubricCriterion, Task


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
    assert "[claude-agent-sdk-mock]" in response
    assert llm.call_count == 1


def test_agent_runs_successfully():
    agent = ClaudeAgentSdkAgent(llm=MockLlm(latency_ms=1))
    task = sample_task()
    with tempfile.TemporaryDirectory() as td:
        result = agent.run(task, td)
    assert result.succeeded
    assert result.task_id == "t_test"
    assert result.llm_calls == 1


def test_agent_reports_tokens():
    agent = ClaudeAgentSdkAgent(llm=MockLlm(latency_ms=1))
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
        agent = ClaudeAgentSdkAgent(llm=MockLlm(latency_ms=1))
        result = agent.run(task, td)
    assert result.succeeded
    # Prompt substitution should have happened → output contains the mock prefix
    assert "[claude-agent-sdk-mock]" in result.output


def test_agent_handles_missing_file():
    with tempfile.TemporaryDirectory() as td:
        task = sample_task(
            prompt="Summarize: {{input}}",
            input={"file": "nonexistent.txt"},
        )
        agent = ClaudeAgentSdkAgent(llm=MockLlm(latency_ms=1))
        result = agent.run(task, td)
    assert not result.succeeded
    assert "input load failed" in (result.error or "")


def test_framework_overhead_applied():
    """Agent should take at LEAST framework_overhead + llm_latency."""
    agent = ClaudeAgentSdkAgent(llm=MockLlm(latency_ms=10))
    task = sample_task()
    with tempfile.TemporaryDirectory() as td:
        result = agent.run(task, td)
    delta_ms = (result.ended_at - result.started_at).total_seconds() * 1000
    # FRAMEWORK_OVERHEAD (12) + mock latency (10) = ~22ms minimum
    # Allow slack for scheduler jitter; assert a conservative lower bound.
    assert delta_ms >= 18, f"expected >= 18ms, got {delta_ms:.1f}"


def test_framework_overhead_is_honest_low_number():
    """Claude Agent SDK is Anthropic's own lean SDK. Overhead must be low (<=20ms).

    This protects against accidentally inflating the number — Anthropic's published
    measurements put it at ~12ms, and we must stay close to that honestly.
    """
    assert ClaudeAgentSdkAgent.FRAMEWORK_OVERHEAD_MS <= 20, (
        "Claude Agent SDK overhead should be honest and low; "
        f"got {ClaudeAgentSdkAgent.FRAMEWORK_OVERHEAD_MS}ms"
    )


def test_result_serializes_to_json():
    agent = ClaudeAgentSdkAgent(llm=MockLlm(latency_ms=1))
    task = sample_task()
    with tempfile.TemporaryDirectory() as td:
        result = agent.run(task, td)
    js = result.model_dump_json()
    back = json.loads(js)
    assert back["task_id"] == "t_test"
    assert back["succeeded"] is True
    assert "started_at" in back
    assert "ended_at" in back


def test_runner_name_is_claude_agent_sdk():
    agent = ClaudeAgentSdkAgent(llm=MockLlm(latency_ms=1))
    task = sample_task()
    with tempfile.TemporaryDirectory() as td:
        result = agent.run(task, td)
    assert result.runner == "claude-agent-sdk v0.2 (mock-llm)"
    assert result.model == "claude-agent-sdk-mock"


def test_task_input_inline_string():
    task = sample_task(input="inline value")
    assert task.load_input("/tmp") == "inline value"


def test_task_input_empty_string_default():
    task = sample_task(input="")
    assert task.load_input("/tmp") == ""
