//! Transaction safety guards for all Ergo transaction builders.
//!
//! Every transaction in Xergon passes through these guards before submission.
//! This prevents: dust outputs, insufficient fees, value overflow, token loss,
//! and invalid addresses.

use anyhow::{bail, Result};

// --- Constants (matching Ergo protocol) ---

/// Minimum box value on Ergo: 360 nanoERG per byte of box size.
pub const BOX_VALUE_PER_BYTE: u64 = 360;

/// Practical minimum for simple boxes without tokens/registers.
/// 0.001 ERG = 1,000,000 nanoERG
pub const SAFE_MIN_BOX_VALUE: u64 = 1_000_000;

/// Recommended minimum fee: 0.001 ERG = 1,000,000 nanoERG
pub const RECOMMENDED_MIN_FEE: u64 = 1_000_000;

/// Maximum fee guard: 0.1 ERG (100,000,000 nanoERG). Prevents fat-finger errors.
pub const MAX_REASONABLE_FEE: u64 = 100_000_000;

/// Maximum batch size for usage proof batching (prevents gas exhaustion).
pub const MAX_BATCH_SIZE: usize = 50;

/// Maximum ERG value in a single output: 1,000,000 ERG (prevents overflow).
/// Ergo total supply is ~97.7M ERG.
pub const MAX_OUTPUT_VALUE: u64 = 1_000_000_000_000_000; // 1M ERG in nanoERG

// --- Validation Functions ---

/// Validate that a box value meets the minimum requirement.
/// For boxes with tokens or registers, the minimum is higher due to size.
pub fn validate_box_value(value_nanoerg: u64, has_tokens: bool, register_count: u32) -> Result<()> {
    // Dynamic minimum: base 100 bytes (header + ergo_tree + basic fields)
    // + extra for tokens (34 bytes each) + registers (variable)
    let estimated_box_size: u64 = 100
        + if has_tokens { 40 } else { 0 } // token list overhead
        + (register_count as u64) * 40;    // rough register size estimate

    let dynamic_min = (estimated_box_size * BOX_VALUE_PER_BYTE).max(SAFE_MIN_BOX_VALUE);

    if value_nanoerg < dynamic_min {
        bail!(
            "Box value {} nanoERG is below safe minimum {} nanoERG (estimated box size: {} bytes)",
            value_nanoerg, dynamic_min, estimated_box_size
        );
    }

    if value_nanoerg > MAX_OUTPUT_VALUE {
        bail!(
            "Box value {} nanoERG exceeds maximum allowed {} nanoERG",
            value_nanoerg, MAX_OUTPUT_VALUE
        );
    }

    Ok(())
}

/// Validate a transaction fee is within reasonable bounds.
pub fn validate_fee(fee_nanoerg: u64) -> Result<()> {
    if fee_nanoerg < RECOMMENDED_MIN_FEE {
        bail!(
            "Fee {} nanoERG is below recommended minimum {} nanoERG",
            fee_nanoerg, RECOMMENDED_MIN_FEE
        );
    }
    if fee_nanoerg > MAX_REASONABLE_FEE {
        bail!(
            "Fee {} nanoERG exceeds maximum reasonable fee {} nanoERG (possible error)",
            fee_nanoerg, MAX_REASONABLE_FEE
        );
    }
    Ok(())
}

/// Validate that an Ergo address or ErgoTree hex is properly formatted.
pub fn validate_address_or_tree(address_or_tree: &str) -> Result<()> {
    if address_or_tree.is_empty() {
        bail!("Address or ErgoTree cannot be empty");
    }

    // If it looks like hex (long string of hex chars), validate as ErgoTree
    if address_or_tree.len() > 40 && address_or_tree.chars().all(|c| c.is_ascii_hexdigit()) {
        // ErgoTree hex - validate minimum length (at least 10 bytes = 20 hex chars)
        if address_or_tree.len() < 20 {
            bail!("ErgoTree hex too short: {} chars", address_or_tree.len());
        }
        return Ok(());
    }

    // Otherwise, validate as an Ergo address (base58, specific prefixes)
    // Mainnet: 3 (P2PK), 2 (P2SH), ? (P2S)
    // Testnet: 9 (P2PK), 8 (P2SH), ? (P2S)
    // pk_ prefix for raw public keys
    if address_or_tree.starts_with("pk_") {
        let pk_hex = &address_or_tree[3..];
        let pk_bytes = hex::decode(pk_hex)
            .map_err(|_| anyhow::anyhow!("Invalid hex after pk_ prefix"))?;
        if pk_bytes.len() != 33 {
            bail!("PK after pk_ prefix must be 33 bytes (compressed secp256k1), got {} bytes", pk_bytes.len());
        }
        return Ok(());
    }

    // Base58 check: must be reasonable length (20-100 chars), alphanumeric
    if address_or_tree.len() < 20 || address_or_tree.len() > 100 {
        bail!("Address length {} is outside valid range (20-100 chars)", address_or_tree.len());
    }

    // Check for valid Ergo address prefixes
    let first_char = address_or_tree.chars().next().unwrap();
    match first_char {
        '3' | '2' | '9' | '8' => Ok(()), // Valid mainnet/testnet prefixes
        _ => bail!(
            "Address '{}' does not start with a recognized Ergo prefix (3, 2, 9, or 8)",
            &address_or_tree[..1.min(address_or_tree.len())]
        ),
    }
}

