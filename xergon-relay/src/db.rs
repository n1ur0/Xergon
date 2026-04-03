//! Database layer — SQLite-backed storage for users, sessions, and credit transactions.
//!
//! Tables:
//!   users             — id, email, name, password_hash, tier, created_at, updated_at
//!   credit_transactions — id, user_id, amount_usd, balance_after, kind, description, stripe_payment_id, created_at
//!   usage_records     — id, user_id, provider_id, model, prompt_tokens, completion_tokens, cost_credits, cost_usd, tier, ip_address, created_at, status
//!   api_keys          — id, user_id, key_hash, key_prefix, name, last_used_at, expires_at, created_at, revoked_at

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use tracing::info;
use uuid::Uuid;

/// Thread-safe wrapper around SQLite connection
pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub tier: String, // "free" | "pro"
    pub auto_replenish: bool,
    pub replenish_pack_id: Option<String>,
    pub replenish_threshold_usd: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditTransaction {
    pub id: String,
    pub user_id: String,
    pub amount_usd: f64,
    pub balance_after: f64,
    pub kind: String, // "purchase" | "deduction" | "bonus"
    pub description: String,
    pub stripe_payment_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A persisted usage record in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: String,
    pub user_id: Option<String>,
    pub provider_id: String,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cost_credits: f64,
    pub cost_usd: f64,
    pub tier: String,
    pub ip_address: Option<String>,
    pub created_at: String,
    pub status: String,
}

/// Provider-configurable model pricing entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub id: String,
    pub provider_id: String,
    pub model_id: String,
    pub price_per_1k_prompt: f64,
    pub price_per_1k_completion: f64,
    pub currency: String,
    pub updated_at: String,
}

/// Full API key row (includes key_hash — never exposed via API).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRow {
    pub id: String,
    pub user_id: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub name: String,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

/// API key info for listing (excludes key_hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyInfo {
    pub id: String,
    pub key_prefix: String,
    pub name: String,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

impl From<ApiKeyRow> for ApiKeyInfo {
    fn from(row: ApiKeyRow) -> Self {
        Self {
            id: row.id,
            key_prefix: row.key_prefix,
            name: row.name,
            last_used_at: row.last_used_at,
            expires_at: row.expires_at,
            created_at: row.created_at,
            revoked_at: row.revoked_at,
        }
    }
}

/// User summary for admin listing (excludes password_hash and other sensitive fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummary {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub tier: String,
    pub credits: f64,
    pub created_at: DateTime<Utc>,
}

/// Platform-wide statistics for the admin dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformStats {
    pub total_users: i64,
    pub total_providers: i64,
    pub total_requests: i64,
    pub total_credits_in_circulation: f64,
    pub total_revenue: f64,
}

/// Aggregated usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    pub total_requests: i64,
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub total_cost_credits: f64,
    pub total_cost_usd: f64,
    pub by_model: Vec<ModelStats>,
}

/// Per-model usage breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStats {
    pub model: String,
    pub requests: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cost_credits: f64,
    pub cost_usd: f64,
}

impl Db {
    /// Open (or create) the database file and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database at {}", path.display()))?;

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .context("Failed to set SQLite pragmas")?;

        let db = Self {
            conn: Mutex::new(conn),
        };

        db.run_migrations()?;

