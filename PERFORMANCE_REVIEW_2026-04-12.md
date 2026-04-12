# Xergon Network Performance & Optimization Review
**Branch:** `feature/wiring-complete-2026-04-11`  
**Date:** 2026-04-12  
**Scope:** xergon-relay (Rust/Axum), xergon-agent (Rust), xergon-marketplace (Next.js 15)

---

## Executive Summary

The codebase shows a well-structured architecture with modern Rust/Next.js patterns. However, several critical performance bottlenecks were identified:

1. **Database contention** - Single SQLite connection serialized via Mutex
2. **N+1 query patterns** in settlement operations
3. **Missing async boundaries** - Blocking operations in async contexts
4. **Inefficient rate limiting** - O(n) cleanup per request
5. **No caching layer** - Repeated database queries
6. **Memory leaks** - Rate limiter doesn't evict old entries properly

---

## 1. xergon-relay Performance Issues

### 1.1 Database Query Optimization (CRITICAL)

**File:** `xergon-relay/src/settlement.rs`

#### Issue 1: Single-Threaded SQLite Contention
```rust
pub struct SettlementManager {
    conn: Arc<Mutex<Connection>>,  // SINGLE connection, serialized access
}
```

**Problem:** All database operations serialize through a single Mutex, creating a bottleneck under concurrent load. Each request must wait for the lock.

**Impact:** 
- Max throughput limited to SQLite's single-writer performance (~100-500 req/s)
- Latency spikes under concurrent settlement/balance checks

**Recommendation:**
```rust
// Option A: Connection pool (recommended)
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;

pub struct SettlementManager {
    pool: Pool<SqliteConnectionManager>,
}

// Option B: WAL mode + multiple readers
// In config.toml: PRAGMA journal_mode = WAL;
// This allows concurrent reads while serializing writes
```

#### Issue 2: N+1 Query Pattern in `get_pending_proofs`
```rust
// Lines 173-195: Iterates proofs one-by-one without batching
pub async fn get_pending_proofs(&self, api_key: &str, limit: usize) -> Result<Vec<UsageProof>> {
    let mut stmt = conn.prepare("SELECT ... FROM pending_usage WHERE api_key = ? AND settled = 0 ...")?;
    let proofs = stmt.query_map(params![api_key, limit], |row| {
        Ok(UsageProof { ... })  // Row-by-row mapping
    })?;
    Ok(proofs.collect::<Result<Vec<_>, _>>()?)
}
```

**Problem:** While the query itself is efficient, the row-by-row mapping in `query_map` can be optimized. More critically, if this is called in a loop elsewhere (N+1), it's a bottleneck.

**Recommendation:**
```rust
// Use query_and_then for better error handling
let proofs = stmt.query_and_then(params![api_key, limit], |row| {
    Ok(UsageProof {
        provider_id: api_key.to_string(),
        tokens_input: row.get(0)?,
        tokens_output: row.get(1)?,
        model_used: row.get(3).ok(),
        timestamp: row.get(2)?,
        inference_id: None,
    })
})?
.collect::<Result<Vec<_>, _>>()?;
```

#### Issue 3: Missing Indexes for Common Queries
```sql
-- Current indexes (lines 67-75)
CREATE INDEX IF NOT EXISTS idx_pending_api_key ON pending_usage(api_key);
CREATE INDEX IF NOT EXISTS idx_pending_settled ON pending_usage(settled);
```

**Missing:** Composite index for the most common query pattern:
```sql
-- ADD: Composite index for the WHERE clause in get_pending_proofs
CREATE INDEX IF NOT EXISTS idx_pending_api_key_settled ON pending_usage(api_key, settled, timestamp);
```

### 1.2 Async/Await & Blocking Operations

#### Issue 4: Blocking SQLite in Async Context
```rust
// Lines 83-120: All DB operations use .lock().await then blocking rusqlite calls
pub async fn get_or_create_balance(&self, api_key: &str, ergo_address: &str) -> Result<UserBalance> {
    let conn = self.conn.lock().await;  // Async lock
    let mut stmt = conn.prepare(...)?;   // BLOCKING rusqlite call
    // ...
}
```