/// Validate a compressed secp256k1 public key hex.
pub fn validate_pk_hex(pk_hex: &str, field_name: &str) -> Result<()> {
    if pk_hex.is_empty() {
        bail!("{} is required", field_name);
    }
    let pk_bytes = hex::decode(pk_hex)
        .map_err(|_| anyhow::anyhow!("{} is not valid hex", field_name))?;
    if pk_bytes.len() != 33 {
        bail!(
            "{} must be 33 bytes (compressed secp256k1), got {} bytes",
            field_name, pk_bytes.len()
        );
    }
    // Verify the prefix byte (02 or 03 for compressed keys)
    match pk_bytes[0] {
        0x02 | 0x03 => Ok(()),
        _ => bail!(
            "{} has invalid prefix byte 0x{:02x}, expected 0x02 or 0x03",
            field_name, pk_bytes[0]
        ),
    }
}

/// Validate a token ID (should be 64 hex chars = 32 bytes).
pub fn validate_token_id(token_id: &str) -> Result<()> {
    if token_id.is_empty() {
        bail!("Token ID cannot be empty");
    }
    if token_id.len() != 64 {
        bail!("Token ID must be 64 hex chars (32 bytes), got {} chars", token_id.len());
    }
    hex::decode(token_id)
        .map_err(|_| anyhow::anyhow!("Token ID '{}' is not valid hex", token_id))?;
    Ok(())
}

/// Validate batch size for batched operations.
pub fn validate_batch_size(batch_size: usize) -> Result<()> {
    if batch_size == 0 {
        bail!("Batch size cannot be zero");
    }
    if batch_size > MAX_BATCH_SIZE {
        bail!(
            "Batch size {} exceeds maximum {} (prevents gas exhaustion)",
            batch_size, MAX_BATCH_SIZE
        );
    }
    Ok(())
}

