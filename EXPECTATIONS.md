# Xergon Network - Product Expectations

**Version:** 1.0.0  
**Last Updated:** 2026-04-12  
**Status:** Draft

---

## 🎯 Mission Statement

Xergon Network is a decentralized GPU inference marketplace built on the Ergo blockchain, enabling:
- **Providers** to monetize GPU resources through Proof-of-Work (PoNW) reputation system
- **Consumers** to access AI/ML inference at competitive prices with on-chain settlement
- **Protocol** to ensure trustless, verifiable, and secure transactions via Ergo smart contracts

---

## 📊 System Capabilities

### Current State (Testnet)

| Component | Status | Capability |
|-----------|--------|------------|
| **Smart Contracts** | ✅ Deployed (testnet) | 5 contracts: treasury, provider_box, user_staking, usage_proof, provider_slashing |
| **Relay Server** | ✅ Running (port 9090) | API endpoints: registration, heartbeat, chat/completions, settlement |
| **Agent** | ⚠️ Development | PoNW scoring, settlement tracking (Ergo node integration) |
| **Marketplace UI** | ⚠️ Development | Next.js 15 frontend (Next.js 16.2.3 security update pending) |
| **SDK** | ⚠️ Development | TypeScript client (blake2b dependency issue) |

### Production Requirements (Not Met)

- [ ] Mainnet contract deployment (addresses must be replaced)
- [ ] 80%+ test coverage for critical modules
- [ ] Prometheus metrics & alerting
- [ ] Load testing validation (>1000 req/s target)
- [ ] Security audit completion (external firm)
- [ ] Multi-sig treasury implementation
- [ ] Circuit breaker auto-recovery

---

## 🔐 Security Model

### Authentication
- **Method:** HMAC-SHA256 signatures (HMAC-SHA256)
- **Header:** `X-API-Key` + `X-Signature`
- **Rate Limiting:** Tiered (Free: 100/min, Premium: 1000/min, Enterprise: 10000/min)
- **Circuit Breaker:** Fail-closed on repeated auth failures

### Smart Contract Security
- **Singleton NFT Pattern:** Provider identity via NFT (tokens(0)._1 == nftId)
- **Value Preservation:** All outputs must preserve NFT and script
- **Storage Rent:** 4-year expiry for usage proof boxes
- **Authorization:** SigmaProp proveDlog patterns

### Known Vulnerabilities
- ⚠️ **T-02:** Single-key treasury (centralization risk) - *Mitigation: Multi-sig planned*
- ⚠️ **Placeholder addresses:** Must be replaced before mainnet
- ⚠️ **No formal verification:** Contracts not formally verified

---

## 📈 Performance Expectations

### Current Benchmarks (Testnet)
- **Relay Throughput:** ~5,700 TPS (sequential), ~6,700 TPS (concurrent)
- **Ergo Node:** Height 281,503 (testnet), fully synced
- **Settlement:** SQLite-based, single-threaded (bottleneck)

### Production Targets
- **Throughput:** >10,000 req/s (with load balancer)
- **Latency:** <100ms p95 for API responses
- **Settlement:** Batch processing every 10 minutes
- **Uptime:** 99.9% SLA (requires HA deployment)

### Limitations
- **Database:** SQLite not suitable for production scale (need PostgreSQL)
- **Single-node:** No horizontal scaling currently
- **No caching:** Response caching layer not implemented

---