        info!(path = %path.display(), "Database opened and migrated");
        Ok(db)
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }

    fn run_migrations(&self) -> Result<()> {
        let conn = self.conn();

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS users (
                id              TEXT PRIMARY KEY,
                email           TEXT NOT NULL UNIQUE,
                name            TEXT,
                password_hash   TEXT NOT NULL,
                tier            TEXT NOT NULL DEFAULT 'free',
                auto_replenish  INTEGER NOT NULL DEFAULT 0,
                replenish_pack_id TEXT,
                replenish_threshold_usd REAL NOT NULL DEFAULT 1.0,
                created_at      TEXT NOT NULL,
                updated_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS credit_transactions (
                id                  TEXT PRIMARY KEY,
                user_id             TEXT NOT NULL REFERENCES users(id),
                amount_usd          REAL NOT NULL,
                balance_after       REAL NOT NULL,
                kind                TEXT NOT NULL,
                description         TEXT NOT NULL DEFAULT '',
                stripe_payment_id   TEXT,
                created_at          TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_credit_tx_user ON credit_transactions(user_id);
            CREATE INDEX IF NOT EXISTS idx_credit_tx_stripe ON credit_transactions(stripe_payment_id);

            CREATE TABLE IF NOT EXISTS processed_webhook_events (
                event_id     TEXT PRIMARY KEY,
                event_type   TEXT NOT NULL,
                processed_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS password_reset_tokens (
                token       TEXT PRIMARY KEY,
                user_id     TEXT NOT NULL REFERENCES users(id),
                expires_at  TEXT NOT NULL,
                used        INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS usage_records (
                id                TEXT PRIMARY KEY,
                user_id           TEXT,
                provider_id       TEXT NOT NULL,
                model             TEXT NOT NULL,
                prompt_tokens     INTEGER NOT NULL DEFAULT 0,
                completion_tokens INTEGER NOT NULL DEFAULT 0,
                cost_credits      REAL NOT NULL,
                cost_usd          REAL NOT NULL DEFAULT 0,
                tier              TEXT NOT NULL,
                ip_address        TEXT,
                created_at        TEXT NOT NULL DEFAULT (datetime('now')),
                status            TEXT NOT NULL DEFAULT 'success'
            );

            CREATE INDEX IF NOT EXISTS idx_usage_user ON usage_records(user_id);
            CREATE INDEX IF NOT EXISTS idx_usage_created ON usage_records(created_at);
            CREATE INDEX IF NOT EXISTS idx_usage_model ON usage_records(model);

            CREATE TABLE IF NOT EXISTS model_pricing (
                id                    TEXT PRIMARY KEY,
                provider_id           TEXT NOT NULL,
                model_id              TEXT NOT NULL,
                price_per_1k_prompt   REAL NOT NULL DEFAULT 0,
                price_per_1k_completion REAL NOT NULL DEFAULT 0.002,
                currency              TEXT NOT NULL DEFAULT 'credits',
                updated_at            TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(provider_id, model_id)
            );

            CREATE INDEX IF NOT EXISTS idx_model_pricing_provider ON model_pricing(provider_id);
            CREATE INDEX IF NOT EXISTS idx_model_pricing_model ON model_pricing(model_id);

            CREATE TABLE IF NOT EXISTS api_keys (
                id          TEXT PRIMARY KEY,
                user_id     TEXT NOT NULL REFERENCES users(id),
                key_hash    TEXT NOT NULL,
                key_prefix  TEXT NOT NULL,
                name        TEXT NOT NULL,
                last_used_at TEXT,
                expires_at  TEXT,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                revoked_at  TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash);
            CREATE INDEX IF NOT EXISTS idx_api_keys_user ON api_keys(user_id);
            ",
        )
        .context("Failed to run database migrations")?;

        // Add auto-replenish columns if they don't exist (migration for existing DBs)
        // NOTE: Use `conn` (already locked above) — calling self.conn() again
        // would deadlock since std::sync::Mutex is not reentrant.
        conn.execute_batch(
            "ALTER TABLE users ADD COLUMN auto_replenish INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE users ADD COLUMN replenish_pack_id TEXT;
             ALTER TABLE users ADD COLUMN replenish_threshold_usd REAL NOT NULL DEFAULT 1.0;",
        ).ok(); // Ignore errors if columns already exist

        // Add ergo_address column for wallet linking
        conn.execute_batch("ALTER TABLE users ADD COLUMN ergo_address TEXT;")
            .ok(); // Ignore errors if column already exists

        // Add last_replenish_at column for auto-replenish idempotency guard
        conn.execute_batch("ALTER TABLE users ADD COLUMN last_replenish_at TEXT;")
            .ok(); // Ignore errors if column already exists

        Ok(())
    }

    // ── User CRUD ──

    pub fn create_user(
        &self,
        id: &str,
        email: &str,
        name: Option<&str>,
        password_hash: &str,
    ) -> Result<User> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn();
        conn.execute(
            "INSERT INTO users (id, email, name, password_hash, tier, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'free', ?5, ?6)",
            params![id, email, name, password_hash, now, now],
        )
        .with_context(|| format!("Failed to create user {}", email))?;

        Ok(User {
            id: id.to_string(),
            email: email.to_string(),
            name: name.map(|s| s.to_string()),
            password_hash: password_hash.to_string(),
            tier: "free".to_string(),
            auto_replenish: false,
            replenish_pack_id: None,
            replenish_threshold_usd: 1.0,
            created_at: now.parse().unwrap(),
            updated_at: now.parse().unwrap(),
        })
    }

    pub fn get_user_by_email(&self, email: &str) -> Result<Option<User>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT id, email, name, password_hash, tier, auto_replenish, replenish_pack_id, replenish_threshold_usd, created_at, updated_at FROM users WHERE email = ?1")
            .context("Failed to prepare user lookup")?;

        let user = stmt
            .query_row(params![email], |row| {
                Ok(User {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    name: row.get(2)?,
                    password_hash: row.get(3)?,
                    tier: row.get(4)?,
                    auto_replenish: row.get::<_, i32>(5)? != 0,
                    replenish_pack_id: row.get(6)?,
                    replenish_threshold_usd: row.get(7)?,
                    created_at: row.get::<_, String>(8)?.parse().unwrap(),
                    updated_at: row.get::<_, String>(9)?.parse().unwrap(),
                })
            })
            .ok();

        Ok(user)
    }

    pub fn get_user_by_id(&self, id: &str) -> Result<Option<User>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT id, email, name, password_hash, tier, auto_replenish, replenish_pack_id, replenish_threshold_usd, created_at, updated_at FROM users WHERE id = ?1")
            .context("Failed to prepare user lookup by id")?;

        let user = stmt
            .query_row(params![id], |row| {
                Ok(User {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    name: row.get(2)?,
                    password_hash: row.get(3)?,
                    tier: row.get(4)?,
                    auto_replenish: row.get::<_, i32>(5)? != 0,
                    replenish_pack_id: row.get(6)?,
                    replenish_threshold_usd: row.get(7)?,
                    created_at: row.get::<_, String>(8)?.parse().unwrap(),
                    updated_at: row.get::<_, String>(9)?.parse().unwrap(),
                })
            })
            .ok();

        Ok(user)
    }

    // ── Password Reset ──

    /// Create a password reset token for a user. Returns the raw token string.
    pub fn create_password_reset_token(&self, user_id: &str) -> Result<String> {
        let token = Uuid::new_v4().to_string();
        let expires_at = Utc::now() + chrono::Duration::hours(1);
        let conn = self.conn();
        conn.execute(
            "INSERT INTO password_reset_tokens (token, user_id, expires_at, used) VALUES (?1, ?2, ?3, 0)",
            params![token, user_id, expires_at.to_rfc3339()],
        )
        .with_context(|| format!("Failed to create password reset token for user {}", user_id))?;
        Ok(token)
    }

    /// Consume a password reset token: validates it's not expired/used, marks as used, returns user_id.
    pub fn consume_password_reset_token(&self, token: &str) -> Result<Option<String>> {
        let conn = self.conn();

        // Check token exists, not used, not expired
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT user_id, expires_at FROM password_reset_tokens WHERE token = ?1 AND used = 0",
                params![token],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let (user_id, expires_at) = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        // Check expiry
        let expires: DateTime<Utc> = expires_at.parse().context("Invalid expires_at format")?;
        if Utc::now() > expires {
            return Ok(None);
        }

        // Mark as used
        conn.execute(
            "UPDATE password_reset_tokens SET used = 1 WHERE token = ?1",
            params![token],
        )
        .context("Failed to mark reset token as used")?;

        Ok(Some(user_id))
    }

    /// Update a user's password hash.
    pub fn update_user_password(&self, user_id: &str, new_hash: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn();
        conn.execute(
            "UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE id = ?3",
            params![new_hash, now, user_id],
        )
        .with_context(|| format!("Failed to update password for user {}", user_id))?;
        Ok(())
    }

    /// Update a user's profile (name and/or email).
    pub fn update_user_profile(&self, user_id: &str, name: Option<&str>, email: Option<&str>) -> Result<User> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn();

        if let Some(email) = email {
            conn.execute(
                "UPDATE users SET email = ?1, updated_at = ?2 WHERE id = ?3",
                params![email, now, user_id],
            )
            .with_context(|| format!("Failed to update email for user {}", user_id))?;
        }

        if let Some(name) = name {
            conn.execute(
                "UPDATE users SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![name, now, user_id],
            )
            .with_context(|| format!("Failed to update name for user {}", user_id))?;
        }

        // Fetch and return updated user
        self.get_user_by_id(user_id)?
            .context("User not found after profile update")
    }

    // ── Credits ──

    /// Get current USD credit balance for a user.
    pub fn get_credit_balance(&self, user_id: &str) -> Result<f64> {
        let conn = self.conn();
        let balance: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(amount_usd), 0.0) FROM credit_transactions WHERE user_id = ?1",
                params![user_id],
                |row| row.get(0),
            )
            .context("Failed to get credit balance")?;
        Ok(balance)
    }

    /// Add a credit transaction (purchase, bonus, etc.)
    pub fn add_credits(
        &self,
        id: &str,
        user_id: &str,
        amount_usd: f64,
        kind: &str,
        description: &str,
        stripe_payment_id: Option<&str>,
    ) -> Result<CreditTransaction> {
        let balance_after = self.get_credit_balance(user_id)? + amount_usd;
        let now = Utc::now().to_rfc3339();
        let conn = self.conn();

        conn.execute(
            "INSERT INTO credit_transactions (id, user_id, amount_usd, balance_after, kind, description, stripe_payment_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, user_id, amount_usd, balance_after, kind, description, stripe_payment_id, now],
        )
        .context("Failed to add credit transaction")?;

        Ok(CreditTransaction {
            id: id.to_string(),
            user_id: user_id.to_string(),
            amount_usd,
            balance_after,
            kind: kind.to_string(),
            description: description.to_string(),
            stripe_payment_id: stripe_payment_id.map(|s| s.to_string()),
            created_at: now.parse().unwrap(),
        })
    }

    /// Deduct credits for an inference request atomically.
    /// Uses BEGIN IMMEDIATE to prevent race conditions.
    /// Returns the balance after deduction, or Err if insufficient funds.
    pub fn deduct_credits(
        &self,
        id: &str,
        user_id: &str,
        amount_usd: f64,
        description: &str,
    ) -> Result<f64> {
        let conn = self.conn();

        // Begin an immediate (write-locking) transaction
        conn.execute_batch("BEGIN IMMEDIATE")
            .context("Failed to begin immediate transaction for credit deduction")?;

        // Atomically check current balance within the locked transaction
        let current_balance: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(amount_usd), 0.0) FROM credit_transactions WHERE user_id = ?1",
                params![user_id],
                |row| row.get(0),
            )
            .context("Failed to read balance for deduction")?;

        if current_balance < amount_usd {
            conn.execute_batch("ROLLBACK").ok();
            anyhow::bail!(
                "Insufficient credits: balance ${:.4}, need ${:.4}",
                current_balance,
                amount_usd
            );
        }

        let new_balance = current_balance - amount_usd;

        // Insert the deduction transaction record (negative amount_usd)
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO credit_transactions (id, user_id, amount_usd, balance_after, kind, description, stripe_payment_id, created_at)
             VALUES (?1, ?2, ?3, ?4, 'deduction', ?5, NULL, ?6)",
            params![id, user_id, -amount_usd, new_balance, description, now],
        )
        .context("Failed to insert deduction transaction record")?;

        conn.execute_batch("COMMIT")
            .context("Failed to commit credit deduction transaction")?;

        Ok(new_balance)
    }

    /// Get recent credit transactions for a user.
    pub fn get_transactions(&self, user_id: &str, limit: u32) -> Result<Vec<CreditTransaction>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, user_id, amount_usd, balance_after, kind, description, stripe_payment_id, created_at
                 FROM credit_transactions WHERE user_id = ?1 ORDER BY created_at DESC LIMIT ?2",
            )
            .context("Failed to prepare transaction query")?;

        let rows = stmt
            .query_map(params![user_id, limit], |row| {
                Ok(CreditTransaction {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    amount_usd: row.get(2)?,
                    balance_after: row.get(3)?,
                    kind: row.get(4)?,
                    description: row.get(5)?,
                    stripe_payment_id: row.get(6)?,
                    created_at: row.get::<_, String>(7)?.parse().unwrap(),
                })
            })
            .context("Failed to query transactions")?;

        let mut txs = Vec::new();
        for row in rows {
            txs.push(row.context("Failed to read transaction row")?);
        }
        Ok(txs)
    }

    // ── Auto-replenish ──

    /// Update auto-replenish settings for a user.
    pub fn update_auto_replenish(
        &self,
        user_id: &str,
        enabled: bool,
        pack_id: Option<&str>,
        threshold_usd: f64,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn();
        conn.execute(
            "UPDATE users SET auto_replenish = ?1, replenish_pack_id = ?2, replenish_threshold_usd = ?3, updated_at = ?4 WHERE id = ?5",
            params![enabled as i32, pack_id, threshold_usd, now, user_id],
        )
        .with_context(|| format!("Failed to update auto-replenish for user {}", user_id))?;
        Ok(())
    }

    /// Get all users with auto-replenish enabled whose balance is below threshold.
    /// Filters out users who were replenished in the last 5 minutes (idempotency guard).
    /// Used by a periodic background job to trigger auto-replenish.
    pub fn get_users_needing_replenish(&self) -> Result<Vec<(User, f64)>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT u.id, u.email, u.name, u.password_hash, u.tier, u.auto_replenish, u.replenish_pack_id, u.replenish_threshold_usd, u.created_at, u.updated_at,
                        COALESCE(SUM(ct.amount_usd), 0.0) as balance
                 FROM users u
                 LEFT JOIN credit_transactions ct ON ct.user_id = u.id
                 WHERE u.auto_replenish = 1
                   AND u.replenish_pack_id IS NOT NULL
                   AND (u.last_replenish_at IS NULL OR u.last_replenish_at < datetime('now', '-5 minutes'))
                 GROUP BY u.id
                 HAVING balance < u.replenish_threshold_usd",
            )
            .context("Failed to query users needing replenish")?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    User {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        name: row.get(2)?,
                        password_hash: row.get(3)?,
                        tier: row.get(4)?,
                        auto_replenish: row.get::<_, i32>(5)? != 0,
                        replenish_pack_id: row.get(6)?,
                        replenish_threshold_usd: row.get(7)?,
                        created_at: row.get::<_, String>(8)?.parse().unwrap(),
                        updated_at: row.get::<_, String>(9)?.parse().unwrap(),
                    },
                    row.get::<_, f64>(10)?,
                ))
            })
            .context("Failed to query")?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.context("Failed to read replenish row")?);
        }
        Ok(result)
    }

    // ── Webhook idempotency ──

    /// Check if a Stripe webhook event has already been processed.
    pub fn is_event_processed(&self, event_id: &str) -> Result<bool> {
        let conn = self.conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM processed_webhook_events WHERE event_id = ?1",
                rusqlite::params![event_id],
                |row| row.get(0),
            )
            .context("Failed to check processed event")?;
        Ok(count > 0)
    }

    /// Mark a Stripe webhook event as processed (idempotent insert).
    pub fn mark_event_processed(&self, event_id: &str, event_type: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT OR IGNORE INTO processed_webhook_events (event_id, event_type, processed_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![event_id, event_type, chrono::Utc::now().to_rfc3339()],
        )
        .context("Failed to mark event processed")?;
        Ok(())
    }

    // ── Usage Analytics ──

    /// Insert a usage record after a successful inference.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_usage_record(
        &self,
        user_id: Option<&str>,
        provider_id: &str,
        model: &str,
        prompt_tokens: i64,
        completion_tokens: i64,
        cost_credits: f64,
        cost_usd: f64,
        tier: &str,
        hashed_ip: Option<&str>,
    ) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let conn = self.conn();
        conn.execute(
            "INSERT INTO usage_records (id, user_id, provider_id, model, prompt_tokens, completion_tokens, cost_credits, cost_usd, tier, ip_address)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, user_id, provider_id, model, prompt_tokens, completion_tokens, cost_credits, cost_usd, tier, hashed_ip],
        )
        .context("Failed to insert usage record")?;
        Ok(())
    }

    /// Get aggregated usage stats for a user (or all users if None).
    pub fn get_usage_stats(
        &self,
        user_id: Option<&str>,
        start_date: &str,
        end_date: &str,
    ) -> Result<UsageStats> {
        let conn = self.conn();

        // Build the WHERE clause based on whether we have a user_id
        let (where_clause, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match user_id {
            Some(uid) => (
                "WHERE user_id = ?1 AND created_at >= ?2 AND created_at <= ?3".to_string(),
                vec![
                    Box::new(uid.to_string()),
                    Box::new(start_date.to_string()),
                    Box::new(end_date.to_string()),
                ],
            ),
            None => (
                "WHERE created_at >= ?1 AND created_at <= ?2".to_string(),
                vec![
                    Box::new(start_date.to_string()),
                    Box::new(end_date.to_string()),
                ],
            ),
        };

        // Total aggregated stats
        let total_sql = format!(
            "SELECT COUNT(*) as total_requests,
                    COALESCE(SUM(prompt_tokens), 0) as total_prompt_tokens,
                    COALESCE(SUM(completion_tokens), 0) as total_completion_tokens,
                    COALESCE(SUM(cost_credits), 0) as total_cost_credits,
                    COALESCE(SUM(cost_usd), 0) as total_cost_usd
             FROM usage_records {}",
            where_clause
        );

        let mut stmt = conn.prepare(&total_sql).context("Failed to prepare usage stats query")?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let stats = stmt
            .query_row(params_refs.as_slice(), |row| {
                Ok(UsageStats {
                    total_requests: row.get(0)?,
                    total_prompt_tokens: row.get(1)?,
                    total_completion_tokens: row.get(2)?,
                    total_cost_credits: row.get(3)?,
                    total_cost_usd: row.get(4)?,
                    by_model: Vec::new(), // populated below
                })
            })
            .context("Failed to query usage stats")?;

        // Per-model breakdown
        let model_sql = format!(
            "SELECT model,
                    COUNT(*) as requests,
                    COALESCE(SUM(prompt_tokens), 0) as prompt_tokens,
                    COALESCE(SUM(completion_tokens), 0) as completion_tokens,
                    COALESCE(SUM(cost_credits), 0) as cost_credits,
                    COALESCE(SUM(cost_usd), 0) as cost_usd
             FROM usage_records {}
             GROUP BY model
             ORDER BY requests DESC",
            where_clause
        );

        let mut model_stmt = conn.prepare(&model_sql).context("Failed to prepare model stats query")?;
        let model_rows = model_stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(ModelStats {
                    model: row.get(0)?,
                    requests: row.get(1)?,
                    prompt_tokens: row.get(2)?,
                    completion_tokens: row.get(3)?,
                    cost_credits: row.get(4)?,
                    cost_usd: row.get(5)?,
                })
            })
            .context("Failed to query model stats")?;

        let mut by_model = Vec::new();
        for row in model_rows {
            by_model.push(row.context("Failed to read model stats row")?);
        }

        Ok(UsageStats {
            by_model,
            ..stats
        })
    }

    /// Get paginated usage history for a user (or all users if None).
    pub fn get_usage_history(
        &self,
        user_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<UsageRecord>> {
        let conn = self.conn();

        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match user_id {
            Some(uid) => (
                "SELECT id, user_id, provider_id, model, prompt_tokens, completion_tokens,
                        cost_credits, cost_usd, tier, ip_address, created_at, status
                 FROM usage_records
                 WHERE user_id = ?1
                 ORDER BY created_at DESC
                 LIMIT ?2 OFFSET ?3"
                    .to_string(),
                vec![
                    Box::new(uid.to_string()),
                    Box::new(limit),
                    Box::new(offset),
                ],
            ),
            None => (
                "SELECT id, user_id, provider_id, model, prompt_tokens, completion_tokens,
                        cost_credits, cost_usd, tier, ip_address, created_at, status
                 FROM usage_records
                 ORDER BY created_at DESC
                 LIMIT ?1 OFFSET ?2"
                    .to_string(),
                vec![Box::new(limit), Box::new(offset)],
            ),
        };

        let mut stmt = conn.prepare(&sql).context("Failed to prepare usage history query")?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(UsageRecord {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    provider_id: row.get(2)?,
                    model: row.get(3)?,
                    prompt_tokens: row.get(4)?,
                    completion_tokens: row.get(5)?,
                    cost_credits: row.get(6)?,
                    cost_usd: row.get(7)?,
                    tier: row.get(8)?,
                    ip_address: row.get(9)?,
                    created_at: row.get(10)?,
                    status: row.get(11)?,
                })
            })
            .context("Failed to query usage history")?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.context("Failed to read usage record row")?);
        }
        Ok(records)
    }

    /// Delete usage records older than N days. Returns the number of deleted rows.
    #[allow(dead_code)] // TODO: will be used for admin cleanup endpoint
    pub fn cleanup_old_usage(&self, days: i64) -> Result<usize> {
        let conn = self.conn();
        let cutoff = Utc::now() - chrono::Duration::days(days);
        let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%S").to_string();

        let deleted = conn
            .execute(
                "DELETE FROM usage_records WHERE created_at < ?1",
                params![cutoff_str],
            )
            .context("Failed to cleanup old usage records")?;

        Ok(deleted)
    }

    // ── Model Pricing ──

    /// Upsert model pricing for a provider+model combination.
    /// Uses INSERT OR REPLACE to handle the UNIQUE(provider_id, model_id) constraint.
    pub fn upsert_model_pricing(
        &self,
        provider_id: &str,
        model_id: &str,
        price_per_1k_prompt: f64,
        price_per_1k_completion: f64,
    ) -> Result<ModelPricing> {
        let id = Uuid::new_v4().to_string();
        let conn = self.conn();

        conn.execute(
            "INSERT INTO model_pricing (id, provider_id, model_id, price_per_1k_prompt, price_per_1k_completion, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(provider_id, model_id) DO UPDATE SET
                price_per_1k_prompt = excluded.price_per_1k_prompt,
                price_per_1k_completion = excluded.price_per_1k_completion,
                updated_at = datetime('now')",
            params![id, provider_id, model_id, price_per_1k_prompt, price_per_1k_completion],
        )
        .with_context(|| format!("Failed to upsert model pricing for {} / {}", provider_id, model_id))?;

        // Fetch the row back (id may differ on update due to ON CONFLICT)
        let pricing = conn
            .query_row(
                "SELECT id, provider_id, model_id, price_per_1k_prompt, price_per_1k_completion, currency, updated_at
                 FROM model_pricing WHERE provider_id = ?1 AND model_id = ?2",
                params![provider_id, model_id],
                |row| {
                    Ok(ModelPricing {
                        id: row.get(0)?,
                        provider_id: row.get(1)?,
                        model_id: row.get(2)?,
                        price_per_1k_prompt: row.get(3)?,
                        price_per_1k_completion: row.get(4)?,
                        currency: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .context("Failed to read back upserted model pricing")?;

        Ok(pricing)
    }

    /// Get model pricing entries, optionally filtered by provider_id and/or model_id.
    /// If both are None, returns all pricing entries.
    pub fn get_model_pricing(
        &self,
        provider_id: Option<&str>,
        model_id: Option<&str>,
    ) -> Result<Vec<ModelPricing>> {
        let conn = self.conn();

        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            match (provider_id, model_id) {
                (Some(pid), Some(mid)) => (
                    "SELECT id, provider_id, model_id, price_per_1k_prompt, price_per_1k_completion, currency, updated_at
                     FROM model_pricing WHERE provider_id = ?1 AND model_id = ?2"
                        .to_string(),
                    vec![Box::new(pid.to_string()), Box::new(mid.to_string())],
                ),
                (Some(pid), None) => (
                    "SELECT id, provider_id, model_id, price_per_1k_prompt, price_per_1k_completion, currency, updated_at
                     FROM model_pricing WHERE provider_id = ?1"
                        .to_string(),
                    vec![Box::new(pid.to_string())],
                ),
                (None, Some(mid)) => (
                    "SELECT id, provider_id, model_id, price_per_1k_prompt, price_per_1k_completion, currency, updated_at
                     FROM model_pricing WHERE model_id = ?1"
                        .to_string(),
                    vec![Box::new(mid.to_string())],
                ),
                (None, None) => (
                    "SELECT id, provider_id, model_id, price_per_1k_prompt, price_per_1k_completion, currency, updated_at
                     FROM model_pricing"
                        .to_string(),
                    vec![],
                ),
            };

        let mut stmt = conn.prepare(&sql).context("Failed to prepare model pricing query")?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(ModelPricing {
                    id: row.get(0)?,
                    provider_id: row.get(1)?,
                    model_id: row.get(2)?,
                    price_per_1k_prompt: row.get(3)?,
                    price_per_1k_completion: row.get(4)?,
                    currency: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })
            .context("Failed to query model pricing")?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.context("Failed to read model pricing row")?);
        }
        Ok(results)
    }

    /// Delete model pricing for a specific provider+model combination.
    /// Returns true if a row was deleted, false if it didn't exist.
    #[allow(dead_code)] // TODO: will be used for admin model pricing management
    pub fn delete_model_pricing(&self, provider_id: &str, model_id: &str) -> Result<bool> {
        let conn = self.conn();
        let deleted = conn
            .execute(
                "DELETE FROM model_pricing WHERE provider_id = ?1 AND model_id = ?2",
                params![provider_id, model_id],
            )
            .with_context(|| {
                format!(
                    "Failed to delete model pricing for {} / {}",
                    provider_id, model_id
                )
            })?;

        Ok(deleted > 0)
    }

    // ── API Keys ──

    /// Create a new API key for a user. Returns the full row.
    pub fn create_api_key(
        &self,
        user_id: &str,
        key_hash: &str,
        key_prefix: &str,
        name: &str,
        expires_at: Option<&str>,
    ) -> Result<ApiKeyRow> {
        let id = Uuid::new_v4().to_string();
        let conn = self.conn();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, key_hash, key_prefix, name, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, user_id, key_hash, key_prefix, name, expires_at],
        )
        .with_context(|| format!("Failed to create API key for user {}", user_id))?;

        Ok(ApiKeyRow {
            id,
            user_id: user_id.to_string(),
            key_hash: key_hash.to_string(),
            key_prefix: key_prefix.to_string(),
            name: name.to_string(),
            last_used_at: None,
            expires_at: expires_at.map(|s| s.to_string()),
            created_at: Utc::now().to_rfc3339(),
            revoked_at: None,
        })
    }

    /// List all API keys for a user (excluding key_hash).
    pub fn list_api_keys(&self, user_id: &str) -> Result<Vec<ApiKeyInfo>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, user_id, key_prefix, name, last_used_at, expires_at, created_at, revoked_at
                 FROM api_keys WHERE user_id = ?1 AND revoked_at IS NULL ORDER BY created_at DESC",
            )
            .context("Failed to prepare api_keys list query")?;

        let rows = stmt
            .query_map(params![user_id], |row| {
                Ok(ApiKeyInfo {
                    id: row.get(0)?,
                    // skip user_id (index 1) — not in ApiKeyInfo
                    key_prefix: row.get(2)?,
                    name: row.get(3)?,
                    last_used_at: row.get(4)?,
                    expires_at: row.get(5)?,
                    created_at: row.get(6)?,
                    revoked_at: row.get(7)?,
                })
            })
            .context("Failed to query api_keys")?;

        let mut keys = Vec::new();
        for row in rows {
            keys.push(row.context("Failed to read api_key row")?);
        }
        Ok(keys)
    }

    /// Revoke an API key (sets revoked_at). Only allows revoking keys belonging to the given user.
    pub fn revoke_api_key(&self, user_id: &str, key_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn();
        let deleted = conn
            .execute(
                "UPDATE api_keys SET revoked_at = ?1 WHERE id = ?2 AND user_id = ?3 AND revoked_at IS NULL",
                params![now, key_id, user_id],
            )
            .with_context(|| format!("Failed to revoke API key {} for user {}", key_id, user_id))?;

        if deleted == 0 {
            anyhow::bail!("API key not found, already revoked, or not owned by user");
        }
        Ok(())
    }

    /// Find an active (non-expired, non-revoked) API key by its hash.
    pub fn find_active_api_key(&self, key_hash: &str) -> Result<Option<ApiKeyRow>> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT id, user_id, key_hash, key_prefix, name, last_used_at, expires_at, created_at, revoked_at
                 FROM api_keys WHERE key_hash = ?1 AND revoked_at IS NULL",
                params![key_hash],
                |row| {
                    Ok(ApiKeyRow {
                        id: row.get(0)?,
                        user_id: row.get(1)?,
                        key_hash: row.get(2)?,
                        key_prefix: row.get(3)?,
                        name: row.get(4)?,
                        last_used_at: row.get(5)?,
                        expires_at: row.get(6)?,
                        created_at: row.get(7)?,
                        revoked_at: row.get(8)?,
                    })
                },
            )
            .ok();

        let key_row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        // Check expiry
        if let Some(ref expires_at) = key_row.expires_at {
            let expires: DateTime<Utc> = expires_at
                .parse()
                .context("Invalid expires_at in api_key")?;
            if Utc::now() > expires {
                return Ok(None);
            }
        }

        Ok(Some(key_row))
    }

    /// Update last_used_at for an API key.
    pub fn touch_api_key_last_used(&self, key_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn();
        conn.execute(
            "UPDATE api_keys SET last_used_at = ?1 WHERE id = ?2",
            params![now, key_id],
        )
        .with_context(|| format!("Failed to update last_used_at for API key {}", key_id))?;
        Ok(())
    }

    // ── Admin Methods ──

    /// Update a user's ergo wallet address. Pass None to unlink.
    /// Returns the updated User.
    pub fn update_wallet_address(&self, user_id: &str, ergo_address: Option<&str>) -> Result<User> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn();
        conn.execute(
            "UPDATE users SET ergo_address = ?1, updated_at = ?2 WHERE id = ?3",
            params![ergo_address, now, user_id],
        )
        .with_context(|| format!("Failed to update wallet address for user {}", user_id))?;

        self.get_user_by_id(user_id)?
            .context("User not found after wallet update")
    }

    /// Get a user's ergo wallet address (may be NULL).
    pub fn get_user_ergo_address(&self, user_id: &str) -> Result<Option<String>> {
        let conn = self.conn();
        let result: Option<String> = conn
            .query_row(
                "SELECT ergo_address FROM users WHERE id = ?1",
                params![user_id],
                |row| row.get(0),
            )
            .ok();
        Ok(result)
    }

    /// List users with optional tier filter. Returns (users, total_count).
    pub fn list_users(
        &self,
        limit: i64,
        offset: i64,
        tier_filter: Option<&str>,
    ) -> Result<(Vec<UserSummary>, i64)> {
        let conn = self.conn();

        let (total, rows) = match tier_filter {
            Some(tier) => {
                // Parameterized query — no SQL injection
                let total: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM users WHERE tier = ?1",
                        params![tier],
                        |row| row.get(0),
                    )
                    .context("Failed to count users")?;

                let mut stmt = conn
                    .prepare(
                        "SELECT u.id, u.email, u.name, u.tier, u.created_at,
                                COALESCE(SUM(ct.amount_usd), 0.0) as credits
                         FROM users u
                         LEFT JOIN credit_transactions ct ON ct.user_id = u.id
                         WHERE u.tier = ?3
                         GROUP BY u.id
                         ORDER BY u.created_at DESC
                         LIMIT ?1 OFFSET ?2",
                    )
                    .context("Failed to prepare user list query")?;

                let rows = stmt
                    .query_map(params![limit, offset, tier], |row| {
                        Ok(UserSummary {
                            id: row.get(0)?,
                            email: row.get(1)?,
                            name: row.get(2)?,
                            tier: row.get(3)?,
                            credits: row.get(5)?,
                            created_at: row.get::<_, String>(4)?.parse().unwrap(),
                        })
                    })
                    .context("Failed to query users")?;

                let mut users = Vec::new();
                for row in rows {
                    users.push(row.context("Failed to read user row")?);
                }
                (total, users)
            }
            None => {
                let total: i64 = conn
                    .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
                    .context("Failed to count users")?;

                let mut stmt = conn
                    .prepare(
                        "SELECT u.id, u.email, u.name, u.tier, u.created_at,
                                COALESCE(SUM(ct.amount_usd), 0.0) as credits
                         FROM users u
                         LEFT JOIN credit_transactions ct ON ct.user_id = u.id
                         GROUP BY u.id
                         ORDER BY u.created_at DESC
                         LIMIT ?1 OFFSET ?2",
                    )
                    .context("Failed to prepare user list query")?;

                let rows = stmt
                    .query_map(params![limit, offset], |row| {
                        Ok(UserSummary {
                            id: row.get(0)?,
                            email: row.get(1)?,
                            name: row.get(2)?,
                            tier: row.get(3)?,
                            credits: row.get(5)?,
                            created_at: row.get::<_, String>(4)?.parse().unwrap(),
                        })
                    })
                    .context("Failed to query users")?;

                let mut users = Vec::new();
                for row in rows {
                    users.push(row.context("Failed to read user row")?);
                }
                (total, users)
            }
        };

        Ok((rows, total))
    }

    /// Admin: update a user's tier. Returns the full updated User.
    pub fn update_user_tier_admin(&self, user_id: &str, new_tier: &str) -> Result<User> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn();

        let updated = conn
            .execute(
                "UPDATE users SET tier = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_tier, now, user_id],
            )
            .with_context(|| format!("Failed to update tier for user {}", user_id))?;

        if updated == 0 {
            anyhow::bail!("User not found: {}", user_id);
        }

        self.get_user_by_id(user_id)?
            .context("User not found after tier update")
    }

    /// Admin: adjust user credits (add or deduct). Creates a credit_transaction record.
    /// Returns the full updated User.
    pub fn admin_adjust_credits(
        &self,
        user_id: &str,
        amount: f64,
        reason: &str,
    ) -> Result<User> {
        // Verify user exists
        self.get_user_by_id(user_id)?
            .context(format!("User not found: {}", user_id))?;

        // Create a credit transaction record
        let tx_id = Uuid::new_v4().to_string();
        let kind = if amount >= 0.0 { "admin_credit" } else { "admin_debit" };
        let description = format!("Admin: {}", reason);

        self.add_credits(&tx_id, user_id, amount, kind, &description, None)
            .with_context(|| format!("Failed to adjust credits for user {}", user_id))?;

        self.get_user_by_id(user_id)?
            .context("User not found after credit adjustment")
    }

    /// Get platform-wide statistics for the admin dashboard.
    pub fn get_platform_stats(&self) -> Result<PlatformStats> {
        let conn = self.conn();

        let stats = conn
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM users) as total_users,
                    (SELECT COUNT(DISTINCT provider_id) FROM usage_records) as total_providers,
                    (SELECT COUNT(*) FROM usage_records) as total_requests,
                    (SELECT COALESCE(SUM(ct_sum), 0.0) FROM (
                        SELECT SUM(amount_usd) as ct_sum
                        FROM credit_transactions
                        GROUP BY user_id
                    )) as total_credits,
                    (SELECT COALESCE(SUM(cost_credits), 0.0) FROM usage_records) as total_revenue",
                [],
                |row| {
                    Ok(PlatformStats {
                        total_users: row.get(0)?,
                        total_providers: row.get(1)?,
                        total_requests: row.get(2)?,
                        total_credits_in_circulation: row.get(3)?,
                        total_revenue: row.get(4)?,
                    })
                },
            )
            .context("Failed to query platform stats")?;

        Ok(stats)
    }

    // ── Auto-replenish idempotency ──

    /// Update last_replenish_at for a user after successful auto-replenish.
    pub fn update_last_replenish_at(&self, user_id: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE users SET last_replenish_at = datetime('now') WHERE id = ?1",
            params![user_id],
        )
        .with_context(|| format!("Failed to update last_replenish_at for user {}", user_id))?;
        Ok(())
    }

    // ── API Key limits ──

    /// Count the number of active (non-revoked) API keys for a user.
    pub fn count_active_api_keys(&self, user_id: &str) -> Result<i64> {
        let conn = self.conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM api_keys WHERE user_id = ?1 AND revoked_at IS NULL",
                params![user_id],
                |row| row.get(0),
            )
            .context("Failed to count active API keys")?;
        Ok(count)
    }

    // ── Test helper ──

    /// Create an in-memory database for testing.
    #[cfg(test)]
    pub(crate) fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .context("Failed to open in-memory database")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .context("Failed to set SQLite pragmas")?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.run_migrations()?;
        Ok(db)
    }

    /// Aggregate per-provider usage stats for the public leaderboard.
    ///
    /// Groups successful usage_records by `provider_id` and computes totals
    /// for requests, tokens, revenue, and unique models served.
    pub fn get_provider_leaderboard(&self) -> Result<Vec<ProviderLeaderboardEntry>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT
                provider_id,
                COUNT(*)                                   AS total_requests,
                COALESCE(SUM(prompt_tokens), 0)            AS total_prompt_tokens,
                COALESCE(SUM(completion_tokens), 0)        AS total_completion_tokens,
                COALESCE(SUM(prompt_tokens + completion_tokens), 0) AS total_tokens,
                COALESCE(SUM(cost_usd), 0)                 AS total_revenue_usd,
                MIN(created_at)                            AS first_seen,
                MAX(created_at)                            AS last_seen,
                (SELECT COUNT(DISTINCT ur2.model)
                 FROM usage_records ur2
                 WHERE ur2.provider_id = usage_records.provider_id
                   AND ur2.status = 'success')             AS unique_models
            FROM usage_records
            WHERE status = 'success'
            GROUP BY provider_id
            ORDER BY total_tokens DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(ProviderLeaderboardEntry {
                provider_id: row.get(0)?,
                total_requests: row.get(1)?,
                total_prompt_tokens: row.get(2)?,
                total_completion_tokens: row.get(3)?,
                total_tokens: row.get(4)?,
                total_revenue_usd: row.get(5)?,
                first_seen: row.get(6)?,
                last_seen: row.get(7)?,
                unique_models: row.get(8)?,
            })
        })?;

        let mut entries: Vec<ProviderLeaderboardEntry> = rows
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to collect leaderboard rows")?;

        entries.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));
        Ok(entries)
    }
}