/// Validate that a payment request JSON has correct structure before sending to node.
/// This is the final gatekeeper before any tx hits the network.
pub fn validate_payment_request(request: &serde_json::Value) -> Result<()> {
    // Check fee
    if let Some(fee) = request.get("fee") {
        let fee_val: u64 = if fee.is_string() {
            fee.as_str()
                .unwrap_or("0")
                .parse()
                .map_err(|_| anyhow::anyhow!("Fee is not a valid number"))?
        } else {
            fee.as_u64().unwrap_or(0)
        };
        validate_fee(fee_val)?;
    } else {
        bail!("Payment request missing 'fee' field");
    }

    // Check requests array
    let requests = request
        .get("requests")
        .and_then(|r| r.as_array())
        .ok_or_else(|| anyhow::anyhow!("Payment request missing 'requests' array"))?;

    if requests.is_empty() {
        bail!("Payment request has empty 'requests' array — no outputs");
    }

    for (i, req) in requests.iter().enumerate() {
        // Check value
        let value: u64 = req
            .get("value")
            .and_then(|v| v.as_str().and_then(|s| s.parse().ok()))
            .or_else(|| req.get("value").and_then(|v| v.as_u64()))
            .ok_or_else(|| anyhow::anyhow!("Request [{}] missing 'value' field", i))?;

        let has_tokens = req
            .get("assets")
            .and_then(|a| a.as_array())
            .map(|arr| !arr.is_empty())
            .unwrap_or(false);

        let register_count = req
            .get("registers")
            .and_then(|r| r.as_object())
            .map(|obj| obj.len() as u32)
            .unwrap_or(0);

        validate_box_value(value, has_tokens, register_count)
            .map_err(|e| anyhow::anyhow!("Request [{}] validation failed: {}", i, e))?;

        // Check address or ergoTree
        let has_address = req.get("address").is_some();
        let has_tree = req.get("ergoTree").is_some();
        if !has_address && !has_tree {
            bail!("Request [{}] must have either 'address' or 'ergoTree'", i);
        }

        if let Some(addr) = req.get("address").and_then(|a| a.as_str()) {
            validate_address_or_tree(addr)
                .map_err(|e| anyhow::anyhow!("Request [{}] address invalid: {}", i, e))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_box_value_ok() {
        assert!(validate_box_value(SAFE_MIN_BOX_VALUE, false, 0).is_ok());
        assert!(validate_box_value(100_000_000, true, 4).is_ok());
    }

    #[test]
    fn test_validate_box_value_too_low() {
        assert!(validate_box_value(100, false, 0).is_err());
    }

    #[test]
    fn test_validate_box_value_too_high() {
        assert!(validate_box_value(MAX_OUTPUT_VALUE + 1, false, 0).is_err());
    }

    #[test]
    fn test_validate_fee_ok() {
        assert!(validate_fee(RECOMMENDED_MIN_FEE).is_ok());
        assert!(validate_fee(10_000_000).is_ok());
    }

    #[test]
    fn test_validate_fee_too_low() {
        assert!(validate_fee(100).is_err());
    }

    #[test]
    fn test_validate_fee_too_high() {
        assert!(validate_fee(MAX_REASONABLE_FEE + 1).is_err());
    }

    #[test]
    fn test_validate_address_or_tree_empty() {
        assert!(validate_address_or_tree("").is_err());
    }

    #[test]
    fn test_validate_address_mainnet() {
        // Valid mainnet P2PK address (prefix 3, length 30-100 chars)
        assert!(validate_address_or_tree("3WxsWnB2j4e2o8XQFQp1BLM9M5v5VQqK2abc").is_ok());
        // Too short for a valid address
        assert!(validate_address_or_tree("3WxsW").is_err());
    }

    #[test]
    fn test_validate_address_pk_prefix_bad() {
        assert!(validate_address_or_tree("pk_not_hex").is_err());
    }

    #[test]
    fn test_validate_address_bad_prefix() {
        assert!(validate_address_or_tree("1WxsWnB2j4e2o8XQFQp1BLM9M5v5VQqK2").is_err());
    }

    #[test]
    fn test_validate_pk_hex_ok() {
        let pk = "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(validate_pk_hex(pk, "test_pk").is_ok());
    }

    #[test]
    fn test_validate_pk_hex_empty() {
        assert!(validate_pk_hex("", "test_pk").is_err());
    }

    #[test]
    fn test_validate_pk_hex_wrong_length() {
        assert!(validate_pk_hex("02aabb", "test_pk").is_err());
    }

    #[test]
    fn test_validate_pk_hex_bad_prefix() {
        let pk = "04aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(validate_pk_hex(pk, "test_pk").is_err());
    }

    #[test]
    fn test_validate_token_id_ok() {
        let id = "a".repeat(64);
        assert!(validate_token_id(&id).is_ok());
    }

    #[test]
    fn test_validate_token_id_empty() {
        assert!(validate_token_id("").is_err());
    }

    #[test]
    fn test_validate_token_id_wrong_length() {
        assert!(validate_token_id("abc123").is_err());
    }

    #[test]
    fn test_validate_token_id_bad_hex() {
        assert!(validate_token_id(("zz".repeat(32)).as_str()).is_err());
    }

    #[test]
    fn test_validate_batch_size_ok() {
        assert!(validate_batch_size(1).is_ok());
        assert!(validate_batch_size(MAX_BATCH_SIZE).is_ok());
    }

    #[test]
    fn test_validate_batch_size_zero() {
        assert!(validate_batch_size(0).is_err());
    }

    #[test]
    fn test_validate_batch_size_too_large() {
        assert!(validate_batch_size(MAX_BATCH_SIZE + 1).is_err());
    }

    #[test]
    fn test_validate_payment_request_basic() {
        let request = serde_json::json!({
            "requests": [{
                "address": "3WxsWnB2j4e2o8XQFQp1BLM9M5v5VQqK2a",
                "value": "1000000000"
            }],
            "fee": 1000000
        });
        // Valid address (33 chars, prefix 3) and valid fee
        assert!(validate_payment_request(&request).is_ok());
    }

    #[test]
    fn test_validate_payment_request_missing_fee() {
        let request = serde_json::json!({
            "requests": [{
                "address": "3WxsWnB2j4e2o8XQFQp1BLM9M5v5VQqK2a",
                "value": "1000000000"
            }]
        });
        assert!(validate_payment_request(&request).is_err());
    }

    #[test]
    fn test_validate_payment_request_empty_requests() {
        let request = serde_json::json!({
            "requests": [],
            "fee": 1000000
        });
        assert!(validate_payment_request(&request).is_err());
    }

    #[test]
    fn test_validate_payment_request_fee_as_string() {
        let request = serde_json::json!({
            "requests": [{
                "address": "3WxsWnB2j4e2o8XQFQp1BLM9M5v5VQqK2a",
                "value": "1000000000"
            }],
            "fee": "1000000"
        });
        // Valid: fee as string is accepted
        assert!(validate_payment_request(&request).is_ok());
    }
}
