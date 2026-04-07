# Xergon SDK for Python

Python SDK for the [Xergon Network](https://degens.world) decentralized AI compute relay on Ergo blockchain.

The relay provides an OpenAI-compatible API that routes inference requests to GPU providers.

## Installation

```bash
pip install xergon-sdk
```

Or install in development mode:

```bash
git clone https://github.com/Xergon-Network/xergon-sdk-python.git
cd xergon-sdk-python
pip install -e ".[dev]"
```

## Quickstart

### Basic Chat Completion

```python
from xergon import XergonClient

client = XergonClient(base_url="https://relay.xergon.gg")

completion = client.chat(
    messages=[{"role": "user", "content": "Hello, Xergon!"}],
    model="qwen3.5-32b",
)

print(completion.choices[0].message.content)
```

### With Authentication

```python
from xergon import XergonClient

client = XergonClient()
client.authenticate(
    public_key="your_ergo_public_key_hex",
    private_key="your_ergo_private_key_hex",
)

completion = client.chat(
    messages=[{"role": "user", "content": "Authenticated request"}],
    model="qwen3.5-32b",
    temperature=0.8,
    max_tokens=2048,
)
```

### Streaming (Sync)

```python
from xergon import XergonClient

client = XergonClient()

for chunk in client.stream_chat(
    messages=[{"role": "user", "content": "Tell me a story"}],
    model="qwen3.5-32b",
):
    if chunk.choices and chunk.choices[0].delta.content:
        print(chunk.choices[0].delta.content, end="", flush=True)
print()
```

### Streaming (Async)

```python
import asyncio
from xergon import AsyncXergonClient

async def main():
    client = AsyncXergonClient()
    async with client:
        stream = await client.stream_chat(
            messages=[{"role": "user", "content": "Hello!"}],
            model="qwen3.5-32b",
        )
        async for chunk in stream:
            if chunk.choices and chunk.choices[0].delta.content:
                print(chunk.choices[0].delta.content, end="", flush=True)
        print()

asyncio.run(main())
```

### List Models

```python
from xergon import XergonClient

client = XergonClient()
models = client.models()
for m in models:
    print(f"{m.id} (owned by {m.owned_by}, pricing: {m.pricing})")
```

### List Providers

```python
from xergon import XergonClient

client = XergonClient()
providers = client.providers()
for p in providers:
    print(f"PK: {p.public_key}, models: {p.models}, PoNW: {p.pown_score}")
```

### Check Balance

```python
from xergon import XergonClient

client = XergonClient()
balance = client.balance("your_public_key_hex")
print(f"Balance: {balance.balance_erg} ERG")
```

### Health Check

```python
from xergon import XergonClient

client = XergonClient()
print(client.health())  # "OK"
print(client.ready())   # True
```

## API Reference

### XergonClient (sync)

| Method | Returns | Description |
|--------|---------|-------------|
| `chat(messages, model, ...)` | `ChatCompletion` | Non-streaming chat completion |
| `stream_chat(messages, model, ...)` | `Generator[ChatCompletionChunk]` | SSE streaming chat |
| `models()` | `List[Model]` | List available models |
| `providers()` | `List[Provider]` | List active providers |
| `leaderboard(limit, offset)` | `List[LeaderboardEntry]` | Provider leaderboard |
| `balance(user_pk)` | `BalanceResponse` | Check ERG balance |
| `auth_status()` | `AuthStatus` | Verify authentication |
| `health()` | `str` | Liveness probe |
| `ready()` | `bool` | Readiness probe |
| `gpu_listings(...)` | `List[GpuListing]` | Browse GPU marketplace |
| `rent_gpu(listing_id, hours)` | `GpuRental` | Rent a GPU |
| `gpu_rentals(renter_pk)` | `List[GpuRental]` | Active GPU rentals |
| `incentive_status()` | `IncentiveStatus` | Incentive system status |
| `bridge_status()` | `BridgeStatus` | Cross-chain bridge status |

### AsyncXergonClient

Same API surface as `XergonClient` but all methods are `async` and return awaitables. Use `async with` for the context manager.

### Authentication

```python
# HMAC auth with full keypair
client.authenticate(public_key="0x...", private_key="0x...")

# Public key only (wallet-managed signing)
client.set_public_key("0x...")

# Clear credentials
client.clear_auth()
```

### Error Handling

```python
from xergon import (
    XergonError,
    AuthenticationError,
    RateLimitError,
    ProviderUnavailableError,
    BadRequestError,
    NotFoundError,
)

try:
    completion = client.chat(messages=[...], model="...")
except AuthenticationError as e:
    print(f"Auth failed: {e.message}")
except RateLimitError as e:
    print(f"Rate limited: {e.message}, retry_after={e.retry_after}")
except ProviderUnavailableError as e:
    print(f"No providers: {e.message}")
except XergonError as e:
    print(f"API error: {e.type} - {e.message}")
```

## Requirements

- Python >= 3.9
- httpx >= 0.24.0
- pydantic >= 2.0

## Development

```bash
pip install -e ".[dev]"
pytest
```

## License

MIT
