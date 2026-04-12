use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::types::{UsageProof, get_current_timestamp};

#[derive(Debug, Clone)]
pub struct UserBalance {
    pub api_key: String,
    pub ergo_address: String,
    pub balance_erg: f64,
    pub used_tokens_input: u64,
    pub used_tokens_output: u64,
}

#[derive(Debug, Clone)]
pub struct PendingUsage {
    pub id: Option<i64>,
    pub api_key: String,
    pub tokens_input: u32,
    pub tokens_output: u32,
    pub model: String,
    pub timestamp: u64,
    pub settled: bool,
}

// Thread-safe wrapper around the database connection
pub struct SettlementManager {
    conn: Arc<Mutex<Connection>>,
}

impl SettlementManager {
    pub fn new(db_path: &str) -> Result<Self, Box<dyn Error>> {
        let conn = Connection::open(db_path)?;
        
        // Create tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS user_balances (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                api_key TEXT UNIQUE NOT NULL,
                ergo_address TEXT NOT NULL,
                balance_erg REAL NOT NULL DEFAULT 0.0,
                used_tokens_input INTEGER NOT NULL DEFAULT 0,
                used_tokens_output INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS pending_usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                api_key TEXT NOT NULL,
                tokens_input INTEGER NOT NULL,
                tokens_output INTEGER NOT NULL,
                model TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                settled INTEGER NOT NULL DEFAULT 0,
                settled_at INTEGER,
                transaction_id TEXT
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_pending_api_key ON pending_usage(api_key)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_pending_settled ON pending_usage(settled)",
            [],
        )?;

