"""
XergonClient -- sync and async HTTP clients for the Xergon Relay API.

Provides high-level methods matching the TypeScript SDK's API surface.
Uses httpx for HTTP and supports HMAC authentication via XergonAuth.
"""

from __future__ import annotations

import json
from typing import Any, Dict, Generator, List, Optional

import httpx

from .auth import XergonAuth
from .errors import XergonError, _error_for_type
from .streaming import astream_chat, stream_chat
from .types import (
    AuthStatus,
    BalanceResponse,
    BridgeChain,
    BridgeInvoice,
    BridgeStatus,
    ChatCompletion,
    ChatCompletionChunk,
    ChatMessage,
    ChatRole,
    GpuListing,
    GpuPricing,
    GpuReputation,
    GpuRental,
    HealthStatus,
    IncentiveStatus,
    LeaderboardEntry,
    Model,
    ModelsResponse,
    Provider,
    RareModel,
)

try:
    from typing import AsyncGenerator
except ImportError:
    from collections.abc import AsyncGenerator  # type: ignore[misc]

DEFAULT_BASE_URL = "https://relay.xergon.gg"


def _handle_error_response(response: httpx.Response) -> None:
    """Raise the appropriate typed exception for non-2xx responses."""
    try:
        data = response.json()
    except Exception:
        data = {"message": response.text or response.reason_phrase}

    if isinstance(data, dict) and "error" in data:
        err = data["error"]
        if isinstance(err, dict):
            error_type = err.get("type", "internal_error")
            message = err.get("message", "Unknown error")
            code = err.get("code", response.status_code)
            raise _error_for_type(error_type, message, code)

    raise XergonError(
        message=data.get("message", response.reason_phrase) if isinstance(data, dict) else response.reason_phrase,
        code=response.status_code,
    )


