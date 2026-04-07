"""Unit tests for xergon-sdk-python using pytest + respx."""

from __future__ import annotations

import json

import httpx
import pytest
import respx

from xergon import XergonClient, XergonError, AsyncXergonClient
from xergon.auth import XergonAuth, build_hmac_payload, hmac_sign, hmac_verify
from xergon.errors import (
    AuthenticationError,
    BadRequestError,
    NotFoundError,
    ProviderUnavailableError,
    RateLimitError,
)
from xergon.types import (
    ChatCompletion,
    ChatCompletionChunk,
    ChatMessage,
    Model,
    Provider,
    BalanceResponse,
    AuthStatus,
    GpuListing,
    GpuRental,
    IncentiveStatus,
    BridgeInvoice,
    BridgeStatus,
)

BASE_URL = "https://relay.xergon.gg"


# ── Fixtures ─────────────────────────────────────────────────────────────

@pytest.fixture
def mock_client():
    with respx.mock(base_url=BASE_URL) as respx_mock:
        yield respx_mock


@pytest.fixture
def client(mock_client):
    return XergonClient(base_url=BASE_URL)


# ── Health ───────────────────────────────────────────────────────────────

class TestHealth:

    def test_health_ok(self, client: XergonClient, mock_client: respx.MockRouter):
        route = mock_client.get("/health").mock(return_value=httpx.Response(200, text="OK"))
        result = client.health()
        assert result == "OK"
        assert route.called

    def test_ready_ok(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_client.get("/ready").mock(return_value=httpx.Response(200))
        assert client.ready() is True

    def test_ready_not_ready(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_client.get("/ready").mock(return_value=httpx.Response(503, json={
            "error": {"type": "service_unavailable", "message": "Not ready", "code": 503}
        }))
        assert client.ready() is False


# ── Models ───────────────────────────────────────────────────────────────

class TestModels:

    def test_models_list(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_response = {
            "object": "list",
            "data": [
                {"id": "qwen3.5-32b", "object": "model", "owned_by": "provider1", "pricing": "100"},
                {"id": "llama3-70b", "object": "model", "owned_by": "provider2", "pricing": "200"},
            ],
        }
        mock_client.get("/v1/models").mock(
            return_value=httpx.Response(200, json=mock_response)
        )
        models = client.models()
        assert len(models) == 2
        assert models[0].id == "qwen3.5-32b"
        assert models[0].pricing == "100"
        assert isinstance(models[0], Model)

    def test_models_empty(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_client.get("/v1/models").mock(
            return_value=httpx.Response(200, json={"object": "list", "data": []})
        )
        assert client.models() == []


# ── Chat Completion ──────────────────────────────────────────────────────

class TestChatCompletion:

    def test_chat_completion(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_response = {
            "id": "chatcmpl-abc123",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "qwen3.5-32b",
            "choices": [
                {
                    "index": 0,
                    "message": {"role": "assistant", "content": "Hello! How can I help?"},
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30,
            },
        }
        mock_client.post("/v1/chat/completions").mock(
            return_value=httpx.Response(200, json=mock_response)
        )
        result = client.chat(
            messages=[{"role": "user", "content": "Hi"}],
            model="qwen3.5-32b",
            temperature=0.7,
            max_tokens=1024,
        )
        assert isinstance(result, ChatCompletion)
        assert result.id == "chatcmpl-abc123"
        assert result.choices[0].message.content == "Hello! How can I help?"
        assert result.usage.total_tokens == 30

        # Verify the request body
        route = mock_client.routes[0]
        request = route.calls[0].request
        body = json.loads(request.content)
        assert body["model"] == "qwen3.5-32b"
        assert body["stream"] is False
        assert body["temperature"] == 0.7
        assert body["max_tokens"] == 1024

    def test_chat_completion_with_defaults(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_response = {
            "id": "chatcmpl-def456",
            "object": "chat.completion",
            "created": 1700000001,
            "model": "llama3-70b",
            "choices": [
                {
                    "index": 0,
                    "message": {"role": "assistant", "content": "Hi there!"},
                    "finish_reason": "stop",
                }
            ],
        }
        mock_client.post("/v1/chat/completions").mock(
            return_value=httpx.Response(200, json=mock_response)
        )
        result = client.chat(
            messages=[{"role": "user", "content": "Hello"}],
            model="llama3-70b",
        )
        assert result.model == "llama3-70b"


# ── Providers ────────────────────────────────────────────────────────────

class TestProviders:

    def test_providers_list(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_response = [
            {
                "public_key": "0xabc",
                "endpoint": "https://provider1.example.com",
                "models": ["qwen3.5-32b"],
                "region": "us-east",
                "pown_score": 95.5,
            }
        ]
        mock_client.get("/v1/providers").mock(
            return_value=httpx.Response(200, json=mock_response)
        )
        providers = client.providers()
        assert len(providers) == 1
        assert providers[0].public_key == "0xabc"
        assert providers[0].models == ["qwen3.5-32b"]
        assert isinstance(providers[0], Provider)


# ── Balance ──────────────────────────────────────────────────────────────

class TestBalance:

    def test_balance(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_response = {
            "public_key": "0xuser123",
            "balance_nanoerg": "1000000000",
            "balance_erg": "1.0",
            "staking_box_id": "box123",
        }
        mock_client.get("/v1/balance/0xuser123").mock(
            return_value=httpx.Response(200, json=mock_response)
        )
        bal = client.balance("0xuser123")
        assert isinstance(bal, BalanceResponse)
        assert bal.balance_erg == "1.0"
        assert bal.staking_box_id == "box123"


# ── Auth Status ──────────────────────────────────────────────────────────

class TestAuthStatus:

    def test_auth_status(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_response = {
            "authenticated": True,
            "public_key": "0xabc",
            "tier": "standard",
        }
        mock_client.get("/v1/auth/status").mock(
            return_value=httpx.Response(200, json=mock_response)
        )
        status = client.auth_status()
        assert isinstance(status, AuthStatus)
        assert status.authenticated is True
        assert status.tier == "standard"


# ── Error Handling ───────────────────────────────────────────────────────

class TestErrors:

    def test_unauthorized_error(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_client.post("/v1/chat/completions").mock(
            return_value=httpx.Response(401, json={
                "error": {"type": "unauthorized", "message": "Invalid HMAC signature", "code": 401}
            })
        )
        with pytest.raises(AuthenticationError) as exc_info:
            client.chat(
                messages=[{"role": "user", "content": "test"}],
                model="qwen3.5-32b",
            )
        assert exc_info.value.code == 401
        assert "Invalid HMAC signature" in exc_info.value.message

    def test_rate_limit_error(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_client.post("/v1/chat/completions").mock(
            return_value=httpx.Response(429, json={
                "error": {"type": "rate_limit_error", "message": "Rate limit exceeded", "code": 429}
            })
        )
        with pytest.raises(RateLimitError) as exc_info:
            client.chat(
                messages=[{"role": "user", "content": "test"}],
                model="qwen3.5-32b",
            )
        assert exc_info.value.code == 429

    def test_service_unavailable_error(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_client.post("/v1/chat/completions").mock(
            return_value=httpx.Response(503, json={
                "error": {"type": "service_unavailable", "message": "No providers available", "code": 503}
            })
        )
        with pytest.raises(ProviderUnavailableError):
            client.chat(
                messages=[{"role": "user", "content": "test"}],
                model="qwen3.5-32b",
            )

    def test_bad_request_error(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_client.post("/v1/chat/completions").mock(
            return_value=httpx.Response(400, json={
                "error": {"type": "invalid_request", "message": "Missing required field: model", "code": 400}
            })
        )
        with pytest.raises(BadRequestError):
            client.chat(
                messages=[{"role": "user", "content": "test"}],
                model="qwen3.5-32b",
            )

    def test_not_found_error(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_client.get("/v1/gpu/listings/nonexistent").mock(
            return_value=httpx.Response(404, json={
                "error": {"type": "not_found", "message": "Resource not found", "code": 404}
            })
        )
        with pytest.raises(NotFoundError):
            client.gpu_listing("nonexistent")

    def test_error_from_malformed_response(self):
        data = {"message": "something went wrong"}
        err = XergonError.from_response(data)
        assert isinstance(err, XergonError)
        assert err.message == "something went wrong"
        assert err.code == 500


# ── HMAC Auth ────────────────────────────────────────────────────────────

class TestAuth:

    def test_hmac_sign_and_verify(self):
        private_key = "a" * 64  # 32-byte hex key
        message = "test message"
        sig = hmac_sign(message, private_key)
        assert len(sig) == 64  # SHA-256 hex output
        assert hmac_verify(message, sig, private_key) is True
        assert hmac_verify("wrong message", sig, private_key) is False

    def test_build_hmac_payload(self):
        payload = build_hmac_payload('{"key":"value"}', 1700000000)
        assert payload == '{"key":"value"}1700000000'

    def test_auth_headers_without_private_key(self):
        auth = XergonAuth(public_key="0xabc")
        headers = auth.build_headers("POST", "/v1/chat/completions", '{"test": true}')
        assert headers["X-Xergon-Public-Key"] == "0xabc"
        assert "X-Xergon-Signature" not in headers
        assert "X-Xergon-Timestamp" not in headers

    def test_auth_headers_with_full_keypair(self):
        auth = XergonAuth(public_key="0xabc", private_key="a" * 64)
        headers = auth.build_headers("POST", "/v1/chat/completions", '{"test": true}')
        assert headers["X-Xergon-Public-Key"] == "0xabc"
        assert "X-Xergon-Signature" in headers
        assert "X-Xergon-Timestamp" in headers

    def test_auth_no_public_key(self):
        auth = XergonAuth()
        headers = auth.build_headers("GET", "/v1/models")
        assert headers == {}

    def test_authenticate_and_clear(self):
        auth = XergonAuth()
        auth.authenticate("0xpub", "a" * 64)
        assert auth.public_key == "0xpub"
        assert auth.private_key == "a" * 64
        auth.clear()
        assert auth.public_key is None
        assert auth.private_key is None

    def test_client_auth_headers_sent(self, client: XergonClient, mock_client: respx.MockRouter):
        client.authenticate("0xpub", "a" * 64)
        route = mock_client.get("/v1/models").mock(
            return_value=httpx.Response(200, json={"object": "list", "data": []})
        )
        client.models()
        request = route.calls[0].request
        assert request.headers["x-xergon-public-key"] == "0xpub"
        assert "x-xergon-signature" in request.headers


# ── GPU Bazar ────────────────────────────────────────────────────────────

class TestGpuBazar:

    def test_gpu_listings(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_response = [
            {
                "listing_id": "list1",
                "provider_pk": "0xprov1",
                "gpu_type": "A100",
                "vram_gb": 80,
                "price_per_hour_nanoerg": "50000000",
                "region": "us-east",
                "available": True,
            }
        ]
        mock_client.get("/v1/gpu/listings").mock(
            return_value=httpx.Response(200, json=mock_response)
        )
        listings = client.gpu_listings()
        assert len(listings) == 1
        assert listings[0].gpu_type == "A100"
        assert listings[0].vram_gb == 80

    def test_rent_gpu(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_response = {
            "rental_id": "rent1",
            "listing_id": "list1",
            "provider_pk": "0xprov1",
            "renter_pk": "0xrenter",
            "hours": 4,
            "cost_nanoerg": "200000000",
            "started_at": 1700000000,
            "expires_at": 1700014400,
            "status": "active",
        }
        mock_client.post("/v1/gpu/rent").mock(
            return_value=httpx.Response(200, json=mock_response)
        )
        rental = client.rent_gpu("list1", 4)
        assert isinstance(rental, GpuRental)
        assert rental.hours == 4
        assert rental.status.value == "active"


# ── Incentive ────────────────────────────────────────────────────────────

class TestIncentive:

    def test_incentive_status(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_response = {
            "active": True,
            "total_bonus_erg": "1000",
            "rare_models_count": 5,
        }
        mock_client.get("/v1/incentive/status").mock(
            return_value=httpx.Response(200, json=mock_response)
        )
        status = client.incentive_status()
        assert isinstance(status, IncentiveStatus)
        assert status.active is True
        assert status.rare_models_count == 5


# ── Bridge ───────────────────────────────────────────────────────────────

class TestBridge:

    def test_bridge_status(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_response = {
            "status": "operational",
            "supported_chains": ["btc", "eth", "ada"],
        }
        mock_client.get("/v1/bridge/status").mock(
            return_value=httpx.Response(200, json=mock_response)
        )
        status = client.bridge_status()
        assert isinstance(status, BridgeStatus)
        assert status.supported_chains == ["btc", "eth", "ada"]

    def test_bridge_invoices(self, client: XergonClient, mock_client: respx.MockRouter):
        mock_response = [
            {
                "invoice_id": "inv1",
                "amount_nanoerg": "1000000000",
                "chain": "eth",
                "status": "pending",
                "created_at": 1700000000,
                "refund_timeout": 1700086400,
            }
        ]
        mock_client.get("/v1/bridge/invoices").mock(
            return_value=httpx.Response(200, json=mock_response)
        )
        invoices = client.bridge_invoices()
        assert len(invoices) == 1
        assert isinstance(invoices[0], BridgeInvoice)
        assert invoices[0].chain.value == "eth"


# ── Context Manager ──────────────────────────────────────────────────────

class TestContextManager:

    def test_context_manager(self, mock_client: respx.MockRouter):
        mock_client.get("/health").mock(return_value=httpx.Response(200, text="OK"))
        with XergonClient(base_url=BASE_URL) as c:
            assert c.health() == "OK"
        # After exiting, the client should be closed

    def test_api_key_alias(self):
        client = XergonClient(base_url=BASE_URL, api_key="0xkey123")
        assert client.auth.public_key == "0xkey123"
        client.close()
