"""argentor_client — Python client for the Argentor API."""

import httpx
import json
from typing import Optional, Dict, List, Any, Iterator, AsyncIterator


class ArgentorClient:
    """Synchronous client for the Argentor API."""

    def __init__(
        self,
        base_url: str = "http://localhost:3000",
        api_key: str = "",
        tenant_id: str = "",
        timeout: float = 60.0,
    ):
        self.base_url = base_url
        self.headers = {"X-API-Key": api_key, "X-Tenant-ID": tenant_id}
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
        payload: Dict[str, Any] = {"agent_role": agent_role, "context": context}
        if model is not None:
            payload["model"] = model
        if max_tokens is not None:
            payload["max_tokens"] = max_tokens
        if tools is not None:
            payload["tools"] = tools
        resp = self._client.post("/v1/run", json=payload)
        resp.raise_for_status()
        return resp.json()

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

    def batch(
        self,
        tasks: List[Dict[str, Any]],
        *,
        max_concurrent: int = 5,
    ) -> dict:
        """Submit a batch of tasks for parallel execution."""
        payload = {"tasks": tasks, "max_concurrent": max_concurrent}
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
        payload: Dict[str, Any] = {"response": response, "context": context}
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
        payload = {"tenant_id": tenant_id, "agent_role": agent_role, "persona": persona}
        resp = self._client.post("/v1/personas", json=payload)
        resp.raise_for_status()
        return resp.json()

    def list_personas(self, tenant_id: str) -> dict:
        """List all personas for a tenant."""
        resp = self._client.get("/v1/personas", params={"tenant_id": tenant_id})
        resp.raise_for_status()
        return resp.json()

    def get_usage(self, tenant_id: str) -> dict:
        """Get usage breakdown for a tenant."""
        resp = self._client.get(f"/v1/usage/{tenant_id}")
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
        payload: Dict[str, Any] = {"event": event, "data": data}
        if source:
            payload["source"] = source
        if secret:
            payload["secret"] = secret
        resp = self._client.post("/v1/webhooks/proxy", json=payload)
        resp.raise_for_status()
        return resp.json()


class AsyncArgentorClient:
    """Async client for the Argentor API."""

    def __init__(
        self,
        base_url: str = "http://localhost:3000",
        api_key: str = "",
        tenant_id: str = "",
        timeout: float = 60.0,
    ):
        self.base_url = base_url
        self.headers = {"X-API-Key": api_key, "X-Tenant-ID": tenant_id}
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
        payload: Dict[str, Any] = {"agent_role": agent_role, "context": context}
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
        payload: Dict[str, Any] = {"agent_role": agent_role, "context": context}
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
        payload = {"tasks": tasks, "max_concurrent": max_concurrent}
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
        payload: Dict[str, Any] = {"response": response, "context": context}
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
        payload = {"tenant_id": tenant_id, "agent_role": agent_role, "persona": persona}
        resp = await self._client.post("/v1/personas", json=payload)
        resp.raise_for_status()
        return resp.json()

    async def list_personas(self, tenant_id: str) -> dict:
        """List all personas for a tenant."""
        resp = await self._client.get("/v1/personas", params={"tenant_id": tenant_id})
        resp.raise_for_status()
        return resp.json()

    async def get_usage(self, tenant_id: str) -> dict:
        """Get usage breakdown for a tenant."""
        resp = await self._client.get(f"/v1/usage/{tenant_id}")
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
        payload: Dict[str, Any] = {"event": event, "data": data}
        if source:
            payload["source"] = source
        if secret:
            payload["secret"] = secret
        resp = await self._client.post("/v1/webhooks/proxy", json=payload)
        resp.raise_for_status()
        return resp.json()