**Problem:** rusqlite operations are blocking but wrapped in async. Under load, this blocks the Tokio runtime threads.

**Recommendation:**
```rust
// Option A: Use tokio::task::spawn_blocking for DB operations
pub async fn get_or_create_balance(&self, api_key: &str, ergo_address: &str) -> Result<UserBalance> {
    let conn = self.conn.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = conn.blocking_lock();
        // ... blocking DB operations
    })
    .await?
}

// Option B: Use async-sqlite (if available) or deadpool-sqlite
```

### 1.3 Rate Limiting Efficiency

**File:** `xergon-relay/src/auth.rs` (Lines 118-163)

#### Issue 5: O(n) Cleanup Per Request
```rust
pub fn check_limit(&mut self, api_key: &str, limit: usize) -> bool {
    let now = Instant::now();
    let requests = self.requests.entry(api_key.to_string()).or_insert_with(Vec::new());
    
    // O(n) cleanup EVERY request
    requests.retain(|&timestamp| now.duration_since(timestamp) < self.window);
    
    if requests.len() < limit {
        requests.push(now);
        true
    } else {
        false
    }
}
```

**Problem:** Every request triggers a full cleanup of expired timestamps. Under high load with many API keys, this becomes O(n * m) where n = requests, m = API keys.

**Impact:**
- Memory grows indefinitely (old keys never removed)
- Cleanup cost increases linearly with traffic

**Recommendation:**
```rust
use std::collections::VecDeque;

pub struct RateLimiter {
    requests: HashMap<String, (usize, VecDeque<Instant>)>,  // (count, timestamps)
    window: Duration,
}

impl RateLimiter {
    pub fn check_limit(&mut self, api_key: &str, limit: usize) -> bool {
        let now = Instant::now();
        let entry = self.requests.entry(api_key.to_string()).or_insert((0, VecDeque::new()));
        let (_, timestamps) = &mut *entry;
        
        // Lazy cleanup: only remove old entries when they're at the front
        while timestamps.front().map_or(false, |&t| now.duration_since(t) >= self.window) {
            timestamps.pop_front();
        }
        
        if timestamps.len() < limit {
            timestamps.push_back(now);
            true
        } else {
            false
        }
    }
}
```

### 1.4 Memory Efficiency

#### Issue 6: Rate Limiter Memory Leak
```rust
// Lines 123-124: HashMap grows indefinitely
pub struct RateLimiter {
    requests: HashMap<String, Vec<Instant>>,  // Never evicted
    window: Duration,
}
```

**Problem:** Old API keys are never removed from the HashMap. Even if no requests come from a key for hours, it stays in memory.

**Recommendation:** Add periodic cleanup or use a TTL-based map like `dashmap` with expiry.

### 1.5 Provider Routing Inefficiency

**File:** `xergon-relay/src/handlers.rs` (Lines 156-166)

#### Issue 7: Suboptimal Provider Selection
```rust
// Lines 156-166: Always picks first provider
let provider_id = state.providers.keys().next().cloned();
```

**Problem:** No load balancing, no health-aware routing. All traffic goes to one provider.

**Recommendation:**
```rust
// Implement weighted round-robin based on pown_score and capacity
async fn select_provider(state: &AppState) -> Option<String> {
    let registry = state.registry.read().await;
    let healthy_providers: Vec<_> = registry
        .list_providers()
        .iter()
        .filter(|p| p.health_status == HealthStatus::Healthy)
        .collect();
    
    if healthy_providers.is_empty() {
        return None;
    }
    
    // Weight by pown_score
    let total_score: f32 = healthy_providers.iter()
        .map(|p| p.pown_score.unwrap_or(1.0))
        .sum();
    
    let mut rand_score = fastrand::f32() * total_score;
    for provider in healthy_providers {
        rand_score -= provider.pown_score.unwrap_or(1.0);
        if rand_score <= 0.0 {
            return Some(provider.provider_id.clone());
        }
    }
    Some(healthy_providers.last().unwrap().provider_id.clone())
}
```

---

## 2. xergon-agent Performance Issues

### 2.1 Settlement Engine

**File:** `xergon-agent/src/settlement/mod.rs`

