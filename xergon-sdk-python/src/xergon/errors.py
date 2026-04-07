"""
Typed exceptions for the Xergon Relay API.

Relay errors follow the format: {"error": {"type": "...", "message": "...", "code": N}}
"""

from __future__ import annotations

from enum import Enum
from typing import Any, Optional


class XergonErrorType(str, Enum):
    INVALID_REQUEST = "invalid_request"
    UNAUTHORIZED = "unauthorized"
    FORBIDDEN = "forbidden"
    NOT_FOUND = "not_found"
    RATE_LIMIT_ERROR = "rate_limit_error"
    INTERNAL_ERROR = "internal_error"
    SERVICE_UNAVAILABLE = "service_unavailable"


class XergonError(Exception):
    """Base error for all Xergon relay API errors."""

    def __init__(
        self,
        message: str,
        error_type: str = "internal_error",
        code: int = 500,
    ) -> None:
        super().__init__(message)
        self.message = message
        self.type = error_type
        self.code = code

    @classmethod
    def from_response(cls, data: Any) -> XergonError:
        """Parse a relay error response into the appropriate typed exception."""
        if isinstance(data, dict) and "error" in data:
            err = data["error"]
            if isinstance(err, dict):
                error_type = err.get("type", "internal_error")
                message = err.get("message", "Unknown error")
                code = err.get("code", 500)
                return _error_for_type(error_type, message, code)

        # Fallback for malformed responses
        message = data.get("message", "Unknown error") if isinstance(data, dict) else "Unknown error"
        return cls(message=message, error_type="internal_error", code=500)

    @property
    def is_unauthorized(self) -> bool:
        return self.type == XergonErrorType.UNAUTHORIZED

    @property
    def is_rate_limited(self) -> bool:
        return self.type == XergonErrorType.RATE_LIMIT_ERROR

    @property
    def is_not_found(self) -> bool:
        return self.type == XergonErrorType.NOT_FOUND

    @property
    def is_service_unavailable(self) -> bool:
        return self.type == XergonErrorType.SERVICE_UNAVAILABLE

    def __repr__(self) -> str:
        return f"{self.__class__.__name__}(type={self.type!r}, message={self.message!r}, code={self.code})"


class AuthenticationError(XergonError):
    """Raised when authentication fails (401)."""

    def __init__(self, message: str = "Authentication failed") -> None:
        super().__init__(message=message, error_type="unauthorized", code=401)


class BadRequestError(XergonError):
    """Raised for invalid requests (400)."""

    def __init__(self, message: str = "Bad request") -> None:
        super().__init__(message=message, error_type="invalid_request", code=400)


class RateLimitError(XergonError):
    """Raised when rate limit is exceeded (429)."""

    def __init__(self, message: str = "Rate limit exceeded", retry_after: Optional[float] = None) -> None:
        super().__init__(message=message, error_type="rate_limit_error", code=429)
        self.retry_after = retry_after


class ProviderUnavailableError(XergonError):
    """Raised when no providers are available (503)."""

    def __init__(self, message: str = "No providers available") -> None:
        super().__init__(message=message, error_type="service_unavailable", code=503)


class NotFoundError(XergonError):
    """Raised when a resource is not found (404)."""

    def __init__(self, message: str = "Resource not found") -> None:
        super().__init__(message=message, error_type="not_found", code=404)


def _error_for_type(error_type: str, message: str, code: int) -> XergonError:
    """Map an error type string to the appropriate exception class."""
    mapping = {
        "unauthorized": AuthenticationError,
        "forbidden": AuthenticationError,
        "invalid_request": BadRequestError,
        "rate_limit_error": RateLimitError,
        "service_unavailable": ProviderUnavailableError,
        "not_found": NotFoundError,
    }
    cls = mapping.get(error_type, XergonError)
    return cls(message=message)  # type: ignore[call-arg]
