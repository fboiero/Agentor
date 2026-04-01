//! SDK client code generator for the Argentor API.
//!
//! Generates complete Python and TypeScript SDK client packages from the
//! Argentor API definition. Each generated SDK includes a client class,
//! typed models, SSE streaming helpers, package metadata, and documentation.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// Configuration for SDK generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkConfig {
    /// Base URL of the Argentor API server.
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Package name for the generated SDK.
    #[serde(default = "default_package_name")]
    pub package_name: String,
    /// Semantic version of the generated SDK.
    #[serde(default = "default_version")]
    pub version: String,
    /// Whether to include async client methods.
    #[serde(default = "default_true")]
    pub include_async: bool,
    /// Whether to include SSE streaming helpers.
    #[serde(default = "default_true")]
    pub include_streaming: bool,
}

fn default_base_url() -> String {
    "http://localhost:3000".to_string()
}
fn default_package_name() -> String {
    "argentor_client".to_string()
}
fn default_version() -> String {
    "0.1.0".to_string()
}
fn default_true() -> bool {
    true
}

impl Default for SdkConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            package_name: default_package_name(),
            version: default_version(),
            include_async: true,
            include_streaming: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// A single generated file with its relative path and content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFile {
    /// Relative path inside the SDK output directory.
    pub path: String,
    /// File content.
    pub content: String,
}

/// Output of SDK generation for one language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkOutput {
    /// Target language identifier (`"python"` or `"typescript"`).
    pub language: String,
    /// Generated files for this SDK.
    pub files: Vec<GeneratedFile>,
}

// ---------------------------------------------------------------------------
// SdkGenerator
// ---------------------------------------------------------------------------

/// Generates Python and TypeScript SDK client packages for the Argentor API.
pub struct SdkGenerator;

impl SdkGenerator {
    /// Create a new `SdkGenerator`.
    pub fn new() -> Self {
        Self
    }

    /// Generate a complete Python SDK package.
    pub fn generate_python(&self, config: &SdkConfig) -> SdkOutput {
        let pkg = &config.package_name;
        let mut files = vec![
            GeneratedFile {
                path: format!("{pkg}/__init__.py"),
                content: generate_python_init(config),
            },
            GeneratedFile {
                path: format!("{pkg}/client.py"),
                content: generate_python_client(config),
            },
            GeneratedFile {
                path: format!("{pkg}/models.py"),
                content: generate_python_models(config),
            },
            GeneratedFile {
                path: "setup.py".to_string(),
                content: generate_python_setup(config),
            },
            GeneratedFile {
                path: "README.md".to_string(),
                content: generate_python_readme(config),
            },
        ];

        if config.include_streaming {
            files.push(GeneratedFile {
                path: format!("{pkg}/streaming.py"),
                content: generate_python_streaming(config),
            });
        }

        SdkOutput {
            language: "python".to_string(),
            files,
        }
    }

    /// Generate a complete TypeScript SDK package.
    pub fn generate_typescript(&self, config: &SdkConfig) -> SdkOutput {
        let mut files = vec![
            GeneratedFile {
                path: "src/index.ts".to_string(),
                content: generate_ts_index(config),
            },
            GeneratedFile {
                path: "src/types.ts".to_string(),
                content: generate_ts_types(config),
            },
            GeneratedFile {
                path: "package.json".to_string(),
                content: generate_ts_package_json(config),
            },
            GeneratedFile {
                path: "tsconfig.json".to_string(),
                content: generate_ts_tsconfig(),
            },
            GeneratedFile {
                path: "README.md".to_string(),
                content: generate_ts_readme(config),
            },
        ];

        if config.include_streaming {
            files.push(GeneratedFile {
                path: "src/streaming.ts".to_string(),
                content: generate_ts_streaming(config),
            });
        }

        SdkOutput {
            language: "typescript".to_string(),
            files,
        }
    }

    /// Generate SDKs for all supported languages.
    pub fn generate_all(&self, config: &SdkConfig) -> Vec<SdkOutput> {
        vec![
            self.generate_python(config),
            self.generate_typescript(config),
        ]
    }
}

impl Default for SdkGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Python generators
// ===========================================================================

fn generate_python_init(config: &SdkConfig) -> String {
    let streaming_import = if config.include_streaming {
        format!(
            "from {pkg}.streaming import SSEStream\n",
            pkg = config.package_name
        )
    } else {
        String::new()
    };

    format!(
        r#""""{name} — Python client for the Argentor API."""

from {pkg}.client import ArgentorClient
from {pkg}.models import (
    RunTaskRequest,
    RunTaskResponse,
    BatchRequest,
    BatchResponse,
    EvaluateRequest,
    EvaluateResponse,
    CreatePersonaRequest,
    CreatePersonaResponse,
    ListPersonasResponse,
    UsageBreakdown,
    HealthResponse,
    WebhookProxyRequest,
    WebhookProxyResponse,
)
{streaming_import}
__version__ = "{version}"

__all__ = [
    "ArgentorClient",
    "RunTaskRequest",
    "RunTaskResponse",
    "BatchRequest",
    "BatchResponse",
    "EvaluateRequest",
    "EvaluateResponse",
    "CreatePersonaRequest",
    "CreatePersonaResponse",
    "ListPersonasResponse",
    "UsageBreakdown",
    "HealthResponse",
    "WebhookProxyRequest",
    "WebhookProxyResponse",
]
"#,
        name = config.package_name,
        pkg = config.package_name,
        version = config.version,
    )
}

