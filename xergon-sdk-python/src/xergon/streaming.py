"""
SSE streaming support for Xergon chat completions.

Provides both sync and async streaming helpers that parse the SSE stream
from the relay into ChatCompletionChunk objects.
"""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Dict, Generator, Iterator, List, Optional, Union

import httpx

from .types import ChatCompletionChunk

if TYPE_CHECKING:
    from .auth import XergonAuth

__all__ = ["stream_chat", "astream_chat"]


def _parse_sse_chunks(line_iter: Iterator[str]) -> Generator[ChatCompletionChunk, None, None]:
    """Parse SSE lines into ChatCompletionChunk objects.

    Args:
        line_iter: Iterator yielding individual lines from the SSE stream.

    Yields:
        ChatCompletionChunk objects parsed from the stream.
    """
    for line in line_iter:
        line = line.strip()
        if not line or line.startswith(":"):
            continue
        if line.startswith("data: "):
            data = line[6:]
            if data.strip() == "[DONE]":
                return
            try:
                chunk_data = json.loads(data)
                yield ChatCompletionChunk.model_validate(chunk_data)
            except (json.JSONDecodeError, Exception):
                # Skip malformed chunks
                continue


async def _parse_sse_chunks_async(
    line_iter: "AsyncIterator[str]",
) -> "AsyncGenerator[ChatCompletionChunk, None]":
    """Async version of _parse_sse_chunks."""
    async for line in line_iter:
        line = line.strip()
        if not line or line.startswith(":"):
            continue
        if line.startswith("data: "):
            data = line[6:]
            if data.strip() == "[DONE]":
                return
            try:
                chunk_data = json.loads(data)
                yield ChatCompletionChunk.model_validate(chunk_data)
            except (json.JSONDecodeError, Exception):
                continue


def stream_chat(
    client: httpx.Client,
    *,
    base_url: str,
    messages: List[Dict[str, str]],
    model: str,
    temperature: Optional[float] = None,
    max_tokens: Optional[int] = None,
    top_p: Optional[float] = None,
    auth: Optional["XergonAuth"] = None,
    extra_headers: Optional[Dict[str, str]] = None,
) -> Generator[ChatCompletionChunk, None, None]:
    """Stream a chat completion via SSE (sync).

    Args:
        client: httpx.Client instance.
        base_url: Relay base URL.
        messages: List of message dicts with "role" and "content".
        model: Model identifier.
        temperature: Sampling temperature (0-2).
        max_tokens: Maximum tokens to generate.
        top_p: Nucleus sampling parameter.
        auth: Optional XergonAuth instance for HMAC signing.
        extra_headers: Additional headers to send.

    Yields:
        ChatCompletionChunk objects as they arrive.
    """
    body: Dict[str, object] = {
        "model": model,
        "messages": messages,
        "stream": True,
    }
    if temperature is not None:
        body["temperature"] = temperature
    if max_tokens is not None:
        body["max_tokens"] = max_tokens
    if top_p is not None:
        body["top_p"] = top_p

    body_str = json.dumps(body)

    headers: Dict[str, str] = {
        "Content-Type": "application/json",
        "Accept": "text/event-stream",
        **(extra_headers or {}),
    }

    if auth:
        headers.update(auth.build_headers("POST", "/v1/chat/completions", body_str))

    url = f"{base_url.rstrip('/')}/v1/chat/completions"

    with client.stream("POST", url, content=body_str, headers=headers) as response:
        response.raise_for_status()
        yield from _parse_sse_chunks(response.iter_lines())


async def astream_chat(
    client: httpx.AsyncClient,
    *,
    base_url: str,
    messages: List[Dict[str, str]],
    model: str,
    temperature: Optional[float] = None,
    max_tokens: Optional[int] = None,
    top_p: Optional[float] = None,
    auth: Optional["XergonAuth"] = None,
    extra_headers: Optional[Dict[str, str]] = None,
) -> "AsyncGenerator[ChatCompletionChunk, None]":
    """Stream a chat completion via SSE (async).

    Args:
        client: httpx.AsyncClient instance.
        base_url: Relay base URL.
        messages: List of message dicts with "role" and "content".
        model: Model identifier.
        temperature: Sampling temperature (0-2).
        max_tokens: Maximum tokens to generate.
        top_p: Nucleus sampling parameter.
        auth: Optional XergonAuth instance for HMAC signing.
        extra_headers: Additional headers to send.

    Yields:
        ChatCompletionChunk objects as they arrive.
    """
    body: Dict[str, object] = {
        "model": model,
        "messages": messages,
        "stream": True,
    }
    if temperature is not None:
        body["temperature"] = temperature
    if max_tokens is not None:
        body["max_tokens"] = max_tokens
    if top_p is not None:
        body["top_p"] = top_p

    body_str = json.dumps(body)

    headers: Dict[str, str] = {
        "Content-Type": "application/json",
        "Accept": "text/event-stream",
        **(extra_headers or {}),
    }

    if auth:
        headers.update(auth.build_headers("POST", "/v1/chat/completions", body_str))

    url = f"{base_url.rstrip('/')}/v1/chat/completions"

    async with client.stream("POST", url, content=body_str, headers=headers) as response:
        response.raise_for_status()
        async for chunk in _parse_sse_chunks_async(response.aiter_lines()):
            yield chunk