/// Aggregated usage statistics for a single provider (leaderboard row).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderLeaderboardEntry {
    pub provider_id: String,
    pub total_requests: i64,
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub total_tokens: i64,
    pub total_revenue_usd: f64,
    pub unique_models: i64,
    pub first_seen: String,
    pub last_seen: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Helper: create an in-memory DB with a test user and initial credits.
    fn setup_db_with_user(balance: f64) -> (Arc<Db>, String) {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let user_id = uuid::Uuid::new_v4().to_string();
        db.create_user(&user_id, "test@example.com", Some("Test"), "hash")
            .unwrap();
        if balance > 0.0 {
            db.add_credits(
                &uuid::Uuid::new_v4().to_string(),
                &user_id,
                balance,
                "purchase",
                "initial",
                None,
            )
            .unwrap();
        }
        (db, user_id)
    }

    // ── deduct_credits ──

    #[test]
    fn test_deduct_credits_success() {
        let (db, user_id) = setup_db_with_user(1.0);
        let balance_after = db
            .deduct_credits(
                &uuid::Uuid::new_v4().to_string(),
                &user_id,
                0.5,
                "test deduction",
            )
            .unwrap();
        assert!((balance_after - 0.5).abs() < f64::EPSILON);
        assert!((db.get_credit_balance(&user_id).unwrap() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_deduct_credits_exact_balance() {
        let (db, user_id) = setup_db_with_user(1.0);
        let balance_after = db
            .deduct_credits(
                &uuid::Uuid::new_v4().to_string(),
                &user_id,
                1.0,
                "exact deduction",
            )
            .unwrap();
        assert!((balance_after - 0.0).abs() < f64::EPSILON);
        assert!((db.get_credit_balance(&user_id).unwrap()).abs() < f64::EPSILON);
    }

    #[test]
    fn test_deduct_credits_insufficient_balance() {
        let (db, user_id) = setup_db_with_user(0.3);
        let result = db.deduct_credits(
            &uuid::Uuid::new_v4().to_string(),
            &user_id,
            0.5,
            "should fail",
        );
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains("Insufficient credits"), "unexpected error: {err_msg}");
        // Balance should be unchanged
        assert!((db.get_credit_balance(&user_id).unwrap() - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_deduct_credits_zero_balance_fails() {
        let (db, user_id) = setup_db_with_user(0.0);
        let result = db.deduct_credits(
            &uuid::Uuid::new_v4().to_string(),
            &user_id,
            0.01,
            "no funds",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_concurrent_deducts_no_overspend() {
        // Give user $1.00, try to deduct $0.60 twice concurrently.
        // Only one should succeed; final balance >= $0.00.
        let (db, user_id) = setup_db_with_user(1.0);
        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let uid1 = user_id.clone();
        let uid2 = user_id.clone();

        let t1 = std::thread::spawn(move || {
            db1.deduct_credits(
                &uuid::Uuid::new_v4().to_string(),
                &uid1,
                0.6,
                "concurrent-1",
            )
        });
        let t2 = std::thread::spawn(move || {
            db2.deduct_credits(
                &uuid::Uuid::new_v4().to_string(),
                &uid2,
                0.6,
                "concurrent-2",
            )
        });

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        let successes = r1.is_ok() as i32 + r2.is_ok() as i32;
        let failures = r1.is_err() as i32 + r2.is_err() as i32;
        assert_eq!(successes, 1, "Exactly one deduction should succeed");
        assert_eq!(failures, 1, "Exactly one deduction should fail");

        let final_balance = db.get_credit_balance(&user_id).unwrap();
        assert!(
            final_balance >= 0.0,
            "Balance must not go negative, got {final_balance}"
        );
        assert!(
            (final_balance - 0.4).abs() < 1e-10,
            "Expected $0.40 remaining, got {final_balance}"
        );
    }

    // ── count_active_api_keys ──

    #[test]
    fn test_count_active_api_keys_new_user() {
        let (db, user_id) = setup_db_with_user(0.0);
        let count = db.count_active_api_keys(&user_id).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_active_api_keys_after_create() {
        let (db, user_id) = setup_db_with_user(0.0);
        db.create_api_key(&user_id, "hash1", "xk_test_", "key1", None)
            .unwrap();
        assert_eq!(db.count_active_api_keys(&user_id).unwrap(), 1);

        db.create_api_key(&user_id, "hash2", "xk_test_", "key2", None)
            .unwrap();
        assert_eq!(db.count_active_api_keys(&user_id).unwrap(), 2);
    }

    #[test]
    fn test_count_active_api_keys_after_revoke() {
        let (db, user_id) = setup_db_with_user(0.0);
        let row = db
            .create_api_key(&user_id, "hash1", "xk_test_", "key1", None)
            .unwrap();
        assert_eq!(db.count_active_api_keys(&user_id).unwrap(), 1);

        db.revoke_api_key(&user_id, &row.id).unwrap();
        assert_eq!(db.count_active_api_keys(&user_id).unwrap(), 0);
    }

    // ── update_last_replenish_at ──

    #[test]
    fn test_update_last_replenish_at_sets_timestamp() {
        let (db, user_id) = setup_db_with_user(0.0);
        // Before update, last_replenish_at should be NULL
        {
            let conn = db.conn();
            let before: Option<String> = conn
                .query_row(
                    "SELECT last_replenish_at FROM users WHERE id = ?1",
                    params![user_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(before.is_none());
        } // Drop MutexGuard before calling other db methods

        // Update
        db.update_last_replenish_at(&user_id).unwrap();

        // After update, should be set (datetime('now') produces "YYYY-MM-DD HH:MM:SS")
        {
            let conn = db.conn();
            let after: Option<String> = conn
                .query_row(
                    "SELECT last_replenish_at FROM users WHERE id = ?1",
                    params![user_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(after.is_some());
            let ts = after.unwrap();
            // Verify it parses as NaiveDateTime (SQLite datetime format)
            let parsed =
                chrono::NaiveDateTime::parse_from_str(&ts, "%Y-%m-%d %H:%M:%S").unwrap();
            let now = chrono::Utc::now().naive_utc();
            // Should be within the last 5 seconds
            let diff = (now - parsed).num_seconds().abs();
            assert!(diff <= 5, "Timestamp should be recent, was {diff}s ago");
        }
    }

    // ── add_credits / get_credit_balance ──

    #[test]
    fn test_add_credits_and_balance() {
        let (db, user_id) = setup_db_with_user(0.0);
        assert!((db.get_credit_balance(&user_id).unwrap()).abs() < f64::EPSILON);

        db.add_credits(
            &uuid::Uuid::new_v4().to_string(),
            &user_id,
            5.0,
            "purchase",
            "top-up",
            None,
        )
        .unwrap();
        assert!((db.get_credit_balance(&user_id).unwrap() - 5.0).abs() < f64::EPSILON);

        db.add_credits(
            &uuid::Uuid::new_v4().to_string(),
            &user_id,
            3.0,
            "bonus",
            "promo",
            None,
        )
        .unwrap();
        assert!((db.get_credit_balance(&user_id).unwrap() - 8.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_multiple_deductions_sequential() {
        let (db, user_id) = setup_db_with_user(3.0);
        db.deduct_credits(
            &uuid::Uuid::new_v4().to_string(),
            &user_id,
            1.0,
            "deduct-1",
        )
        .unwrap();
        db.deduct_credits(
            &uuid::Uuid::new_v4().to_string(),
            &user_id,
            1.0,
            "deduct-2",
        )
        .unwrap();
        assert!((db.get_credit_balance(&user_id).unwrap() - 1.0).abs() < f64::EPSILON);

        // Third deduction should fail (need 1.0, have 1.0 — succeeds)
        db.deduct_credits(
            &uuid::Uuid::new_v4().to_string(),
            &user_id,
            1.0,
            "deduct-3",
        )
        .unwrap();
        assert!((db.get_credit_balance(&user_id).unwrap()).abs() < f64::EPSILON);

        // Fourth should fail
        let result = db.deduct_credits(
            &uuid::Uuid::new_v4().to_string(),
            &user_id,
            0.01,
            "deduct-4",
        );
        assert!(result.is_err());
    }
}