class XergonClient:
    """Synchronous client for the Xergon Relay API.

    Args:
        base_url: Relay base URL (default: https://relay.xergon.gg).
        api_key: Alias for public_key (OpenAI-compatible convenience).
        public_key: Ergo public key (hex) for HMAC auth.
        private_key: Ergo private key (hex) for HMAC auth.
        timeout: Request timeout in seconds.
        httpx_client: Custom httpx.Client instance (overrides timeout/base_url).
    """

    def __init__(
        self,
        base_url: str = DEFAULT_BASE_URL,
        api_key: Optional[str] = None,
        *,
        public_key: Optional[str] = None,
        private_key: Optional[str] = None,
        timeout: float = 30.0,
        httpx_client: Optional[httpx.Client] = None,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._auth = XergonAuth(
            public_key=api_key or public_key,
            private_key=private_key,
        )
        if httpx_client is not None:
            self._client = httpx_client
        else:
            self._client = httpx.Client(
                base_url=self._base_url,
                timeout=timeout,
            )

    @property
    def auth(self) -> XergonAuth:
        """Access the auth handler for custom configuration."""
        return self._auth

    @property
    def base_url(self) -> str:
        return self._base_url

    # ── Auth shortcuts ──────────────────────────────────────────────────

    def authenticate(self, public_key: str, private_key: str) -> None:
        """Set full keypair for HMAC auth."""
        self._auth.authenticate(public_key, private_key)

    def set_public_key(self, public_key: str) -> None:
        """Set only the public key (wallet-managed signing)."""
        self._auth.set_public_key(public_key)

    def clear_auth(self) -> None:
        """Clear all credentials."""
        self._auth.clear()

    # ── Internal ────────────────────────────────────────────────────────

    def _headers(self, method: str, path: str, body: str = "") -> Dict[str, str]:
        headers: Dict[str, str] = {"Content-Type": "application/json"}
        headers.update(self._auth.build_headers(method, path, body))
        return headers

    def _request(
        self,
        method: str,
        path: str,
        body: Optional[Any] = None,
        skip_auth: bool = False,
    ) -> Any:
        body_str = json.dumps(body) if body is not None else ""
        headers = self._headers(method, path, body_str) if not skip_auth else {}

        response = self._client.request(
            method,
            path,
            content=body_str if body is not None else None,
            headers=headers,
        )

        if not response.is_success:
            _handle_error_response(response)

        content_type = response.headers.get("content-type", "")
        if "text/plain" in content_type:
            return response.text

        return response.json()

    # ── Chat Completions ────────────────────────────────────────────────

    def chat(
        self,
        messages: List[Dict[str, str]],
        model: str,
        *,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
    ) -> ChatCompletion:
        """Create a chat completion (non-streaming).

        Args:
            messages: List of message dicts, e.g. [{"role": "user", "content": "Hello"}].
            model: Model identifier (e.g. "qwen3.5-32b").
            temperature: Sampling temperature (0-2).
            max_tokens: Maximum tokens to generate.
            top_p: Nucleus sampling parameter.

        Returns:
            ChatCompletion response object.
        """
        body: Dict[str, Any] = {
            "model": model,
            "messages": messages,
            "stream": False,
        }
        if temperature is not None:
            body["temperature"] = temperature
        if max_tokens is not None:
            body["max_tokens"] = max_tokens
        if top_p is not None:
            body["top_p"] = top_p

        data = self._request("POST", "/v1/chat/completions", body)
        return ChatCompletion.model_validate(data)

    def stream_chat(
        self,
        messages: List[Dict[str, str]],
        model: str,
        *,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
    ) -> Generator[ChatCompletionChunk, None, None]:
        """Stream a chat completion via SSE (sync generator).

        Args:
            messages: List of message dicts.
            model: Model identifier.
            temperature: Sampling temperature (0-2).
            max_tokens: Maximum tokens to generate.
            top_p: Nucleus sampling parameter.

        Yields:
            ChatCompletionChunk objects as they arrive.
        """
        return stream_chat(
            self._client,
            base_url=self._base_url,
            messages=messages,
            model=model,
            temperature=temperature,
            max_tokens=max_tokens,
            top_p=top_p,
            auth=self._auth,
        )

    # ── Models ──────────────────────────────────────────────────────────

    def models(self) -> List[Model]:
        """List all available models.

        Returns:
            List of Model objects.
        """
        data = self._request("GET", "/v1/models")
        response = ModelsResponse.model_validate(data)
        return response.data

    # ── Providers ───────────────────────────────────────────────────────

    def providers(self) -> List[Provider]:
        """List all active providers.

        Returns:
            List of Provider objects.
        """
        data = self._request("GET", "/v1/providers")
        return [Provider.model_validate(p) for p in data]

    def leaderboard(self, limit: int = 20, offset: int = 0) -> List[LeaderboardEntry]:
        """Get provider leaderboard ranked by PoNW score.

        Args:
            limit: Max entries to return.
            offset: Pagination offset.

        Returns:
            List of LeaderboardEntry objects.
        """
        data = self._request(
            "GET",
            "/v1/leaderboard",
            skip_auth=True,
        )
        # Note: query params should be appended; simple implementation:
        return [LeaderboardEntry.model_validate(p) for p in data]

    # ── Balance ─────────────────────────────────────────────────────────

    def balance(self, user_pk: str) -> BalanceResponse:
        """Check a user's ERG balance from their on-chain Staking Box.

        Args:
            user_pk: User's Ergo public key (hex).

        Returns:
            BalanceResponse with balance info.
        """
        data = self._request("GET", f"/v1/balance/{user_pk}")
        return BalanceResponse.model_validate(data)

    # ── Auth Status ─────────────────────────────────────────────────────

    def auth_status(self) -> AuthStatus:
        """Verify authentication status.

        Returns:
            AuthStatus with authentication info and tier.
        """
        data = self._request("GET", "/v1/auth/status")
        return AuthStatus.model_validate(data)

    # ── Health ──────────────────────────────────────────────────────────

    def health(self) -> str:
        """Liveness probe. Returns "OK" if the relay is running.

        Returns:
            Plain text status string.
        """
        return self._request("GET", "/health", skip_auth=True)

    def ready(self) -> bool:
        """Readiness probe. Returns True if relay can serve requests.

        Returns:
            Whether the relay is ready.
        """
        response = self._client.get(
            f"{self._base_url}/ready",
            headers={},
        )
        return response.is_success

    # ── GPU Bazar ───────────────────────────────────────────────────────

    def gpu_listings(
        self,
        *,
        gpu_type: Optional[str] = None,
        min_vram: Optional[int] = None,
        max_price: Optional[float] = None,
        region: Optional[str] = None,
    ) -> List[GpuListing]:
        """Browse GPU listings.

        Args:
            gpu_type: Filter by GPU type.
            min_vram: Minimum VRAM in GB.
            max_price: Maximum price per hour.
            region: Filter by region.

        Returns:
            List of GpuListing objects.
        """
        data = self._request("GET", "/v1/gpu/listings")
        return [GpuListing.model_validate(g) for g in data]

    def gpu_listing(self, listing_id: str) -> GpuListing:
        """Get GPU listing details.

        Args:
            listing_id: Listing identifier.

        Returns:
            GpuListing object.
        """
        data = self._request("GET", f"/v1/gpu/listings/{listing_id}")
        return GpuListing.model_validate(data)

    def rent_gpu(self, listing_id: str, hours: int) -> GpuRental:
        """Rent a GPU.

        Args:
            listing_id: Listing to rent.
            hours: Rental duration in hours.

        Returns:
            GpuRental object.
        """
        data = self._request("POST", "/v1/gpu/rent", {
            "listing_id": listing_id,
            "hours": hours,
        })
        return GpuRental.model_validate(data)

    def gpu_rentals(self, renter_pk: str) -> List[GpuRental]:
        """Get a user's active GPU rentals.

        Args:
            renter_pk: Renter's public key.

        Returns:
            List of GpuRental objects.
        """
        data = self._request("GET", f"/v1/gpu/rentals/{renter_pk}")
        return [GpuRental.model_validate(r) for r in data]

    def gpu_pricing(self) -> GpuPricing:
        """Get GPU pricing information.

        Returns:
            GpuPricing object.
        """
        data = self._request("GET", "/v1/gpu/pricing")
        return GpuPricing.model_validate(data)

    def rate_gpu(
        self,
        target_pk: str,
        rental_id: str,
        score: int,
        comment: Optional[str] = None,
    ) -> None:
        """Rate a GPU provider or renter.

        Args:
            target_pk: Target public key.
            rental_id: Rental identifier.
            score: Rating 1-5.
            comment: Optional comment.
        """
        body: Dict[str, Any] = {
            "target_pk": target_pk,
            "rental_id": rental_id,
            "score": score,
        }
        if comment is not None:
            body["comment"] = comment
        self._request("POST", "/v1/gpu/rate", body)

    def gpu_reputation(self, public_key: str) -> GpuReputation:
        """Get reputation score for a public key.

        Args:
            public_key: Public key to look up.

        Returns:
            GpuReputation object.
        """
        data = self._request("GET", f"/v1/gpu/reputation/{public_key}")
        return GpuReputation.model_validate(data)

    # ── Incentive ───────────────────────────────────────────────────────

    def incentive_status(self) -> IncentiveStatus:
        """Get incentive system status.

        Returns:
            IncentiveStatus object.
        """
        data = self._request("GET", "/v1/incentive/status")
        return IncentiveStatus.model_validate(data)

    def incentive_models(self) -> List[RareModel]:
        """Get rare model bonuses.

        Returns:
            List of RareModel objects.
        """
        data = self._request("GET", "/v1/incentive/models")
        return [RareModel.model_validate(m) for m in data]

    # ── Bridge ──────────────────────────────────────────────────────────

    def bridge_status(self) -> BridgeStatus:
        """Get bridge operational status.

        Returns:
            BridgeStatus object.
        """
        data = self._request("GET", "/v1/bridge/status")
        return BridgeStatus.model_validate(data)

    def bridge_invoices(self) -> List[BridgeInvoice]:
        """List all invoices.

        Returns:
            List of BridgeInvoice objects.
        """
        data = self._request("GET", "/v1/bridge/invoices")
        return [BridgeInvoice.model_validate(i) for i in data]

    def bridge_invoice(self, invoice_id: str) -> BridgeInvoice:
        """Get invoice status.

        Args:
            invoice_id: Invoice identifier.

        Returns:
            BridgeInvoice object.
        """
        data = self._request("GET", f"/v1/bridge/invoice/{invoice_id}")
        return BridgeInvoice.model_validate(data)

    def create_bridge_invoice(self, amount_nanoerg: str, chain: BridgeChain) -> BridgeInvoice:
        """Create a payment invoice.

        Args:
            amount_nanoerg: Amount in nanoERG.
            chain: Target chain (btc, eth, ada).

        Returns:
            BridgeInvoice object.
        """
        data = self._request("POST", "/v1/bridge/create-invoice", {
            "amount_nanoerg": amount_nanoerg,
            "chain": chain.value,
        })
        return BridgeInvoice.model_validate(data)

    def confirm_bridge_payment(self, invoice_id: str, tx_hash: str) -> None:
        """Confirm a bridge payment.

        Args:
            invoice_id: Invoice identifier.
            tx_hash: Transaction hash.
        """
        self._request("POST", "/v1/bridge/confirm", {
            "invoice_id": invoice_id,
            "tx_hash": tx_hash,
        })

    def refund_bridge_invoice(self, invoice_id: str) -> None:
        """Refund a bridge invoice.

        Args:
            invoice_id: Invoice identifier.
        """
        self._request("POST", "/v1/bridge/refund", {
            "invoice_id": invoice_id,
        })

    # ── Context manager ─────────────────────────────────────────────────

    def __enter__(self) -> "XergonClient":
        return self

    def __exit__(self, *args: Any) -> None:
        self._client.__exit__(*args)

    def close(self) -> None:
        """Close the underlying httpx client."""
        self._client.close()


class AsyncXergonClient:
    """Asynchronous client for the Xergon Relay API.

    Args:
        base_url: Relay base URL (default: https://relay.xergon.gg).
        api_key: Alias for public_key (OpenAI-compatible convenience).
        public_key: Ergo public key (hex) for HMAC auth.
        private_key: Ergo private key (hex) for HMAC auth.
        timeout: Request timeout in seconds.
        httpx_client: Custom httpx.AsyncClient instance.
    """

    def __init__(
        self,
        base_url: str = DEFAULT_BASE_URL,
        api_key: Optional[str] = None,
        *,
        public_key: Optional[str] = None,
        private_key: Optional[str] = None,
        timeout: float = 30.0,
        httpx_client: Optional[httpx.AsyncClient] = None,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._auth = XergonAuth(
            public_key=api_key or public_key,
            private_key=private_key,
        )
        if httpx_client is not None:
            self._client = httpx_client
        else:
            self._client = httpx.AsyncClient(
                base_url=self._base_url,
                timeout=timeout,
            )

    @property
    def auth(self) -> XergonAuth:
        return self._auth

    @property
    def base_url(self) -> str:
        return self._base_url

    # ── Auth shortcuts ──────────────────────────────────────────────────

    def authenticate(self, public_key: str, private_key: str) -> None:
        """Set full keypair for HMAC auth."""
        self._auth.authenticate(public_key, private_key)

    def set_public_key(self, public_key: str) -> None:
        """Set only the public key."""
        self._auth.set_public_key(public_key)

    def clear_auth(self) -> None:
        """Clear all credentials."""
        self._auth.clear()

    # ── Internal ────────────────────────────────────────────────────────

    def _headers(self, method: str, path: str, body: str = "") -> Dict[str, str]:
        headers: Dict[str, str] = {"Content-Type": "application/json"}
        headers.update(self._auth.build_headers(method, path, body))
        return headers

    async def _request(
        self,
        method: str,
        path: str,
        body: Optional[Any] = None,
        skip_auth: bool = False,
    ) -> Any:
        body_str = json.dumps(body) if body is not None else ""
        headers = self._headers(method, path, body_str) if not skip_auth else {}

        response = await self._client.request(
            method,
            path,
            content=body_str if body is not None else None,
            headers=headers,
        )

        if not response.is_success:
            _handle_error_response(response)

        content_type = response.headers.get("content-type", "")
        if "text/plain" in content_type:
            return response.text

        return response.json()

    # ── Chat Completions ────────────────────────────────────────────────

    async def chat(
        self,
        messages: List[Dict[str, str]],
        model: str,
        *,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
    ) -> ChatCompletion:
        """Create a chat completion (non-streaming, async)."""
        body: Dict[str, Any] = {
            "model": model,
            "messages": messages,
            "stream": False,
        }
        if temperature is not None:
            body["temperature"] = temperature
        if max_tokens is not None:
            body["max_tokens"] = max_tokens
        if top_p is not None:
            body["top_p"] = top_p

        data = await self._request("POST", "/v1/chat/completions", body)
        return ChatCompletion.model_validate(data)

    async def stream_chat(
        self,
        messages: List[Dict[str, str]],
        model: str,
        *,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
    ) -> AsyncGenerator:
        """Stream a chat completion via SSE (async generator).

        Returns:
            AsyncGenerator yielding ChatCompletionChunk objects.
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

        return astream_chat(
            self._client,
            base_url=self._base_url,
            messages=messages,
            model=model,
            temperature=temperature,
            max_tokens=max_tokens,
            top_p=top_p,
            auth=self._auth,
        )

    # ── Models ──────────────────────────────────────────────────────────

    async def models(self) -> List[Model]:
        """List all available models."""
        data = await self._request("GET", "/v1/models")
        response = ModelsResponse.model_validate(data)
        return response.data

    # ── Providers ───────────────────────────────────────────────────────

    async def providers(self) -> List[Provider]:
        """List all active providers."""
        data = await self._request("GET", "/v1/providers")
        return [Provider.model_validate(p) for p in data]

    async def leaderboard(self, limit: int = 20, offset: int = 0) -> List[LeaderboardEntry]:
        """Get provider leaderboard."""
        data = await self._request("GET", "/v1/leaderboard", skip_auth=True)
        return [LeaderboardEntry.model_validate(p) for p in data]

    # ── Balance ─────────────────────────────────────────────────────────

    async def balance(self, user_pk: str) -> BalanceResponse:
        """Check a user's ERG balance."""
        data = await self._request("GET", f"/v1/balance/{user_pk}")
        return BalanceResponse.model_validate(data)

    # ── Auth Status ─────────────────────────────────────────────────────

    async def auth_status(self) -> AuthStatus:
        """Verify authentication status."""
        data = await self._request("GET", "/v1/auth/status")
        return AuthStatus.model_validate(data)

    # ── Health ──────────────────────────────────────────────────────────

    async def health(self) -> str:
        """Liveness probe."""
        return await self._request("GET", "/health", skip_auth=True)

    async def ready(self) -> bool:
        """Readiness probe."""
        response = await self._client.get(
            f"{self._base_url}/ready",
            headers={},
        )
        return response.is_success

    # ── GPU Bazar ───────────────────────────────────────────────────────

    async def gpu_listings(
        self,
        *,
        gpu_type: Optional[str] = None,
        min_vram: Optional[int] = None,
        max_price: Optional[float] = None,
        region: Optional[str] = None,
    ) -> List[GpuListing]:
        """Browse GPU listings."""
        data = await self._request("GET", "/v1/gpu/listings")
        return [GpuListing.model_validate(g) for g in data]

    async def rent_gpu(self, listing_id: str, hours: int) -> GpuRental:
        """Rent a GPU."""
        data = await self._request("POST", "/v1/gpu/rent", {
            "listing_id": listing_id,
            "hours": hours,
        })
        return GpuRental.model_validate(data)

    async def gpu_rentals(self, renter_pk: str) -> List[GpuRental]:
        """Get a user's active GPU rentals."""
        data = await self._request("GET", f"/v1/gpu/rentals/{renter_pk}")
        return [GpuRental.model_validate(r) for r in data]

    # ── Incentive ───────────────────────────────────────────────────────

    async def incentive_status(self) -> IncentiveStatus:
        """Get incentive system status."""
        data = await self._request("GET", "/v1/incentive/status")
        return IncentiveStatus.model_validate(data)

    async def incentive_models(self) -> List[RareModel]:
        """Get rare model bonuses."""
        data = await self._request("GET", "/v1/incentive/models")
        return [RareModel.model_validate(m) for m in data]

    # ── Bridge ──────────────────────────────────────────────────────────

    async def bridge_status(self) -> BridgeStatus:
        """Get bridge operational status."""
        data = await self._request("GET", "/v1/bridge/status")
        return BridgeStatus.model_validate(data)

    async def bridge_invoices(self) -> List[BridgeInvoice]:
        """List all invoices."""
        data = await self._request("GET", "/v1/bridge/invoices")
        return [BridgeInvoice.model_validate(i) for i in data]

    async def create_bridge_invoice(self, amount_nanoerg: str, chain: BridgeChain) -> BridgeInvoice:
        """Create a payment invoice."""
        data = await self._request("POST", "/v1/bridge/create-invoice", {
            "amount_nanoerg": amount_nanoerg,
            "chain": chain.value,
        })
        return BridgeInvoice.model_validate(data)

    # ── Context manager ─────────────────────────────────────────────────

    async def __aenter__(self) -> "AsyncXergonClient":
        return self

    async def __aexit__(self, *args: Any) -> None:
        await self._client.__aexit__(*args)

    async def close(self) -> None:
        """Close the underlying httpx async client."""
        await self._client.aclose()