        Ok(Self { 
            conn: Arc::new(Mutex::new(conn))
        })
    }

    // Initialize or get user balance
    pub async fn get_or_create_balance(&self, api_key: &str, ergo_address: &str) -> Result<UserBalance, Box<dyn Error>> {
        let conn = self.conn.lock().await;
        
        let mut stmt = conn.prepare(
            "SELECT api_key, ergo_address, balance_erg, used_tokens_input, used_tokens_output 
             FROM user_balances WHERE api_key = ?"
        )?;
        
        let mut rows = stmt.query(params![api_key])?;
        
        if let Some(row) = rows.next()? {
            Ok(UserBalance {
                api_key: api_key.to_string(),
                ergo_address: row.get(1)?,
                balance_erg: row.get(2)?,
                used_tokens_input: row.get(3)?,
                used_tokens_output: row.get(4)?,
            })
        } else {
            drop(rows); // Drop rows before executing again
            drop(stmt); // Drop stmt
            
            // Insert new balance
            let now = get_current_timestamp();
            conn.execute(
                "INSERT INTO user_balances (api_key, ergo_address, balance_erg, used_tokens_input, used_tokens_output, created_at, updated_at)
                 VALUES (?1, ?2, 100.0, 0, 0, ?3, ?3)",
                params![api_key, ergo_address, now],
            )?;
            
            Ok(UserBalance {
                api_key: api_key.to_string(),
                ergo_address: ergo_address.to_string(),
                balance_erg: 100.0, // Default starting balance
                used_tokens_input: 0,
                used_tokens_output: 0,
            })
        }
    }

    // Check if user has sufficient balance
    pub async fn check_balance(&self, api_key: &str, required_erg: f64) -> Result<bool, Box<dyn Error>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT balance_erg FROM user_balances WHERE api_key = ?"
        )?;
        
        let balance: f64 = stmt.query_row(params![api_key], |row| row.get(0))?;
        Ok(balance >= required_erg)
    }

    // Deduct balance (called after successful inference)
    pub async fn deduct_balance(&self, api_key: &str, amount_erg: f64) -> Result<(), Box<dyn Error>> {
        let conn = self.conn.lock().await;
        let now = get_current_timestamp();
        conn.execute(
            "UPDATE user_balances 
             SET balance_erg = balance_erg - ?1, used_tokens_input = used_tokens_input + ?2, used_tokens_output = used_tokens_output + ?3, updated_at = ?4
             WHERE api_key = ?5",
            params![amount_erg, 0, 0, now, api_key],
        )?;
        Ok(())
    }

    // Record usage for settlement
    pub async fn record_usage(&self, api_key: &str, tokens_input: u32, tokens_output: u32, model: &str) -> Result<i64, Box<dyn Error>> {
        let conn = self.conn.lock().await;
        let now = get_current_timestamp();
        
        conn.execute(
            "INSERT INTO pending_usage (api_key, tokens_input, tokens_output, model, timestamp, settled)
             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![api_key, tokens_input, tokens_output, model, now],
        )?;

        // Get the ID of the inserted row
        let id = conn.last_insert_rowid();
        
        // Also update token counts in balance table
        conn.execute(
            "UPDATE user_balances 
             SET used_tokens_input = used_tokens_input + ?1, used_tokens_output = used_tokens_output + ?2, updated_at = ?3
             WHERE api_key = ?4",
            params![tokens_input, tokens_output, now, api_key],
        )?;

        Ok(id)
    }

    // Get pending usage proofs for a provider
    pub async fn get_pending_proofs(&self, api_key: &str, limit: usize) -> Result<Vec<UsageProof>, Box<dyn Error>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT tokens_input, tokens_output, model, timestamp 
             FROM pending_usage 
             WHERE api_key = ? AND settled = 0 
             ORDER BY timestamp ASC 
             LIMIT ?"
        )?;
        
        let proofs = stmt.query_map(params![api_key, limit], |row| {
            Ok(UsageProof {
                provider_id: api_key.to_string(), // Using api_key as provider_id for now
                tokens_input: row.get(0)?,
                tokens_output: row.get(1)?,
                timestamp: row.get(2)?,
                inference_id: None,
                model_used: row.get(3).ok(),
            })
        })?;
        
        Ok(proofs.collect::<Result<Vec<_>, _>>()?)
    }

    // Mark usage as settled
    pub async fn mark_settled(&self, api_key: &str, transaction_id: &str) -> Result<usize, Box<dyn Error>> {
        let conn = self.conn.lock().await;
        let now = get_current_timestamp();
        
        let affected = conn.execute(
            "UPDATE pending_usage 
             SET settled = 1, settled_at = ?1, transaction_id = ?2
             WHERE api_key = ?3 AND settled = 0",
            params![now, transaction_id, api_key],
        )?;
        
        Ok(affected)
    }

    // Get settlement summary for a provider
    pub async fn get_settlement_summary(&self, api_key: &str) -> Result<SettlementSummary, Box<dyn Error>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT 
                COUNT(*) as total,
                SUM(CASE WHEN settled = 0 THEN 1 ELSE 0 END) as pending,
                SUM(CASE WHEN settled = 1 THEN 1 ELSE 0 END) as settled,
                SUM(tokens_input) as total_tokens_input,
                SUM(tokens_output) as total_tokens_output
             FROM pending_usage WHERE api_key = ?"
        )?;
        
        let summary = stmt.query_row(params![api_key], |row| {
            Ok(SettlementSummary {
                total_records: row.get(0)?,
                pending_records: row.get(1)?,
                settled_records: row.get(2)?,
                total_tokens_input: row.get(3).unwrap_or(0),
                total_tokens_output: row.get(4).unwrap_or(0),
            })
        })?;
        
        Ok(summary)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SettlementSummary {
    pub total_records: usize,
    pub pending_records: usize,
    pub settled_records: usize,
    pub total_tokens_input: u32,
    pub total_tokens_output: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_settlement_manager() -> Result<(), Box<dyn Error>> {
        let temp_db = tempfile::NamedTempFile::new()?;
        let manager = SettlementManager::new(temp_db.path().to_str().unwrap())?;
        
        // Test balance creation
        let balance = manager.get_or_create_balance("test-key", "0x1234").await?;
        assert_eq!(balance.balance_erg, 100.0);
        
        // Test usage recording
        let id = manager.record_usage("test-key", 1000, 500, "test-model").await?;
        assert!(id > 0);
        
        // Test settlement summary
        let summary = manager.get_settlement_summary("test-key").await?;
        assert_eq!(summary.total_records, 1);
        assert_eq!(summary.pending_records, 1);
        
        Ok(())
    }
}
