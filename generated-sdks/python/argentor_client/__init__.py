"""argentor_client — Python client for the Argentor API."""

from argentor_client.client import ArgentorClient
from argentor_client.models import (
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
from argentor_client.streaming import SSEStream

__version__ = "0.1.0"

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