fn generate_python_client(config: &SdkConfig) -> String {
    let base_url = &config.base_url;

    let async_methods = if config.include_async {
        format!(
            r#"

class AsyncArgentorClient:
    """Async client for the Argentor API."""

    def __init__(
        self,
        base_url: str = "{base_url}",
        api_key: str = "",
        tenant_id: str = "",
        timeout: float = 60.0,
    ):
        self.base_url = base_url
        self.headers = {{"X-API-Key": api_key, "X-Tenant-ID": tenant_id}}
        self._client = httpx.AsyncClient(
            base_url=base_url, headers=self.headers, timeout=timeout
        )

    async def close(self) -> None:
        """Close the underlying HTTP client."""
        await self._client.aclose()

    async def __aenter__(self) -> "AsyncArgentorClient":
        return self

    async def __aexit__(self, *exc) -> None:
        await self.close()

    async def run_task(
        self,
        agent_role: str,
        context: str,
        *,
        model: Optional[str] = None,
        max_tokens: Optional[int] = None,
        tools: Optional[List[str]] = None,
    ) -> dict:
        """Execute a single agent task."""
        payload: Dict[str, Any] = {{"agent_role": agent_role, "context": context}}
        if model is not None:
            payload["model"] = model
        if max_tokens is not None:
            payload["max_tokens"] = max_tokens
        if tools is not None:
            payload["tools"] = tools
        resp = await self._client.post("/v1/run", json=payload)
        resp.raise_for_status()
        return resp.json()

    async def run_task_stream(
        self,
        agent_role: str,
        context: str,
        *,
        model: Optional[str] = None,
        max_tokens: Optional[int] = None,
        tools: Optional[List[str]] = None,
    ) -> AsyncIterator[dict]:
        """Stream task results via SSE."""
        payload: Dict[str, Any] = {{"agent_role": agent_role, "context": context}}
        if model is not None:
            payload["model"] = model
        if max_tokens is not None:
            payload["max_tokens"] = max_tokens
        if tools is not None:
            payload["tools"] = tools
        async with self._client.stream("POST", "/v1/run/stream", json=payload) as resp:
            resp.raise_for_status()
            async for line in resp.aiter_lines():
                if line.startswith("data: "):
                    data = line[len("data: "):]
                    if data.strip() == "[DONE]":
                        break
                    yield json.loads(data)

    async def batch(
        self,
        tasks: List[Dict[str, Any]],
        *,
        max_concurrent: int = 5,
    ) -> dict:
        """Submit a batch of tasks for parallel execution."""
        payload = {{"tasks": tasks, "max_concurrent": max_concurrent}}
        resp = await self._client.post("/v1/batch", json=payload)
        resp.raise_for_status()
        return resp.json()

    async def evaluate(
        self,
        response: str,
        context: str,
        criteria: Optional[List[str]] = None,
    ) -> dict:
        """Evaluate an agent response against criteria."""
        payload: Dict[str, Any] = {{"response": response, "context": context}}
        if criteria is not None:
            payload["criteria"] = criteria
        resp = await self._client.post("/v1/evaluate", json=payload)
        resp.raise_for_status()
        return resp.json()

    async def create_persona(
        self,
        tenant_id: str,
        agent_role: str,
        persona: Dict[str, Any],
    ) -> dict:
        """Create a new agent persona for a tenant."""
        payload = {{"tenant_id": tenant_id, "agent_role": agent_role, "persona": persona}}
        resp = await self._client.post("/v1/personas", json=payload)
        resp.raise_for_status()
        return resp.json()

    async def list_personas(self, tenant_id: str) -> dict:
        """List all personas for a tenant."""
        resp = await self._client.get("/v1/personas", params={{"tenant_id": tenant_id}})
        resp.raise_for_status()
        return resp.json()

    async def get_usage(self, tenant_id: str) -> dict:
        """Get usage breakdown for a tenant."""
        resp = await self._client.get(f"/v1/usage/{{tenant_id}}")
        resp.raise_for_status()
        return resp.json()

    async def health(self) -> dict:
        """Check API server health."""
        resp = await self._client.get("/health")
        resp.raise_for_status()
        return resp.json()

    async def webhook_proxy(
        self,
        event: str,
        data: Dict[str, Any],
        *,
        source: str = "",
        secret: str = "",
    ) -> dict:
        """Forward a webhook event through the proxy."""
        payload: Dict[str, Any] = {{"event": event, "data": data}}
        if source:
            payload["source"] = source
        if secret:
            payload["secret"] = secret
        resp = await self._client.post("/v1/webhooks/proxy", json=payload)
        resp.raise_for_status()
        return resp.json()
"#
        )
    } else {
        String::new()
    };

    let streaming_import = if config.include_streaming {
        "import json\n"
    } else {
        ""
    };

    let stream_method = if config.include_streaming {
        r#"
    def run_task_stream(
        self,
        agent_role: str,
        context: str,
        *,
        model: Optional[str] = None,
        max_tokens: Optional[int] = None,
        tools: Optional[List[str]] = None,
    ) -> Iterator[dict]:
        """Stream task results via SSE (synchronous)."""
        payload: Dict[str, Any] = {"agent_role": agent_role, "context": context}
        if model is not None:
            payload["model"] = model
        if max_tokens is not None:
            payload["max_tokens"] = max_tokens
        if tools is not None:
            payload["tools"] = tools
        with self._client.stream("POST", "/v1/run/stream", json=payload) as resp:
            resp.raise_for_status()
            for line in resp.iter_lines():
                if line.startswith("data: "):
                    data = line[len("data: "):]
                    if data.strip() == "[DONE]":
                        break
                    yield json.loads(data)
"#
    } else {
        ""
    };

    format!(
        r#""""{name} — Python client for the Argentor API."""

import httpx
{streaming_import}from typing import Optional, Dict, List, Any, Iterator, AsyncIterator


class ArgentorClient:
    """Synchronous client for the Argentor API."""

    def __init__(
        self,
        base_url: str = "{base_url}",
        api_key: str = "",
        tenant_id: str = "",
        timeout: float = 60.0,
    ):
        self.base_url = base_url
        self.headers = {{"X-API-Key": api_key, "X-Tenant-ID": tenant_id}}
        self._client = httpx.Client(
            base_url=base_url, headers=self.headers, timeout=timeout
        )

    def close(self) -> None:
        """Close the underlying HTTP client."""
        self._client.close()

    def __enter__(self) -> "ArgentorClient":
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    def run_task(
        self,
        agent_role: str,
        context: str,
        *,
        model: Optional[str] = None,
        max_tokens: Optional[int] = None,
        tools: Optional[List[str]] = None,
    ) -> dict:
        """Execute a single agent task."""
        payload: Dict[str, Any] = {{"agent_role": agent_role, "context": context}}
        if model is not None:
            payload["model"] = model
        if max_tokens is not None:
            payload["max_tokens"] = max_tokens
        if tools is not None:
            payload["tools"] = tools
        resp = self._client.post("/v1/run", json=payload)
        resp.raise_for_status()
        return resp.json()
{stream_method}
    def batch(
        self,
        tasks: List[Dict[str, Any]],
        *,
        max_concurrent: int = 5,
    ) -> dict:
        """Submit a batch of tasks for parallel execution."""
        payload = {{"tasks": tasks, "max_concurrent": max_concurrent}}
        resp = self._client.post("/v1/batch", json=payload)
        resp.raise_for_status()
        return resp.json()

    def evaluate(
        self,
        response: str,
        context: str,
        criteria: Optional[List[str]] = None,
    ) -> dict:
        """Evaluate an agent response against criteria."""
        payload: Dict[str, Any] = {{"response": response, "context": context}}
        if criteria is not None:
            payload["criteria"] = criteria
        resp = self._client.post("/v1/evaluate", json=payload)
        resp.raise_for_status()
        return resp.json()

    def create_persona(
        self,
        tenant_id: str,
        agent_role: str,
        persona: Dict[str, Any],
    ) -> dict:
        """Create a new agent persona for a tenant."""
        payload = {{"tenant_id": tenant_id, "agent_role": agent_role, "persona": persona}}
        resp = self._client.post("/v1/personas", json=payload)
        resp.raise_for_status()
        return resp.json()

    def list_personas(self, tenant_id: str) -> dict:
        """List all personas for a tenant."""
        resp = self._client.get("/v1/personas", params={{"tenant_id": tenant_id}})
        resp.raise_for_status()
        return resp.json()

    def get_usage(self, tenant_id: str) -> dict:
        """Get usage breakdown for a tenant."""
        resp = self._client.get(f"/v1/usage/{{tenant_id}}")
        resp.raise_for_status()
        return resp.json()

    def health(self) -> dict:
        """Check API server health."""
        resp = self._client.get("/health")
        resp.raise_for_status()
        return resp.json()

    def webhook_proxy(
        self,
        event: str,
        data: Dict[str, Any],
        *,
        source: str = "",
        secret: str = "",
    ) -> dict:
        """Forward a webhook event through the proxy."""
        payload: Dict[str, Any] = {{"event": event, "data": data}}
        if source:
            payload["source"] = source
        if secret:
            payload["secret"] = secret
        resp = self._client.post("/v1/webhooks/proxy", json=payload)
        resp.raise_for_status()
        return resp.json()
{async_methods}"#,
        name = config.package_name,
        base_url = base_url,
    )
}

