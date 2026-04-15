"""CLI integration tests — exercises the `argentor-crewai-runner` entry point."""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest


def _make_task_json(tmp: Path) -> Path:
    task = {
        "id": "t_cli_test",
        "name": "CLI Test",
        "description": "Exercise the CLI path",
        "kind": "qa",
        "prompt": "What is 2+2?",
        "input": "",
        "ground_truth": "4",
        "rubric": {
            "criteria": [{"name": "correct", "description": "x", "weight": 1.0}],
            "pass_threshold": 5.0,
        },
        "max_turns": 1,
        "allowed_tools": [],
    }
    path = tmp / "task.json"
    path.write_text(json.dumps(task))
    return path


def test_cli_outputs_valid_task_result():
    with tempfile.TemporaryDirectory() as td:
        task_path = _make_task_json(Path(td))
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "argentor_crewai_runner.main",
                "--task", str(task_path),
                "--task-dir", td,
                "--latency-ms", "1",
            ],
            capture_output=True,
            text=True,
        )

    assert result.returncode == 0, f"stderr: {result.stderr}"
    parsed = json.loads(result.stdout)
    assert parsed["task_id"] == "t_cli_test"
    assert parsed["succeeded"] is True
    assert parsed["runner"].startswith("crewai")
    assert parsed["model"] == "crewai-mock"


def test_cli_errors_on_missing_task_file():
    with tempfile.TemporaryDirectory() as td:
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "argentor_crewai_runner.main",
                "--task", "/nonexistent/task.json",
                "--task-dir", td,
            ],
            capture_output=True,
            text=True,
        )
    assert result.returncode != 0
