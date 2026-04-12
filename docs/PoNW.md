# Proof of Work (PoNW) - Xergon Network

## Overview
Proof of Work (PoNW) is Xergon's consensus mechanism for validating AI inference providers.

## How It Works
1. **Provider Registration**: Providers register their GPU capacity on-chain
2. **Inference Requests**: Users submit inference requests with ERG payment
3. **Proof Generation**: Providers generate cryptographic proof of completed work
4. **Verification**: Network verifies proofs and distributes rewards

## Scoring Mechanism
- **Base Score**: Based on GPU capacity and uptime
- **Performance Bonus**: Faster inference times = higher scores
- **Reliability Factor**: Uptime and successful completions
- **Penalty System**: Failed verifications reduce score

## Implementation
See `xergon-agent/src/pown.rs` for the scoring algorithm.

## Security
- All proofs are cryptographically verified
- No double-spending possible
- Sybil attack resistant via bond requirements
