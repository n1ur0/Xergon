"""
Xergon SDK for Python -- client for the Xergon Network decentralized AI relay.

Usage::

    from xergon import XergonClient

    client = XergonClient(base_url="https://relay.xergon.gg")
    completion = client.chat(
        messages=[{"role": "user", "content": "Hello!"}],
        model="qwen3.5-32b",
    )
    print(completion.choices[0].message.content)
"""

__version__ = "0.1.0"

from xergon.client import AsyncXergonClient, XergonClient
from xergon.errors import (
    AuthenticationError,
    BadRequestError,
    NotFoundError,
    ProviderUnavailableError,
    RateLimitError,
    XergonError,
)
from xergon.streaming import stream_chat
from xergon.types import (
    BalanceResponse,
    ChatCompletion,
    ChatCompletionChunk,
    ChatMessage,
    GpuListing,
    GpuRental,
    Model,
    Provider,
    Usage,
)

__all__ = [
    "AsyncXergonClient",
    "XergonClient",
    "AuthenticationError",
    "BadRequestError",
    "NotFoundError",
    "ProviderUnavailableError",
    "RateLimitError",
    "XergonError",
    "stream_chat",
    "BalanceResponse",
    "ChatCompletion",
    "ChatCompletionChunk",
    "ChatMessage",
    "GpuListing",
    "GpuRental",
    "Model",
    "Provider",
    "Usage",
]
