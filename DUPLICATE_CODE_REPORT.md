# Duplicate Code Analysis Report

## Overview
Analysis of `/home/n1ur0x/xergon-network/xergon-sdk-python/` for duplicated utility functions and near-identical code patterns.

---

## 1. SSE Chunk Parsing Duplication

### Location: `streaming.py`
- **`_parse_sse_chunks`** (lines 23-45) - synchronous version
- **`_parse_sse_chunks_async`** (lines 48-64) - asynchronous version

### Pattern:
Both functions implement nearly identical logic with only `for` vs `async for` difference:

```python
# SYNC (lines 23-45):
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
            continue

# ASYNC (lines 48-64):
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
```

### Recommendation:
Extract common logic into a shared helper function that takes an iterator and yields parsed chunks, then wrap with sync/async adapters.

---

## 2. Error JSON Parsing Duplication

### Location: `client.py` and `errors.py`

**`client.py`** - `_handle_error_response` (lines 49-67):
```python
def _handle_error_response(response: httpx.Response) -> None:
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
```

**`errors.py`** - `XergonError.from_response` (lines 37-50):
```python
@classmethod
def from_response(cls, data: Any) -> XergonError:
    if isinstance(data, dict) and "error" in data:
        err = data["error"]
        if isinstance(err, dict):
            error_type = err.get("type", "internal_error")
            message = err.get("message", "Unknown error")
            code = err.get("code", 500)
            return _error_for_type(error_type, message, code)

    message = data.get("message", "Unknown error") if isinstance(data, dict) else "Unknown error"
    return cls(message=message, error_type="internal_error", code=500)
```

### Recommendation:
The error extraction logic is duplicated. `from_response` in `errors.py` could be reused by `_handle_error_response` in `client.py`.

---

## 3. Sync/Async Method Pair Duplication

### Location: `client.py`

Every synchronous method has a near-identical async counterpart:

| Sync Method | Async Method | Lines |
|-------------|--------------|-------|
| `_request` | `async def _request` | 135 vs 595 |
| `chat` | `async def chat` | 163 vs 623 |
| `stream_chat` | `async def stream_chat` | 199 vs 648 |
| `models` | `async def models` | 233 vs 687 |
| `providers` | `async def providers` | 245 vs 695 |
| `leaderboard` | `async def leaderboard` | 254 vs 700 |
| `balance` | `async def balance` | 274 vs 707 |
| `auth_status` | `async def auth_status` | 288 vs 714 |
| `health` | `async def health` | 299 vs 721 |
| `ready` | `async def ready` | 307 vs 725 |
| `gpu_listings` | `async def gpu_listings` | 321 vs 735 |
| `rent_gpu` | `async def rent_gpu` | 355 vs 747 |
| `gpu_rentals` | `async def gpu_rentals` | 371 vs 755 |
| `incentive_status` | `async def incentive_status` | 430 vs 762 |
| `incentive_models` | `async def incentive_models` | 439 vs 767 |
| `bridge_status` | `async def bridge_status` | 450 vs 774 |
| `bridge_invoices` | `async def bridge_invoices` | 459 vs 779 |
| `create_bridge_invoice` | `async def create_bridge_invoice` | 480 vs 784 |

### Recommendation:
Consider a base class with shared implementation, or use a decorator-based approach to generate sync/async pairs from a single implementation.

---

## 4. Generated Model Files Boilerplate (26 files)

### Location: `xergon-sdk-python/src/xergon/generated/models/*.py`

Each of the 26 generated model files contains identical boilerplate code for serialization/deserialization:

```python
def to_str(self) -> str:
    return pprint.pformat(self.model_dump(by_alias=True))

def to_json(self) -> str:
    return json.dumps(to_jsonable_python(self.to_dict()))

@classmethod
def from_json(cls, json_str: str) -> Optional[Self]:
    return cls.from_dict(json.loads(json_str))

def to_dict(self) -> Dict[str, Any]:
    excluded_fields: Set[str] = set([])
    _dict = self.model_dump(by_alias=True, exclude=excluded_fields, exclude_none=True)
    return _dict

@classmethod
def from_dict(cls, obj: Optional[Dict[str, Any]]) -> Optional[Self]:
    if obj is None:
        return None
    if not isinstance(obj, dict):
        return cls.model_validate(obj)
    _obj = cls.model_validate({...})
    return _obj
```

### File Count and Impact:
- 26 model files
- ~100 lines each = **~2,600 lines of duplicated boilerplate**
- All files follow the exact same pattern

### Recommendation:
This is auto-generated code, but the generator could be modified to produce a shared base class with these methods, eliminating the need to repeat them in every model file.

---

## 5. OpenAPI-Generated API Endpoint Variants

### Location: `xergon-sdk-python/src/xergon/generated/api/*.py`

Each API endpoint is generated in 3 variants:
- `method_name` - returns data directly
- `method_name_with_http_info` - returns `ApiResponse[T]` with headers/metadata
- `method_name_without_preload_content` - returns raw response

Example from `inference_api.py`:
- `create_chat_completion` (line 41)
- `create_chat_completion_with_http_info` (line 112)
- `create_chat_completion_without_preload_content` (line 183)

### Recommendation:
This is inherent to OpenAPI generator, but could be addressed at the generator level if modifying the generator template is feasible.

---

## 6. `model_validate` Call Pattern (32 occurrences in client.py)

### Pattern:
```python
return SomeModel.model_validate(data)           # single item
return [Model.model_validate(p) for p in data]  # list of items
```

This pattern appears 32 times in `client.py` and is repeated in both sync and async versions.

### Recommendation:
Create wrapper methods or use a generic deserialization helper to reduce repetition.

---

## Summary Statistics

| Category | Files Affected | Estimated Duplicated Lines |
|----------|---------------|---------------------------|
| SSE chunk parsing | 1 | ~40 |
| Error JSON parsing | 2 | ~30 |
| Sync/async pairs | 1 | ~400 |
| Generated model boilerplate | 26 | ~2,600 |
| API endpoint variants | 5 | ~2,000+ (inherent to generator) |
| **Total** | **35+** | **~5,000+** |

---

## Files Analyzed

- `/home/n1ur0x/xergon-network/xergon-sdk-python/src/xergon/client.py` (802 lines)
- `/home/n1ur0x/xergon-network/xergon-sdk-python/src/xergon/streaming.py` (181 lines)
- `/home/n1ur0x/xergon-network/xergon-sdk-python/src/xergon/errors.py` (119 lines)
- `/home/n1ur0x/xergon-network/xergon-sdk-python/src/xergon/types.py` (242 lines)
- `/home/n1ur0x/xergon-network/xergon-sdk-python/src/xergon/generated/models/*.py` (26 files, ~2,466 lines total)
- `/home/n1ur0x/xergon-network/xergon-sdk-python/src/xergon/generated/api/*.py` (5 API files)
- `/home/n1ur0x/xergon-network/xergon-sdk-python/src/xergon/generated/api_client.py` (808 lines)
- `/home/n1ur0x/xergon-network/xergon-sdk-python/src/xergon/generated/rest.py` (263 lines)
