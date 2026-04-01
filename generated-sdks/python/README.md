# argentor_client

Python SDK client for the Argentor API.

## Installation

```bash
pip install argentor_client
```

## Quick Start

```python
from argentor_client import ArgentorClient

client = ArgentorClient(
    base_url="http://localhost:3000",
    api_key="your-api-key",
    tenant_id="your-tenant-id",
)

# Run a task
result = client.run_task(
    agent_role="code_reviewer",
    context="Review the following pull request...",
)
print(result)

# Stream results
for token in client.run_task_stream(
    agent_role="assistant",
    context="Explain how Argentor works",
):
    print(token, end="", flush=True)

# Batch execution
batch_result = client.batch(
    tasks=[
        {"agent_role": "analyst", "context": "Analyze Q1 sales data"},
        {"agent_role": "analyst", "context": "Analyze Q2 sales data"},
    ],
    max_concurrent=5,
)

# Evaluate a response
evaluation = client.evaluate(
    response="The code looks clean and follows best practices.",
    context="Code review task",
    criteria=["accuracy", "completeness", "helpfulness"],
)

# Health check
health = client.health()
print(health)
```

## Async Usage

```python
import asyncio
from argentor_client.client import AsyncArgentorClient

async def main():
    async with AsyncArgentorClient(
        base_url="http://localhost:3000",
        api_key="your-api-key",
    ) as client:
        result = await client.run_task(
            agent_role="assistant",
            context="Hello, world!",
        )
        print(result)

asyncio.run(main())
```

## API Reference

### ArgentorClient

| Method | Description |
|--------|-------------|
| `run_task(agent_role, context, **kwargs)` | Execute a single agent task |
| `run_task_stream(agent_role, context, **kwargs)` | Stream task results via SSE |
| `batch(tasks, max_concurrent=5)` | Submit a batch of tasks |
| `evaluate(response, context, criteria)` | Evaluate an agent response |
| `create_persona(tenant_id, agent_role, persona)` | Create a new persona |
| `list_personas(tenant_id)` | List personas for a tenant |
| `get_usage(tenant_id)` | Get usage breakdown |
| `health()` | Check API health |
| `webhook_proxy(event, data, source, secret)` | Forward a webhook event |