fn generate_python_models(_config: &SdkConfig) -> String {
    r#""""Pydantic models for Argentor API request/response types."""

from typing import Optional, List, Dict, Any
from pydantic import BaseModel, Field


# ---------------------------------------------------------------------------
# Run Task
# ---------------------------------------------------------------------------

class RunTaskRequest(BaseModel):
    """Request body for POST /v1/run."""
    agent_role: str
    context: str
    model: Optional[str] = None
    max_tokens: Optional[int] = None
    tools: Optional[List[str]] = None


class RunTaskResponse(BaseModel):
    """Response from POST /v1/run."""
    task_id: str
    status: str
    result: Optional[str] = None
    tokens_used: Optional[int] = None
    duration_ms: Optional[int] = None
    metadata: Optional[Dict[str, Any]] = None


# ---------------------------------------------------------------------------
# Batch
# ---------------------------------------------------------------------------

class BatchTask(BaseModel):
    """A single task within a batch request."""
    agent_role: str
    context: str
    model: Optional[str] = None
    max_tokens: Optional[int] = None


class BatchRequest(BaseModel):
    """Request body for POST /v1/batch."""
    tasks: List[BatchTask]
    max_concurrent: int = Field(default=5, ge=1, le=50)


class BatchTaskResult(BaseModel):
    """Result for one task in a batch."""
    task_id: str
    status: str
    result: Optional[str] = None
    error: Optional[str] = None


class BatchResponse(BaseModel):
    """Response from POST /v1/batch."""
    batch_id: str
    results: List[BatchTaskResult]
    total: int
    succeeded: int
    failed: int


# ---------------------------------------------------------------------------
# Evaluate
# ---------------------------------------------------------------------------

class EvaluateRequest(BaseModel):
    """Request body for POST /v1/evaluate."""
    response: str
    context: str
    criteria: Optional[List[str]] = None


class CriterionScore(BaseModel):
    """Score for a single evaluation criterion."""
    criterion: str
    score: float = Field(ge=0.0, le=1.0)
    explanation: Optional[str] = None


class EvaluateResponse(BaseModel):
    """Response from POST /v1/evaluate."""
    overall_score: float = Field(ge=0.0, le=1.0)
    scores: List[CriterionScore]
    summary: Optional[str] = None


# ---------------------------------------------------------------------------
# Personas
# ---------------------------------------------------------------------------

class PersonaConfig(BaseModel):
    """Configuration for an agent persona."""
    name: str
    system_prompt: Optional[str] = None
    temperature: Optional[float] = None
    model: Optional[str] = None
    tools: Optional[List[str]] = None
    metadata: Optional[Dict[str, Any]] = None


class CreatePersonaRequest(BaseModel):
    """Request body for POST /v1/personas."""
    tenant_id: str
    agent_role: str
    persona: PersonaConfig


class CreatePersonaResponse(BaseModel):
    """Response from POST /v1/personas."""
    persona_id: str
    tenant_id: str
    agent_role: str
    created_at: str


class PersonaSummary(BaseModel):
    """Summary of a persona in list responses."""
    persona_id: str
    agent_role: str
    name: str
    created_at: str


class ListPersonasResponse(BaseModel):
    """Response from GET /v1/personas."""
    tenant_id: str
    personas: List[PersonaSummary]


# ---------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------

class ModelUsage(BaseModel):
    """Token usage for a single model."""
    model: str
    input_tokens: int
    output_tokens: int
    total_tokens: int
    cost_usd: Optional[float] = None


class UsageBreakdown(BaseModel):
    """Response from GET /v1/usage/{tenant_id}."""
    tenant_id: str
    period_start: str
    period_end: str
    models: List[ModelUsage]
    total_tokens: int
    total_cost_usd: Optional[float] = None


# ---------------------------------------------------------------------------
# Health
# ---------------------------------------------------------------------------

class HealthResponse(BaseModel):
    """Response from GET /health."""
    status: str
    version: str
    uptime_seconds: Optional[float] = None


# ---------------------------------------------------------------------------
# Webhook Proxy
# ---------------------------------------------------------------------------

class WebhookProxyRequest(BaseModel):
    """Request body for POST /v1/webhooks/proxy."""
    event: str
    data: Dict[str, Any]
    source: Optional[str] = None
    secret: Optional[str] = None


class WebhookProxyResponse(BaseModel):
    """Response from POST /v1/webhooks/proxy."""
    accepted: bool
    event_id: Optional[str] = None
    message: Optional[str] = None
"#
    .to_string()
}

