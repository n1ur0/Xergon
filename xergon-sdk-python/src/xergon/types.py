"""
Pydantic v2 models for all Xergon Relay API responses.

These models match the JSON shapes from the relay's OpenAPI spec (snake_case wire format).
"""

from __future__ import annotations

from enum import Enum
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


# ── Chat Completions (OpenAI-compatible) ─────────────────────────────────


class ChatRole(str, Enum):
    SYSTEM = "system"
    USER = "user"
    ASSISTANT = "assistant"


class ChatMessage(BaseModel):
    role: ChatRole
    content: str


class Usage(BaseModel):
    prompt_tokens: int = 0
    completion_tokens: int = 0
    total_tokens: int = 0


class FinishReason(str, Enum):
    STOP = "stop"
    LENGTH = "length"
    CONTENT_FILTER = "content_filter"


class ChatCompletionChoice(BaseModel):
    index: int = 0
    message: ChatMessage
    finish_reason: Optional[FinishReason] = None


class ChatCompletion(BaseModel):
    id: str
    object: str = "chat.completion"
    created: int
    model: str
    choices: List[ChatCompletionChoice]
    usage: Optional[Usage] = None


class ChatCompletionDelta(BaseModel):
    role: Optional[ChatRole] = None
    content: Optional[str] = None


class ChatCompletionChunkChoice(BaseModel):
    index: int = 0
    delta: ChatCompletionDelta
    finish_reason: Optional[FinishReason] = None


class ChatCompletionChunk(BaseModel):
    id: str
    object: str = "chat.completion.chunk"
    created: int
    model: str
    choices: List[ChatCompletionChunkChoice]


# ── Models ───────────────────────────────────────────────────────────────


class Model(BaseModel):
    id: str
    object: str = "model"
    owned_by: str = ""
    pricing: Optional[str] = None


class ModelsResponse(BaseModel):
    object: str = "list"
    data: List[Model]


# ── Providers ────────────────────────────────────────────────────────────


class Provider(BaseModel):
    public_key: str
    endpoint: str = ""
    models: List[str] = Field(default_factory=list)
    region: str = ""
    pown_score: float = 0.0
    last_heartbeat: Optional[int] = None
    pricing: Optional[Dict[str, str]] = None


class LeaderboardEntry(Provider):
    online: Optional[bool] = None
    total_requests: Optional[int] = None
    total_prompt_tokens: Optional[int] = None
    total_completion_tokens: Optional[int] = None
    total_tokens: Optional[int] = None


# ── Balance ──────────────────────────────────────────────────────────────


class BalanceResponse(BaseModel):
    public_key: str
    balance_nanoerg: str = "0"
    balance_erg: str = "0"
    staking_box_id: Optional[str] = None


# ── GPU Bazar ────────────────────────────────────────────────────────────


class GpuRentalStatus(str, Enum):
    ACTIVE = "active"
    EXPIRED = "expired"
    COMPLETED = "completed"


class GpuListing(BaseModel):
    listing_id: str
    provider_pk: str
    gpu_type: str
    vram_gb: Optional[int] = None
    price_per_hour_nanoerg: str = "0"
    region: str = ""
    available: bool = False
    bandwidth_mbps: Optional[int] = None


class GpuRental(BaseModel):
    rental_id: str
    listing_id: str
    provider_pk: str
    renter_pk: str
    hours: int
    cost_nanoerg: str = "0"
    started_at: int
    expires_at: int
    status: GpuRentalStatus


class GpuPricing(BaseModel):
    avg_price_per_hour: str = "0"
    models: Dict[str, str] = Field(default_factory=dict)


class GpuReputation(BaseModel):
    public_key: str
    score: float
    total_ratings: int
    average: float


# ── Incentive ────────────────────────────────────────────────────────────


class IncentiveStatus(BaseModel):
    active: bool = False
    total_bonus_erg: str = "0"
    rare_models_count: int = 0


class RareModel(BaseModel):
    model: str
    rarity_score: float
    bonus_multiplier: float
    providers_count: int


# ── Bridge ───────────────────────────────────────────────────────────────


class BridgeChain(str, Enum):
    BTC = "btc"
    ETH = "eth"
    ADA = "ada"


class BridgeInvoiceStatus(str, Enum):
    PENDING = "pending"
    CONFIRMED = "confirmed"
    REFUNDED = "refunded"
    EXPIRED = "expired"


class BridgeInvoice(BaseModel):
    invoice_id: str
    amount_nanoerg: str = "0"
    chain: BridgeChain
    status: BridgeInvoiceStatus
    created_at: int
    refund_timeout: int


class BridgeStatus(BaseModel):
    status: str
    supported_chains: List[str] = Field(default_factory=list)


# ── Health ───────────────────────────────────────────────────────────────


class HealthStatus(BaseModel):
    status: str
    version: Optional[str] = None
    uptime_secs: Optional[int] = None
    ergo_node_connected: Optional[bool] = None
    active_providers: Optional[int] = None
    total_providers: Optional[int] = None


# ── Auth ─────────────────────────────────────────────────────────────────


class AuthStatus(BaseModel):
    authenticated: bool
    public_key: str = ""
    tier: str = "trial"


# ── Error ────────────────────────────────────────────────────────────────


class ErrorBody(BaseModel):
    type: str
    message: str
    code: int


class ErrorResponse(BaseModel):
    error: ErrorBody
