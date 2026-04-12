# Xergon API Reference

## Base URL
```
https://relay.xergon.network
```

## Authentication
All endpoints require `X-API-Key` header.

## Endpoints

### POST /v1/chat/completions
Inference request endpoint.

**Request:**
```json
{
  "model": "qwen-3.5-122b",
  "messages": [{"role": "user", "content": "Hello"}]
}
```

**Response:**
```json
{
  "id": "chat-123",
  "choices": [{"message": {"content": "Hi!"}}]
}
```

### GET /providers
List available providers.

### POST /register
Register as a provider.

### POST /heartbeat
Provider health check.