## 🏗️ Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        CLIENTS                               │
│  (Marketplace UI, SDK, Direct API)                           │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    RELAY SERVER (Axum)                       │
│  • Authentication (HMAC-SHA256)                             │
│  • Rate Limiting (Token bucket)                             │
│  • Provider Routing (Round-robin)                           │
│  • Settlement Tracking (SQLite)                             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    PROVIDER NETWORK                          │
│  • GPU Providers (Registered via NFT)                        │
│  • Inference Execution (External APIs)                       │
│  • Heartbeat Monitoring (30s interval)                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    ERGO BLOCKCHAIN                           │
│  • Smart Contracts (PoNW, Staking, Settlement)               │
│  • On-chain Settlement (Batched)                             │
│  • Reputation Tracking (PoNW Score)                          │
└─────────────────────────────────────────────────────────────┘
```

---

## 🚦 Operational Expectations

### Deployment Environments

| Environment | Purpose | Status |
|-------------|---------|--------|
| **Testnet** | Development & Testing | ✅ Active (height 281,503) |
| **Staging** | Pre-production validation | ❌ Not configured |
| **Production** | Mainnet deployment | ❌ Not ready |

### Monitoring Requirements
- **Current:** None (logs only)
- **Required:** Prometheus metrics, Grafana dashboards, alerting
- **Key Metrics:** Request rate, error rate, latency, settlement queue size

### Incident Response
- **Current:** Manual intervention
- **Required:** Automated circuit breaker, alerting, runbooks
- **RTO:** <1 hour (target), <24 hours (current)
- **RPO:** <5 minutes (target), N/A (current - no backups)

---

## 📋 Compliance & Governance

### Smart Contract Governance
- **Current:** Single-key deployer (centralized)
- **Target:** Multi-sig committee (3-of-5 threshold)
- **Upgrade Path:** Contract migration with NFT transfer

### Data Privacy
- **User Data:** API keys stored in-memory (not persisted)
- **Settlement Data:** SQLite database (not encrypted)
- **Compliance:** No GDPR/CCPA compliance measures

### Token Economics
- **Settlement:** ERG (native Ergo token)
- **Pricing:** Provider-defined (market-driven)
- **Fees:** Protocol fee TBD (currently 0%)

---

## ⚠️ Known Limitations

### Technical Debt
1. **Placeholder Documentation:** SMART_CONTRACTS.md, UTXO_GUIDE.md, etc.
2. **Compiler Warnings:** 31 warnings in xergon-relay
3. **Test Coverage:** <10% (only 1 test in relay)
4. **Build Artifacts:** 18GB total (target/ and node_modules/)

### Architectural Constraints
1. **Single-threaded Settlement:** SQLite mutex bottleneck
2. **No Provider Health Polling:** Manual health checks
3. **No Response Caching:** Every request hits provider
4. **Hardcoded Fees:** 0.001 ERG (not configurable)

### Security Gaps
1. **No Formal Verification:** Contracts not formally verified
2. **No Input Validation:** User-controlled data not sanitized
3. **No Audit Trail:** Settlements not logged to immutable store
4. **Key Management:** API keys in-memory (not HSM-backed)

---

## 🎯 Success Criteria

### Phase 1: Testnet Validation (Current)
- [x] Smart contracts compile and deploy
- [x] Relay server handles requests
- [x] Provider registration/heartbeat working
- [ ] Settlement flow end-to-end
- [ ] Load testing (>1000 req/s)

### Phase 2: Staging Preparation (Next)
- [ ] All documentation populated
- [ ] 80% test coverage
- [ ] Prometheus metrics configured
- [ ] Security audit completed
- [ ] Multi-sig treasury implemented

### Phase 3: Production Readiness (Future)
- [ ] Mainnet deployment
- [ ] Load balancer setup
- [ ] Database migration (PostgreSQL)
- [ ] CI/CD pipeline
- [ ] Disaster recovery procedures
- [ ] 99.9% uptime SLA

---

## 📞 Support & Escalation

### Development Team
- **Repository:** https://github.com/Degens-World/Xergon-Network
- **Issues:** https://github.com/Degens-World/Xergon-Network/issues
- **Discord:** [TBD]

### Emergency Contacts
- **Security Issues:** security@xergon.network (TBD)
- **Incident Response:** [TBD]

---

## 🔄 Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-04-12 | Initial draft |

---

**Disclaimer:** This document describes expectations and capabilities at a specific point in time. Features, performance, and security characteristics may change without notice. Production deployment should only occur after formal security audit and stakeholder approval.
