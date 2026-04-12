# Rate Limiting Implementation

## Overview
Xergon implements tiered rate limiting to prevent DoS attacks and ensure fair resource usage.

## Tiers
1. **Free Tier**: 100 requests/minute
2. **Premium Tier**: 1,000 requests/minute
3. **Enterprise Tier**: 10,000 requests/minute

## Implementation
- Per-API-key tracking
- Sliding window algorithm
- Persistent state in settlement DB

## Future Improvements
- [ ] Per-IP rate limiting
- [ ] Distributed rate limiting across nodes
- [ ] Dynamic rate adjustment based on load