fn generate_python_streaming(config: &SdkConfig) -> String {
    format!(
        r#""""{name} — SSE streaming helpers."""

import json
from typing import Iterator, AsyncIterator, Any


class SSEStream:
    """Parses a Server-Sent Events stream from raw lines."""

    def __init__(self, lines: Iterator[str]):
        self._lines = lines

    def __iter__(self) -> Iterator[dict]:
        for line in self._lines:
            if line.startswith("data: "):
                data = line[len("data: "):]
                stripped = data.strip()
                if stripped == "[DONE]":
                    return
                yield json.loads(stripped)


class AsyncSSEStream:
    """Parses a Server-Sent Events stream from async lines."""

    def __init__(self, lines: AsyncIterator[str]):
        self._lines = lines

    async def __aiter__(self) -> AsyncIterator[dict]:
        async for line in self._lines:
            if line.startswith("data: "):
                data = line[len("data: "):]
                stripped = data.strip()
                if stripped == "[DONE]":
                    return
                yield json.loads(stripped)
"#,
        name = config.package_name,
    )
}

fn generate_python_setup(config: &SdkConfig) -> String {
    format!(
        r#"from setuptools import setup, find_packages

setup(
    name="{name}",
    version="{version}",
    description="Python SDK client for the Argentor API",
    long_description=open("README.md").read(),
    long_description_content_type="text/markdown",
    packages=find_packages(),
    python_requires=">=3.9",
    install_requires=[
        "httpx>=0.25.0",
        "pydantic>=2.0.0",
    ],
    extras_require={{
        "dev": [
            "pytest>=7.0",
            "pytest-asyncio>=0.21",
            "respx>=0.20",
        ],
    }},
    classifiers=[
        "Development Status :: 3 - Alpha",
        "Intended Audience :: Developers",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
        "Programming Language :: Python :: 3.12",
    ],
)
"#,
        name = config.package_name,
        version = config.version,
    )
}

