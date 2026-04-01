"""Pydantic models for Argentor API request/response types."""

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