#### Issue 8: Synchronous File I/O in Async Context
```rust
// Lines 217-228: Blocking file operations
pub async fn init(&self) -> Result<()> {
    let ledger_path = self.ledger_path();
    let mut ledger = self.ledger.write().await;
    *ledger = SettlementLedger::load(&ledger_path).await?;  // Async but may block
    // ...
}
```

**Problem:** File I/O operations (load/save) may block the async runtime if not properly offloaded.

**Recommendation:**
```rust
use tokio::fs;

pub async fn init(&self) -> Result<()> {
    let ledger_path = self.ledger_path();
    let mut ledger = self.ledger.write().await;
    
    // Use tokio::fs for true async file I/O
    let content = fs::read_to_string(&ledger_path).await?;
    *ledger = serde_json::from_str(&content)?;
    
    Ok(())
}
```

#### Issue 9: Inefficient Batch Settlement
```rust
// xergon-agent/src/settlement/batch.rs Lines 115-200
pub async fn flush(&self) -> anyhow::Result<BatchSettlementResult> {
    let payments: Vec<PendingPayment> = {
        let mut pending = self.pending.lock().await;
        std::mem::take(&mut *pending)
    };
    
    // Group by provider
    let mut provider_totals: HashMap<String, u64> = HashMap::new();
    for payment in &payments {
        *provider_totals.entry(payment.provider_address.clone()).or_default() += payment.amount;
    }
    
    // Sequential sending - could be parallelized
    for (provider_addr, total_nanoerg) in provider_totals {
        self.tx_service.send_payment(&provider_addr, total_nanoerg).await?;
    }
    // ...
}
```

**Problem:** Payments are sent sequentially. With 100 providers, this takes 100x the latency of a single payment.

**Recommendation:**
```rust
use futures::future::join_all;

// Parallel payment sending with error handling
let send_futures: Vec<_> = provider_totals
    .into_iter()
    .map(|(addr, amount)| self.tx_service.send_payment(&addr, amount))
    .collect();

let results = join_all(send_futures).await;
let success_count = results.iter().filter(|r| r.is_ok()).count();
```

### 2.2 PownScore & Peer Discovery

**File:** `xergon-agent/src/pown.rs` (Not found - may need investigation)

**Note:** The pown.rs and peer_discovery.rs files were not found in the expected locations. The main.rs references these modules but they may be in different locations or need to be created. This is a potential gap in the codebase.

---

## 3. Authentication Overhead

**File:** `xergon-relay/src/auth.rs`

### Issue 10: HMAC Signature Computation on Every Request
```rust
// Lines 83-101: Signature verification on every settlement request
pub fn verify_signature(&self, api_key: &str, payload: &str, signature: &str) -> Result<bool> {
    let key = self.api_keys.get(api_key).ok_or("Invalid API key")?;
    
    let mut mac = HmacSha256::new_from_slice(key.secret.as_bytes())?;
    mac.update(payload.as_bytes());
    let result = mac.finalize();
    let computed_signature = hex::encode(result.into_bytes());
    
    Ok(const_time_eq(&computed_signature, signature))
}
```

**Problem:** HMAC-SHA256 is computationally expensive. For high-frequency settlement batches, this adds up.

**Recommendation:**
- Cache computed HMAC keys (already done via `api_keys` HashMap)
- Consider using a faster hash like BLAKE3 for internal signatures
- Batch signature verification where possible

---

## 4. Database Schema Optimization

### Current Schema (settlement.rs Lines 38-75)

```sql
CREATE TABLE user_balances (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    api_key TEXT UNIQUE NOT NULL,
    ergo_address TEXT NOT NULL,
    balance_erg REAL NOT NULL DEFAULT 0.0,
    used_tokens_input INTEGER NOT NULL DEFAULT 0,
    used_tokens_output INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE pending_usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    api_key TEXT NOT NULL,
    tokens_input INTEGER NOT NULL,
    tokens_output INTEGER NOT NULL,
    model TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    settled INTEGER NOT NULL DEFAULT 0,
    settled_at INTEGER,
    transaction_id TEXT
);
```

### Recommended Optimizations