fn generate_python_readme(config: &SdkConfig) -> String {
    format!(
        r#"# {name}

Python SDK client for the Argentor API.

## Installation

```bash
pip install {name}
```

## Quick Start

```python
from {name} import ArgentorClient

client = ArgentorClient(
    base_url="{base_url}",
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
        {{"agent_role": "analyst", "context": "Analyze Q1 sales data"}},
        {{"agent_role": "analyst", "context": "Analyze Q2 sales data"}},
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
from {name}.client import AsyncArgentorClient

async def main():
    async with AsyncArgentorClient(
        base_url="{base_url}",
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
"#,
        name = config.package_name,
        base_url = config.base_url,
    )
}

// ===========================================================================
// TypeScript generators
// ===========================================================================

fn generate_ts_index(config: &SdkConfig) -> String {
    let base_url = &config.base_url;

    let stream_import = if config.include_streaming {
        "import { parseSSEStream } from './streaming';\n"
    } else {
        ""
    };

    let stream_method = if config.include_streaming {
        r#"
  /**
   * Stream task results via SSE.
   */
  async *runTaskStream(
    agentRole: string,
    context: string,
    options?: { model?: string; maxTokens?: number; tools?: string[] },
  ): AsyncGenerator<Record<string, unknown>> {
    const payload: Record<string, unknown> = { agent_role: agentRole, context };
    if (options?.model) payload.model = options.model;
    if (options?.maxTokens) payload.max_tokens = options.maxTokens;
    if (options?.tools) payload.tools = options.tools;

    const resp = await fetch(`${this.baseUrl}/v1/run/stream`, {
      method: 'POST',
      headers: { ...this.headers, 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });

    if (!resp.ok) {
      throw new Error(`HTTP ${resp.status}: ${resp.statusText}`);
    }

    yield* parseSSEStream(resp);
  }
"#
    } else {
        ""
    };

    let async_qualifier = if config.include_async {
        ""
    } else {
        "// Note: async methods omitted per configuration\n"
    };

    format!(
        r#"/**
 * {name} — TypeScript client for the Argentor API.
 */

import type {{
  RunTaskResponse,
  BatchResponse,
  EvaluateResponse,
  CreatePersonaResponse,
  ListPersonasResponse,
  UsageBreakdown,
  HealthResponse,
  WebhookProxyResponse,
}} from './types';
{stream_import}{async_qualifier}
export class ArgentorClient {{
  private readonly baseUrl: string;
  private readonly headers: Record<string, string>;

  constructor(options?: {{
    baseUrl?: string;
    apiKey?: string;
    tenantId?: string;
  }}) {{
    this.baseUrl = options?.baseUrl ?? '{base_url}';
    this.headers = {{
      'Content-Type': 'application/json',
      'X-API-Key': options?.apiKey ?? '',
      'X-Tenant-ID': options?.tenantId ?? '',
    }};
  }}

  /**
   * Execute a single agent task.
   */
  async runTask(
    agentRole: string,
    context: string,
    options?: {{ model?: string; maxTokens?: number; tools?: string[] }},
  ): Promise<RunTaskResponse> {{
    const payload: Record<string, unknown> = {{ agent_role: agentRole, context }};
    if (options?.model) payload.model = options.model;
    if (options?.maxTokens) payload.max_tokens = options.maxTokens;
    if (options?.tools) payload.tools = options.tools;

    const resp = await fetch(`${{this.baseUrl}}/v1/run`, {{
      method: 'POST',
      headers: this.headers,
      body: JSON.stringify(payload),
    }});
    if (!resp.ok) throw new Error(`HTTP ${{resp.status}}: ${{resp.statusText}}`);
    return resp.json() as Promise<RunTaskResponse>;
  }}
{stream_method}
  /**
   * Submit a batch of tasks for parallel execution.
   */
  async batch(
    tasks: Array<{{ agentRole: string; context: string; model?: string; maxTokens?: number }}>,
    options?: {{ maxConcurrent?: number }},
  ): Promise<BatchResponse> {{
    const payload = {{
      tasks: tasks.map((t) => ({{
        agent_role: t.agentRole,
        context: t.context,
        model: t.model,
        max_tokens: t.maxTokens,
      }})),
      max_concurrent: options?.maxConcurrent ?? 5,
    }};
    const resp = await fetch(`${{this.baseUrl}}/v1/batch`, {{
      method: 'POST',
      headers: this.headers,
      body: JSON.stringify(payload),
    }});
    if (!resp.ok) throw new Error(`HTTP ${{resp.status}}: ${{resp.statusText}}`);
    return resp.json() as Promise<BatchResponse>;
  }}

  /**
   * Evaluate an agent response against criteria.
   */
  async evaluate(
    response: string,
    context: string,
    criteria?: string[],
  ): Promise<EvaluateResponse> {{
    const payload: Record<string, unknown> = {{ response, context }};
    if (criteria) payload.criteria = criteria;

    const resp = await fetch(`${{this.baseUrl}}/v1/evaluate`, {{
      method: 'POST',
      headers: this.headers,
      body: JSON.stringify(payload),
    }});
    if (!resp.ok) throw new Error(`HTTP ${{resp.status}}: ${{resp.statusText}}`);
    return resp.json() as Promise<EvaluateResponse>;
  }}

  /**
   * Create a new agent persona for a tenant.
   */
  async createPersona(
    tenantId: string,
    agentRole: string,
    persona: Record<string, unknown>,
  ): Promise<CreatePersonaResponse> {{
    const payload = {{ tenant_id: tenantId, agent_role: agentRole, persona }};
    const resp = await fetch(`${{this.baseUrl}}/v1/personas`, {{
      method: 'POST',
      headers: this.headers,
      body: JSON.stringify(payload),
    }});
    if (!resp.ok) throw new Error(`HTTP ${{resp.status}}: ${{resp.statusText}}`);
    return resp.json() as Promise<CreatePersonaResponse>;
  }}

  /**
   * List all personas for a tenant.
   */
  async listPersonas(tenantId: string): Promise<ListPersonasResponse> {{
    const resp = await fetch(
      `${{this.baseUrl}}/v1/personas?tenant_id=${{encodeURIComponent(tenantId)}}`,
      {{ headers: this.headers }},
    );
    if (!resp.ok) throw new Error(`HTTP ${{resp.status}}: ${{resp.statusText}}`);
    return resp.json() as Promise<ListPersonasResponse>;
  }}

  /**
   * Get usage breakdown for a tenant.
   */
  async getUsage(tenantId: string): Promise<UsageBreakdown> {{
    const resp = await fetch(
      `${{this.baseUrl}}/v1/usage/${{encodeURIComponent(tenantId)}}`,
      {{ headers: this.headers }},
    );
    if (!resp.ok) throw new Error(`HTTP ${{resp.status}}: ${{resp.statusText}}`);
    return resp.json() as Promise<UsageBreakdown>;
  }}

  /**
   * Check API server health.
   */
  async health(): Promise<HealthResponse> {{
    const resp = await fetch(`${{this.baseUrl}}/health`, {{
      headers: this.headers,
    }});
    if (!resp.ok) throw new Error(`HTTP ${{resp.status}}: ${{resp.statusText}}`);
    return resp.json() as Promise<HealthResponse>;
  }}

  /**
   * Forward a webhook event through the proxy.
   */
  async webhookProxy(
    event: string,
    data: Record<string, unknown>,
    options?: {{ source?: string; secret?: string }},
  ): Promise<WebhookProxyResponse> {{
    const payload: Record<string, unknown> = {{ event, data }};
    if (options?.source) payload.source = options.source;
    if (options?.secret) payload.secret = options.secret;

    const resp = await fetch(`${{this.baseUrl}}/v1/webhooks/proxy`, {{
      method: 'POST',
      headers: this.headers,
      body: JSON.stringify(payload),
    }});
    if (!resp.ok) throw new Error(`HTTP ${{resp.status}}: ${{resp.statusText}}`);
    return resp.json() as Promise<WebhookProxyResponse>;
  }}
}}

export {{ ArgentorClient as default }};
export type * from './types';
"#,
        name = config.package_name,
        base_url = base_url,
    )
}

fn generate_ts_types(_config: &SdkConfig) -> String {
    r#"/**
 * TypeScript type definitions for the Argentor API.
 */

// ---------------------------------------------------------------------------
// Run Task
// ---------------------------------------------------------------------------

export interface RunTaskRequest {
  agent_role: string;
  context: string;
  model?: string;
  max_tokens?: number;
  tools?: string[];
}

export interface RunTaskResponse {
  task_id: string;
  status: string;
  result?: string;
  tokens_used?: number;
  duration_ms?: number;
  metadata?: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Batch
// ---------------------------------------------------------------------------

export interface BatchTask {
  agent_role: string;
  context: string;
  model?: string;
  max_tokens?: number;
}

export interface BatchRequest {
  tasks: BatchTask[];
  max_concurrent: number;
}

export interface BatchTaskResult {
  task_id: string;
  status: string;
  result?: string;
  error?: string;
}

export interface BatchResponse {
  batch_id: string;
  results: BatchTaskResult[];
  total: number;
  succeeded: number;
  failed: number;
}

// ---------------------------------------------------------------------------
// Evaluate
// ---------------------------------------------------------------------------

export interface EvaluateRequest {
  response: string;
  context: string;
  criteria?: string[];
}

export interface CriterionScore {
  criterion: string;
  score: number;
  explanation?: string;
}

export interface EvaluateResponse {
  overall_score: number;
  scores: CriterionScore[];
  summary?: string;
}

// ---------------------------------------------------------------------------
// Personas
// ---------------------------------------------------------------------------

export interface PersonaConfig {
  name: string;
  system_prompt?: string;
  temperature?: number;
  model?: string;
  tools?: string[];
  metadata?: Record<string, unknown>;
}

export interface CreatePersonaRequest {
  tenant_id: string;
  agent_role: string;
  persona: PersonaConfig;
}

export interface CreatePersonaResponse {
  persona_id: string;
  tenant_id: string;
  agent_role: string;
  created_at: string;
}

export interface PersonaSummary {
  persona_id: string;
  agent_role: string;
  name: string;
  created_at: string;
}

export interface ListPersonasResponse {
  tenant_id: string;
  personas: PersonaSummary[];
}

// ---------------------------------------------------------------------------
// Usage
// ---------------------------------------------------------------------------

export interface ModelUsage {
  model: string;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  cost_usd?: number;
}

export interface UsageBreakdown {
  tenant_id: string;
  period_start: string;
  period_end: string;
  models: ModelUsage[];
  total_tokens: number;
  total_cost_usd?: number;
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

export interface HealthResponse {
  status: string;
  version: string;
  uptime_seconds?: number;
}

// ---------------------------------------------------------------------------
// Webhook Proxy
// ---------------------------------------------------------------------------

export interface WebhookProxyRequest {
  event: string;
  data: Record<string, unknown>;
  source?: string;
  secret?: string;
}

export interface WebhookProxyResponse {
  accepted: boolean;
  event_id?: string;
  message?: string;
}
"#
    .to_string()
}

fn generate_ts_streaming(_config: &SdkConfig) -> String {
    r#"/**
 * SSE streaming helpers for the Argentor TypeScript SDK.
 */

/**
 * Parse a Server-Sent Events response into an async generator of parsed data
 * objects. Stops when it encounters a `data: [DONE]` message.
 */
export async function* parseSSEStream(
  response: Response,
): AsyncGenerator<Record<string, unknown>> {
  const reader = response.body?.getReader();
  if (!reader) {
    throw new Error('Response body is not readable');
  }

  const decoder = new TextDecoder();
  let buffer = '';

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() ?? '';

      for (const line of lines) {
        const trimmed = line.trim();
        if (trimmed.startsWith('data: ')) {
          const data = trimmed.slice('data: '.length).trim();
          if (data === '[DONE]') {
            return;
          }
          yield JSON.parse(data) as Record<string, unknown>;
        }
      }
    }

    // Process any remaining buffer content
    if (buffer.trim().startsWith('data: ')) {
      const data = buffer.trim().slice('data: '.length).trim();
      if (data && data !== '[DONE]') {
        yield JSON.parse(data) as Record<string, unknown>;
      }
    }
  } finally {
    reader.releaseLock();
  }
}
"#
    .to_string()
}

fn generate_ts_package_json(config: &SdkConfig) -> String {
    format!(
        r#"{{
  "name": "{name}",
  "version": "{version}",
  "description": "TypeScript SDK client for the Argentor API",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "files": [
    "dist",
    "src"
  ],
  "scripts": {{
    "build": "tsc",
    "clean": "rm -rf dist",
    "prepublishOnly": "npm run build",
    "test": "vitest run",
    "lint": "eslint src/"
  }},
  "keywords": [
    "argentor",
    "ai",
    "agent",
    "sdk",
    "client"
  ],
  "license": "AGPL-3.0-only",
  "devDependencies": {{
    "typescript": "^5.4.0",
    "vitest": "^1.6.0",
    "eslint": "^9.0.0"
  }}
}}
"#,
        name = config.package_name,
        version = config.version,
    )
}

fn generate_ts_tsconfig() -> String {
    r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2022", "DOM"],
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "outDir": "dist",
    "rootDir": "src",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true
  },
  "include": ["src/**/*.ts"],
  "exclude": ["node_modules", "dist"]
}
"#
    .to_string()
}

fn generate_ts_readme(config: &SdkConfig) -> String {
    format!(
        r#"# {name}

TypeScript SDK client for the Argentor API.

## Installation

```bash
npm install {name}
```

## Quick Start

```typescript
import {{ ArgentorClient }} from '{name}';

const client = new ArgentorClient({{
  baseUrl: '{base_url}',
  apiKey: 'your-api-key',
  tenantId: 'your-tenant-id',
}});

// Run a task
const result = await client.runTask(
  'code_reviewer',
  'Review the following pull request...',
);
console.log(result);

// Stream results
for await (const chunk of client.runTaskStream(
  'assistant',
  'Explain how Argentor works',
)) {{
  process.stdout.write(JSON.stringify(chunk));
}}

// Batch execution
const batchResult = await client.batch([
  {{ agentRole: 'analyst', context: 'Analyze Q1 sales data' }},
  {{ agentRole: 'analyst', context: 'Analyze Q2 sales data' }},
]);

// Evaluate a response
const evaluation = await client.evaluate(
  'The code looks clean and follows best practices.',
  'Code review task',
  ['accuracy', 'completeness', 'helpfulness'],
);

// Health check
const health = await client.health();
console.log(health);
```

## API Reference

### ArgentorClient

| Method | Description |
|--------|-------------|
| `runTask(agentRole, context, options?)` | Execute a single agent task |
| `runTaskStream(agentRole, context, options?)` | Stream task results via SSE |
| `batch(tasks, options?)` | Submit a batch of tasks |
| `evaluate(response, context, criteria?)` | Evaluate an agent response |
| `createPersona(tenantId, agentRole, persona)` | Create a new persona |
| `listPersonas(tenantId)` | List personas for a tenant |
| `getUsage(tenantId)` | Get usage breakdown |
| `health()` | Check API health |
| `webhookProxy(event, data, options?)` | Forward a webhook event |
"#,
        name = config.package_name,
        base_url = config.base_url,
    )
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> SdkConfig {
        SdkConfig::default()
    }

    // -----------------------------------------------------------------------
    // SdkConfig defaults
    // -----------------------------------------------------------------------

    #[test]
    fn test_sdk_config_defaults() {
        let cfg = SdkConfig::default();
        assert_eq!(cfg.base_url, "http://localhost:3000");
        assert_eq!(cfg.package_name, "argentor_client");
        assert_eq!(cfg.version, "0.1.0");
        assert!(cfg.include_async);
        assert!(cfg.include_streaming);
    }

    #[test]
    fn test_sdk_config_custom_values() {
        let cfg = SdkConfig {
            base_url: "https://api.example.com".to_string(),
            package_name: "my_sdk".to_string(),
            version: "2.0.0".to_string(),
            include_async: false,
            include_streaming: false,
        };
        assert_eq!(cfg.base_url, "https://api.example.com");
        assert_eq!(cfg.package_name, "my_sdk");
        assert_eq!(cfg.version, "2.0.0");
        assert!(!cfg.include_async);
        assert!(!cfg.include_streaming);
    }

    // -----------------------------------------------------------------------
    // SdkGenerator constructor
    // -----------------------------------------------------------------------

    #[test]
    fn test_sdk_generator_new() {
        let _gen = SdkGenerator::new();
        // Verifies construction does not panic.
    }

    #[test]
    fn test_sdk_generator_default() {
        let _gen = SdkGenerator::default();
    }

    // -----------------------------------------------------------------------
    // Python SDK structure
    // -----------------------------------------------------------------------

    #[test]
    fn test_python_sdk_file_count_with_streaming() {
        let gen = SdkGenerator::new();
        let output = gen.generate_python(&default_config());
        assert_eq!(output.language, "python");
        // __init__.py, client.py, models.py, streaming.py, setup.py, README.md
        assert_eq!(output.files.len(), 6);
    }

    #[test]
    fn test_python_sdk_file_count_without_streaming() {
        let gen = SdkGenerator::new();
        let cfg = SdkConfig {
            include_streaming: false,
            ..default_config()
        };
        let output = gen.generate_python(&cfg);
        // __init__.py, client.py, models.py, setup.py, README.md (no streaming.py)
        assert_eq!(output.files.len(), 5);
    }

    #[test]
    fn test_python_sdk_all_file_paths_present() {
        let gen = SdkGenerator::new();
        let output = gen.generate_python(&default_config());
        let paths: Vec<&str> = output.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"argentor_client/__init__.py"));
        assert!(paths.contains(&"argentor_client/client.py"));
        assert!(paths.contains(&"argentor_client/models.py"));
        assert!(paths.contains(&"argentor_client/streaming.py"));
        assert!(paths.contains(&"setup.py"));
        assert!(paths.contains(&"README.md"));
    }

    #[test]
    fn test_python_init_exports_argentor_client() {
        let gen = SdkGenerator::new();
        let output = gen.generate_python(&default_config());
        let init = output
            .files
            .iter()
            .find(|f| f.path.ends_with("__init__.py"))
            .unwrap();
        assert!(init
            .content
            .contains("from argentor_client.client import ArgentorClient"));
        assert!(init.content.contains("__version__"));
    }

    #[test]
    fn test_python_client_has_all_methods() {
        let gen = SdkGenerator::new();
        let output = gen.generate_python(&default_config());
        let client = output
            .files
            .iter()
            .find(|f| f.path.ends_with("client.py"))
            .unwrap();
        assert!(client.content.contains("def run_task("));
        assert!(client.content.contains("def run_task_stream("));
        assert!(client.content.contains("def batch("));
        assert!(client.content.contains("def evaluate("));
        assert!(client.content.contains("def create_persona("));
        assert!(client.content.contains("def list_personas("));
        assert!(client.content.contains("def get_usage("));
        assert!(client.content.contains("def health("));
        assert!(client.content.contains("def webhook_proxy("));
    }

    #[test]
    fn test_python_client_imports_httpx() {
        let gen = SdkGenerator::new();
        let output = gen.generate_python(&default_config());
        let client = output
            .files
            .iter()
            .find(|f| f.path.ends_with("client.py"))
            .unwrap();
        assert!(client.content.contains("import httpx"));
    }

    #[test]
    fn test_python_client_has_async_client() {
        let gen = SdkGenerator::new();
        let output = gen.generate_python(&default_config());
        let client = output
            .files
            .iter()
            .find(|f| f.path.ends_with("client.py"))
            .unwrap();
        assert!(client.content.contains("class AsyncArgentorClient:"));
    }

    #[test]
    fn test_python_client_no_async_when_disabled() {
        let gen = SdkGenerator::new();
        let cfg = SdkConfig {
            include_async: false,
            ..default_config()
        };
        let output = gen.generate_python(&cfg);
        let client = output
            .files
            .iter()
            .find(|f| f.path.ends_with("client.py"))
            .unwrap();
        assert!(!client.content.contains("class AsyncArgentorClient:"));
    }

    #[test]
    fn test_python_models_has_pydantic_classes() {
        let gen = SdkGenerator::new();
        let output = gen.generate_python(&default_config());
        let models = output
            .files
            .iter()
            .find(|f| f.path.ends_with("models.py"))
            .unwrap();
        assert!(models.content.contains("class RunTaskRequest(BaseModel):"));
        assert!(models.content.contains("class RunTaskResponse(BaseModel):"));
        assert!(models.content.contains("class BatchRequest(BaseModel):"));
        assert!(models.content.contains("class BatchResponse(BaseModel):"));
        assert!(models.content.contains("class EvaluateRequest(BaseModel):"));
        assert!(models
            .content
            .contains("class EvaluateResponse(BaseModel):"));
        assert!(models
            .content
            .contains("class CreatePersonaRequest(BaseModel):"));
        assert!(models
            .content
            .contains("class CreatePersonaResponse(BaseModel):"));
        assert!(models
            .content
            .contains("class ListPersonasResponse(BaseModel):"));
        assert!(models.content.contains("class UsageBreakdown(BaseModel):"));
        assert!(models.content.contains("class HealthResponse(BaseModel):"));
        assert!(models
            .content
            .contains("class WebhookProxyRequest(BaseModel):"));
        assert!(models
            .content
            .contains("class WebhookProxyResponse(BaseModel):"));
    }

    #[test]
    fn test_python_setup_has_package_metadata() {
        let gen = SdkGenerator::new();
        let output = gen.generate_python(&default_config());
        let setup = output.files.iter().find(|f| f.path == "setup.py").unwrap();
        assert!(setup.content.contains("name=\"argentor_client\""));
        assert!(setup.content.contains("version=\"0.1.0\""));
        assert!(setup.content.contains("httpx"));
        assert!(setup.content.contains("pydantic"));
    }

    #[test]
    fn test_python_client_uses_custom_base_url() {
        let gen = SdkGenerator::new();
        let cfg = SdkConfig {
            base_url: "https://my-api.example.com:8080".to_string(),
            ..default_config()
        };
        let output = gen.generate_python(&cfg);
        let client = output
            .files
            .iter()
            .find(|f| f.path.ends_with("client.py"))
            .unwrap();
        assert!(client.content.contains("https://my-api.example.com:8080"));
    }

    #[test]
    fn test_python_streaming_helper() {
        let gen = SdkGenerator::new();
        let output = gen.generate_python(&default_config());
        let streaming = output
            .files
            .iter()
            .find(|f| f.path.ends_with("streaming.py"))
            .unwrap();
        assert!(streaming.content.contains("class SSEStream:"));
        assert!(streaming.content.contains("class AsyncSSEStream:"));
        assert!(streaming.content.contains("[DONE]"));
    }

    // -----------------------------------------------------------------------
    // TypeScript SDK structure
    // -----------------------------------------------------------------------

    #[test]
    fn test_typescript_sdk_file_count_with_streaming() {
        let gen = SdkGenerator::new();
        let output = gen.generate_typescript(&default_config());
        assert_eq!(output.language, "typescript");
        // src/index.ts, src/types.ts, src/streaming.ts, package.json, tsconfig.json, README.md
        assert_eq!(output.files.len(), 6);
    }

    #[test]
    fn test_typescript_sdk_file_count_without_streaming() {
        let gen = SdkGenerator::new();
        let cfg = SdkConfig {
            include_streaming: false,
            ..default_config()
        };
        let output = gen.generate_typescript(&cfg);
        // src/index.ts, src/types.ts, package.json, tsconfig.json, README.md (no streaming.ts)
        assert_eq!(output.files.len(), 5);
    }

    #[test]
    fn test_typescript_sdk_all_file_paths_present() {
        let gen = SdkGenerator::new();
        let output = gen.generate_typescript(&default_config());
        let paths: Vec<&str> = output.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"src/index.ts"));
        assert!(paths.contains(&"src/types.ts"));
        assert!(paths.contains(&"src/streaming.ts"));
        assert!(paths.contains(&"package.json"));
        assert!(paths.contains(&"tsconfig.json"));
        assert!(paths.contains(&"README.md"));
    }

    #[test]
    fn test_typescript_index_has_all_methods() {
        let gen = SdkGenerator::new();
        let output = gen.generate_typescript(&default_config());
        let index = output
            .files
            .iter()
            .find(|f| f.path == "src/index.ts")
            .unwrap();
        assert!(index.content.contains("async runTask("));
        assert!(index.content.contains("runTaskStream("));
        assert!(index.content.contains("async batch("));
        assert!(index.content.contains("async evaluate("));
        assert!(index.content.contains("async createPersona("));
        assert!(index.content.contains("async listPersonas("));
        assert!(index.content.contains("async getUsage("));
        assert!(index.content.contains("async health("));
        assert!(index.content.contains("async webhookProxy("));
    }

    #[test]
    fn test_typescript_types_has_all_interfaces() {
        let gen = SdkGenerator::new();
        let output = gen.generate_typescript(&default_config());
        let types = output
            .files
            .iter()
            .find(|f| f.path == "src/types.ts")
            .unwrap();
        assert!(types.content.contains("export interface RunTaskRequest"));
        assert!(types.content.contains("export interface RunTaskResponse"));
        assert!(types.content.contains("export interface BatchRequest"));
        assert!(types.content.contains("export interface BatchResponse"));
        assert!(types.content.contains("export interface EvaluateRequest"));
        assert!(types.content.contains("export interface EvaluateResponse"));
        assert!(types
            .content
            .contains("export interface CreatePersonaRequest"));
        assert!(types
            .content
            .contains("export interface CreatePersonaResponse"));
        assert!(types
            .content
            .contains("export interface ListPersonasResponse"));
        assert!(types.content.contains("export interface UsageBreakdown"));
        assert!(types.content.contains("export interface HealthResponse"));
        assert!(types
            .content
            .contains("export interface WebhookProxyRequest"));
        assert!(types
            .content
            .contains("export interface WebhookProxyResponse"));
    }

    #[test]
    fn test_typescript_package_json_metadata() {
        let gen = SdkGenerator::new();
        let output = gen.generate_typescript(&default_config());
        let pkg = output
            .files
            .iter()
            .find(|f| f.path == "package.json")
            .unwrap();
        assert!(pkg.content.contains("\"name\": \"argentor_client\""));
        assert!(pkg.content.contains("\"version\": \"0.1.0\""));
        assert!(pkg.content.contains("typescript"));
    }

    #[test]
    fn test_typescript_tsconfig() {
        let gen = SdkGenerator::new();
        let output = gen.generate_typescript(&default_config());
        let tsconfig = output
            .files
            .iter()
            .find(|f| f.path == "tsconfig.json")
            .unwrap();
        assert!(tsconfig.content.contains("\"strict\": true"));
        assert!(tsconfig.content.contains("\"declaration\": true"));
        assert!(tsconfig.content.contains("\"outDir\": \"dist\""));
    }

    #[test]
    fn test_typescript_streaming_helper() {
        let gen = SdkGenerator::new();
        let output = gen.generate_typescript(&default_config());
        let streaming = output
            .files
            .iter()
            .find(|f| f.path == "src/streaming.ts")
            .unwrap();
        assert!(streaming.content.contains("parseSSEStream"));
        assert!(streaming.content.contains("AsyncGenerator"));
        assert!(streaming.content.contains("[DONE]"));
    }

    // -----------------------------------------------------------------------
    // generate_all
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_all_returns_both_languages() {
        let gen = SdkGenerator::new();
        let outputs = gen.generate_all(&default_config());
        assert_eq!(outputs.len(), 2);
        let languages: Vec<&str> = outputs.iter().map(|o| o.language.as_str()).collect();
        assert!(languages.contains(&"python"));
        assert!(languages.contains(&"typescript"));
    }

    #[test]
    fn test_generate_all_total_file_count() {
        let gen = SdkGenerator::new();
        let outputs = gen.generate_all(&default_config());
        let total_files: usize = outputs.iter().map(|o| o.files.len()).sum();
        // 6 (python) + 6 (typescript) = 12
        assert_eq!(total_files, 12);
    }

    // -----------------------------------------------------------------------
    // Custom package name propagation
    // -----------------------------------------------------------------------

    #[test]
    fn test_custom_package_name_python() {
        let gen = SdkGenerator::new();
        let cfg = SdkConfig {
            package_name: "my_custom_sdk".to_string(),
            ..default_config()
        };
        let output = gen.generate_python(&cfg);
        let paths: Vec<&str> = output.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"my_custom_sdk/__init__.py"));
        assert!(paths.contains(&"my_custom_sdk/client.py"));
        assert!(paths.contains(&"my_custom_sdk/models.py"));
    }

    #[test]
    fn test_custom_package_name_typescript() {
        let gen = SdkGenerator::new();
        let cfg = SdkConfig {
            package_name: "my_custom_sdk".to_string(),
            ..default_config()
        };
        let output = gen.generate_typescript(&cfg);
        let pkg = output
            .files
            .iter()
            .find(|f| f.path == "package.json")
            .unwrap();
        assert!(pkg.content.contains("\"name\": \"my_custom_sdk\""));
    }

    // -----------------------------------------------------------------------
    // SDK serialization round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_sdk_output_serialization() {
        let gen = SdkGenerator::new();
        let output = gen.generate_python(&default_config());
        let json_str = serde_json::to_string(&output).unwrap();
        let deserialized: SdkOutput = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.language, "python");
        assert_eq!(deserialized.files.len(), output.files.len());
    }

    #[test]
    fn test_sdk_config_json_deserialization() {
        let json = r#"{"base_url":"https://example.com","package_name":"test_sdk","version":"1.0.0","include_async":false,"include_streaming":true}"#;
        let cfg: SdkConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.base_url, "https://example.com");
        assert_eq!(cfg.package_name, "test_sdk");
        assert_eq!(cfg.version, "1.0.0");
        assert!(!cfg.include_async);
        assert!(cfg.include_streaming);
    }

    #[test]
    fn test_sdk_config_json_defaults() {
        let json = r#"{}"#;
        let cfg: SdkConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.base_url, "http://localhost:3000");
        assert_eq!(cfg.package_name, "argentor_client");
        assert_eq!(cfg.version, "0.1.0");
        assert!(cfg.include_async);
        assert!(cfg.include_streaming);
    }

    // -----------------------------------------------------------------------
    // No empty files generated
    // -----------------------------------------------------------------------

    #[test]
    fn test_no_empty_files_python() {
        let gen = SdkGenerator::new();
        let output = gen.generate_python(&default_config());
        for file in &output.files {
            assert!(
                !file.content.is_empty(),
                "Python file {} should not be empty",
                file.path
            );
        }
    }

    #[test]
    fn test_no_empty_files_typescript() {
        let gen = SdkGenerator::new();
        let output = gen.generate_typescript(&default_config());
        for file in &output.files {
            assert!(
                !file.content.is_empty(),
                "TypeScript file {} should not be empty",
                file.path
            );
        }
    }
}
