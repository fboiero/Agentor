"""Custom exceptions for the Argentor SDK."""

from __future__ import annotations

from typing import Any, Dict, Optional


class ArgentorError(Exception):
    """Base exception for all Argentor SDK errors."""

    def __init__(self, message: str) -> None:
        self.message = message
        super().__init__(message)


class ArgentorAPIError(ArgentorError):
    """Raised when the Argentor API returns an error response (4xx / 5xx)."""

    def __init__(
        self,
        message: str,
        status_code: int,
        response_body: Optional[Dict[str, Any]] = None,
    ) -> None:
        self.status_code = status_code
        self.response_body = response_body or {}
        super().__init__(f"HTTP {status_code}: {message}")


class ArgentorConnectionError(ArgentorError):
    """Raised when the SDK cannot connect to the Argentor server."""


class ArgentorTimeoutError(ArgentorError):
    """Raised when a request to the Argentor API times out."""