```sql
-- 1. Add composite index for common query pattern
CREATE INDEX IF NOT EXISTS idx_pending_usage_lookup 
ON pending_usage(api_key, settled, timestamp ASC);

-- 2. Add index for settlement summary query
CREATE INDEX IF NOT EXISTS idx_pending_usage_settled_summary 
ON pending_usage(api_key, settled);

-- 3. Enable WAL mode for concurrent reads
PRAGMA journal_mode = WAL;

-- 4. Increase cache size (32MB)
PRAGMA cache_size = -32000;

-- 5. Enable synchronous for durability (or NORMAL for performance)
PRAGMA synchronous = NORMAL;

-- 6. Optimize temp store
PRAGMA temp_store = MEMORY;
```

---

## 5. Caching Strategy Recommendations

### 5.1 In-Memory Cache Layer

```rust
use moka::future::Cache;

pub struct CachedSettlementManager {
    inner: SettlementManager,
    balance_cache: Cache<String, UserBalance>,  // TTL-based
}

impl CachedSettlementManager {
    pub async fn get_balance(&self, api_key: &str) -> Result<UserBalance> {
        // Try cache first
        if let Some(cached) = self.balance_cache.get(api_key).await {
            return Ok(cached);
        }
        
        // Cache miss - fetch from DB
        let balance = self.inner.get_or_create_balance(api_key, "").await?;
        self.balance_cache.insert(api_key.to_string(), balance.clone()).await;
        Ok(balance)
    }
}
```

**Recommended Cache Sizes:**
- API key lookup: 10,000 entries, 5min TTL
- Provider registry: 1,000 entries, 1min TTL
- Settlement summary: 1,000 entries, 30s TTL

### 5.2 Response Caching (Axum Middleware)

```rust
// For GET /providers endpoint
use tower_http::cache::CacheLayer;

let app = Router::new()
    .route("/providers", get(list_providers))
    .layer(CacheLayer::new(
        Duration::from_secs(30),  // 30s cache for provider list
    ));
```

---

## 6. Frontend (Next.js 15) Performance

### 6.1 Potential Issues (Based on package.json)

1. **No SWC/turbopack optimization visible** - Ensure `next.config.js` has:
   ```js
   module.exports = {
     experimental: {
       turbo: {
         rules: {
           '*.svg': {
             loaders: ['@svgr/webpack'],
             as: '*.js',
           },
         },
       },
     },
   }
   ```

2. **Zustand for state management** - Good choice, but ensure:
   - Use `shallow` comparison for selectors
   - Avoid storing large objects in state

3. **No visible CDN/edge caching** - Consider:
   - Vercel Edge Functions for API routes
   - Static generation for provider listings

---

## 7. CPU Utilization Optimization

### 7.1 Thread Pool Configuration

**Current:** Default Tokio runtime (auto-detected cores)

**Recommendation for high-throughput:**
```rust
#[tokio::main]
async fn main() {
    let worker_threads = num_cpus::get().max(4);
    
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .max_blocking_threads(worker_threads * 2)  // For DB operations
        .enable_all()
        .build()
        .unwrap()
        .run(run_server)
        .await;
}
```

### 7.2 Connection Pooling

```rust
// For HTTP client (reqwest)
let client = reqwest::Client::builder()
    .connection_verbose(true)
    .pool_idle_timeout(Duration::from_secs(30))
    .pool_max_idle_per_host(50)
    .timeout(Duration::from_secs(30))
    .build()?;
```

---

## 8. Rate Limiting Efficiency Improvements

### Current Implementation Issues:
1. **No sliding window** - Fixed 60s window can be bursty
2. **No tiered rate limiting** - All keys get same treatment
3. **No rate limit headers** - Clients can't adapt

### Recommended: Token Bucket Algorithm

