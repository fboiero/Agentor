"""CLI entry point. Invoked by the Rust harness as:
    argentor-claude-agent-sdk-runner --task <json-path> --task-dir <dir>

Writes a TaskResult as JSON to stdout."""
from __future__ import annotations

import json
import sys
from pathlib import Path

import click

from .agent import ClaudeAgentSdkAgent
from .models import Task


@click.command()
@click.option("--task", "task_path", required=True, type=click.Path(exists=True),
              help="Path to the task JSON file")
@click.option("--task-dir", required=True, type=click.Path(exists=True),
              help="Directory containing task input files")
@click.option("--latency-ms", default=50, type=int,
              help="Simulated LLM latency (ms), for apples-to-apples with Argentor")
def main(task_path: str, task_dir: str, latency_ms: int) -> None:
    """Run one Argentor benchmark task through the Claude Agent SDK."""
    try:
        task_data = json.loads(Path(task_path).read_text())
        task = Task.model_validate(task_data)
    except Exception as e:
        sys.stderr.write(f"Failed to parse task: {e}\n")
        sys.exit(1)

    try:
        from .mock_llm import MockLlm
        agent = ClaudeAgentSdkAgent(llm=MockLlm(latency_ms=latency_ms))
        result = agent.run(task, task_dir)
        # Serialize to stdout (Rust harness reads this)
        sys.stdout.write(result.model_dump_json())
    except Exception as e:
        sys.stderr.write(f"Agent run failed: {e}\n")
        sys.exit(1)


if __name__ == "__main__":
    main()