```rust
use governor::{Guage, Quota, RateLimiter as GovernorLimiter};
use governor::middleware::StateMiddleware;
use std::num::NonZeroU32;

pub struct TieredRateLimiter {
    free: GovernorLimiter<NonZeroU32, std::time::Instant, StateMiddleware>,
    premium: GovernorLimiter<NonZeroU32, std::time::Instant, StateMiddleware>,
    enterprise: GovernorLimiter<NonZeroU32, std::time::Instant, StateMiddleware>,
}

impl TieredRateLimiter {
    pub fn new() -> Self {
        Self {
            free: GovernorLimiter::new(
                Quota::per_minute(NonZeroU32::new(100).unwrap()),
            ),
            premium: GovernorLimiter::new(
                Quota::per_minute(NonZeroU32::new(1000).unwrap()),
            ),
            enterprise: GovernorLimiter::new(
                Quota::per_minute(NonZeroU32::new(10000).unwrap()),
            ),
        }
    }
    
    pub fn check(&self, tier: ApiTier, key: &str) -> bool {
        match tier {
            ApiTier::Free => self.free.check_key(key).is_ok(),
            ApiTier::Premium => self.premium.check_key(key).is_ok(),
            ApiTier::Enterprise => self.enterprise.check_key(key).is_ok(),
        }
    }
}
```

---

## 9. Memory Efficiency Recommendations

### 9.1 Use DashMap for Concurrent HashMaps

Replace `std::sync::Mutex<HashMap>` with `dashmap::DashMap`:

```rust
use dashmap::DashMap;

pub struct AuthManager {
    api_keys: DashMap<String, ApiKey>,  // Sharded, no global lock
}
```

**Benefits:**
- O(1) average case vs O(n) for Mutex
- No single point of contention
- Built-in sharding for parallelism

### 9.2 Avoid Cloning Large Structures

```rust
// Current: Cloning entire Provider on every request
let provider = state.providers.get(&provider_id).unwrap();
let response = provider.chat_completions(request).await?;

// Better: Use Arc<Provider>
#[derive(Clone)]
pub struct Provider {
    inner: Arc<ProviderInner>,  // Clone the Arc, not the struct
}
```

---

## 10. Concurrent Request Handling

### Current: Axum with Tokio

**Recommendations:**

1. **Enable request body limits:**
   ```rust
   use tower_http::limit::RequestBodyLimitLayer;
   
   let app = Router::new()
       .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024)); // 10MB
   ```

2. **Add concurrency limits per route:**
   ```rust
   use tower::limit::ConcurrencyLimitLayer;
   
   let app = Router::new()
       .route("/v1/chat/completions", post(chat_completions))
       .layer(ConcurrencyLimitLayer::new(100)); // Max 100 concurrent
   ```

3. **Enable HTTP/2 for keep-alive:**
   ```rust
   let listener = TcpListener::bind(addr).await?;
   serve(listener, app).await?;
   ```

---

## 11. Summary of Critical Fixes (Priority Order)

| Priority | Issue | File | Effort | Impact |
|----------|-------|------|--------|--------|
| **P0** | SQLite connection pooling | `settlement.rs` | 2h | High |
| **P0** | Add composite DB indexes | Migration | 30min | High |
| **P1** | Fix rate limiter memory leak | `auth.rs` | 1h | Medium |
| **P1** | Parallelize batch settlement | `batch.rs` | 2h | Medium |
| **P2** | Implement provider load balancing | `handlers.rs` | 3h | Medium |
| **P2** | Add caching layer | New module | 4h | High |
| **P3** | WAL mode + PRAGMA tuning | Config | 30min | Low |
| **P3** | Token bucket rate limiting | `auth.rs` | 2h | Medium |

---

## 12. Files Modified/Created

**None created/modified in this review** - All recommendations are for future implementation.

---

## 13. Testing Recommendations

1. **Load testing:** Use `wrk` or `hey` to simulate 1000 concurrent requests
2. **Database profiling:** Use `sqlite3` EXPLAIN QUERY PLAN to verify indexes
3. **Memory profiling:** Use `tokio-console` to track task allocations
4. **Benchmark:** Add `criterion` benchmarks for hot paths

---

## 14. Monitoring Recommendations

1. **Add metrics:** Prometheus endpoint for:
   - Request latency (p50, p95, p99)
   - Database query time
   - Rate limit hits/misses
   - Settlement batch size

2. **Distributed tracing:** OpenTelemetry integration for request flows

---

**End of Review**
