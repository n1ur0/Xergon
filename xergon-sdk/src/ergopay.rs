use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ErgoPay request type
// ---------------------------------------------------------------------------

/// The type of ErgoPay request to build.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ErgoPayRequestType {
    /// Staking-related request.
    Staking,
    /// Simple payment request.
    Payment,
    /// Provider registration / update request.
    Provider,
}

impl std::fmt::Display for ErgoPayRequestType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErgoPayRequestType::Staking => write!(f, "staking"),
            ErgoPayRequestType::Payment => write!(f, "payment"),
            ErgoPayRequestType::Provider => write!(f, "provider"),
        }
    }
}

impl std::str::FromStr for ErgoPayRequestType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "staking" => Ok(ErgoPayRequestType::Staking),
            "payment" => Ok(ErgoPayRequestType::Payment),
            "provider" => Ok(ErgoPayRequestType::Provider),
            other => Err(format!("unknown request type '{}'; expected staking|payment|provider", other)),
        }
    }
}

// ---------------------------------------------------------------------------
// BuildRequest
// ---------------------------------------------------------------------------

/// Parameters for building an ErgoPay request.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BuildRequest {
    /// The type of ErgoPay request.
    pub request_type: ErgoPayRequestType,
    /// Sender / source address.
    pub address: String,
    /// Amount in nanoERG.
    pub amount: u64,
    /// Recipient address (for payment requests).
    pub recipient: String,
}

impl Default for BuildRequest {
    fn default() -> Self {
        Self {
            request_type: ErgoPayRequestType::Payment,
            address: String::new(),
            amount: 0,
            recipient: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// ErgoPayRequest (built request)
// ---------------------------------------------------------------------------

/// A built ErgoPay request ready for signing.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ErgoPayRequest {
    /// Unique request identifier.
    pub request_id: String,
    /// The parameters used to build this request.
    pub params: BuildRequest,
    /// Simulated reduced transaction (hex-encoded for display).
    pub reduced_tx_hex: String,
    /// Timestamp when the request was created.
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// UriRequest
// ---------------------------------------------------------------------------

/// Parameters for generating an `ergopay:` URI.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UriRequest {
    /// The request ID to reference.
    pub request_id: String,
    /// The endpoint URL that serves the signing request.
    pub endpoint_url: String,
}

impl Default for UriRequest {
    fn default() -> Self {
        Self {
            request_id: String::new(),
            endpoint_url: String::from("https://explorer.ergoplatform.com/ergo-pay"),
        }
    }
}

// ---------------------------------------------------------------------------
// UriResult
// ---------------------------------------------------------------------------

/// The generated `ergopay:` URI.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UriResult {
    /// The full `ergopay:` URI string.
    pub uri: String,
    /// The request ID referenced.
    pub request_id: String,
    /// The endpoint URL.
    pub endpoint_url: String,
    /// Whether the URI uses dynamic mode (vs static base64).
    pub is_dynamic: bool,
    /// Timestamp.
    pub generated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// StatusRequest
// ---------------------------------------------------------------------------

/// Parameters for checking an ErgoPay request status.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StatusRequest {
    /// The request ID to check.
    pub request_id: String,
    /// The relay URL that tracks request status.
    pub relay_url: String,
}

impl Default for StatusRequest {
    fn default() -> Self {
        Self {
            request_id: String::new(),
            relay_url: String::from("https://relay.ergoplatform.com"),
        }
    }
}

// ---------------------------------------------------------------------------
// RequestStatus
// ---------------------------------------------------------------------------

/// Status of an ErgoPay request.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum RequestStatus {
    /// Request created but not yet presented to a wallet.
    Pending,
    /// Request has been presented to a wallet; awaiting signature.
    AwaitingSignature,
    /// Wallet signed the transaction.
    Signed,
    /// Signed transaction has been submitted to the network.
    Submitted,
    /// Transaction confirmed on-chain.
    Confirmed,
    /// The request expired before signing.
    Expired,
    /// The request failed.
    Failed,
}

impl std::fmt::Display for RequestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestStatus::Pending => write!(f, "Pending"),
            RequestStatus::AwaitingSignature => write!(f, "AwaitingSignature"),
            RequestStatus::Signed => write!(f, "Signed"),
            RequestStatus::Submitted => write!(f, "Submitted"),
            RequestStatus::Confirmed => write!(f, "Confirmed"),
            RequestStatus::Expired => write!(f, "Expired"),
            RequestStatus::Failed => write!(f, "Failed"),
        }
    }
}

// ---------------------------------------------------------------------------
// StatusResult
// ---------------------------------------------------------------------------

/// Result of checking an ErgoPay request status.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StatusResult {
    /// The request ID.
    pub request_id: String,
    /// Current status.
    pub status: RequestStatus,
    /// Transaction ID, if signed/submitted.
    pub tx_id: Option<String>,
    /// Human-readable message.
    pub message: String,
    /// Relay URL queried.
    pub relay_url: String,
    /// Timestamp of the status check.
    pub checked_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ErgoPayClient
// ---------------------------------------------------------------------------

/// Client for building ErgoPay requests, generating URIs, and checking status.
///
/// In production this would interact with node APIs and the ErgoPay relay;
/// the current implementation is mock / simulated.
pub struct ErgoPayClient;

impl ErgoPayClient {
    /// Create a new client.
    pub fn new() -> Self {
        Self
    }

    /// Build an ErgoPay request from the given parameters.
    ///
    /// Returns a simulated [`ErgoPayRequest`] with a generated request ID and
    /// mock reduced transaction hex.
    pub fn build(&self, params: BuildRequest) -> ErgoPayRequest {
        use std::time::{SystemTime, UNIX_EPOCH};

        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        let request_id = format!("{:016x}", seed);

        // Mock reduced transaction hex (placeholder).
        let reduced_tx_hex = format!(
            "{}{}{:016x}",
            &params.address.as_bytes().iter().map(|b| format!("{:02x}", b)).collect::<String>(),
            &params.recipient.as_bytes().iter().map(|b| format!("{:02x}", b)).collect::<String>(),
            params.amount,
        );

        ErgoPayRequest {
            request_id,
            params,
            reduced_tx_hex,
            created_at: Utc::now(),
        }
    }

    /// Generate an `ergopay:` URI from a request ID and endpoint URL.
    ///
    /// If `endpoint_url` is provided the URI uses dynamic mode; otherwise it
    /// falls back to a static (base64) URI.
    pub fn uri(&self, req: UriRequest) -> UriResult {
        let is_dynamic = !req.endpoint_url.is_empty();

        let uri = if is_dynamic {
            // Dynamic ErgoPay URI.
            let path = req
                .endpoint_url
                .trim_end_matches('/')
                .trim_end_matches("/ergo-pay");
            format!("ergopay:{}/ergo-pay/{}", path, req.request_id)
        } else {
            // Static fallback: encode the request ID as a mock base64 payload.
            let encoded = base16_encode(&req.request_id);
            format!("ergopay:{}", encoded)
        };

        UriResult {
            uri,
            request_id: req.request_id,
            endpoint_url: req.endpoint_url,
            is_dynamic,
            generated_at: Utc::now(),
        }
    }

    /// Check the status of an ErgoPay request.
    ///
    /// Returns a mock [`StatusResult`]. In production this would query the
    /// relay URL.
    pub fn status(&self, req: StatusRequest) -> StatusResult {
        // Mock: always return AwaitingSignature for demonstration.
        StatusResult {
            request_id: req.request_id,
            status: RequestStatus::AwaitingSignature,
            tx_id: None,
            message: String::from("Request is awaiting wallet signature."),
            relay_url: req.relay_url,
            checked_at: Utc::now(),
        }
    }
}

impl Default for ErgoPayClient {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Simple base16 (hex) encoding — avoids pulling in a data-encoding crate.
fn base16_encode(input: &str) -> String {
    input.bytes().map(|b| format!("{:02x}", b)).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_default_values() {
        let req = BuildRequest::default();
        assert!(req.address.is_empty());
        assert_eq!(req.amount, 0);
        assert!(req.recipient.is_empty());
        assert_eq!(req.request_type, ErgoPayRequestType::Payment);
    }

    #[test]
    fn build_request_type_from_str() {
        assert_eq!(
            "staking".parse::<ErgoPayRequestType>().unwrap(),
            ErgoPayRequestType::Staking,
        );
        assert_eq!(
            "payment".parse::<ErgoPayRequestType>().unwrap(),
            ErgoPayRequestType::Payment,
        );
        assert_eq!(
            "provider".parse::<ErgoPayRequestType>().unwrap(),
            ErgoPayRequestType::Provider,
        );
        assert!("unknown".parse::<ErgoPayRequestType>().is_err());
    }

    #[test]
    fn build_request_type_display() {
        assert_eq!(ErgoPayRequestType::Staking.to_string(), "staking");
        assert_eq!(ErgoPayRequestType::Payment.to_string(), "payment");
        assert_eq!(ErgoPayRequestType::Provider.to_string(), "provider");
    }

    #[test]
    fn client_build_returns_request() {
        let client = ErgoPayClient::new();
        let params = BuildRequest {
            request_type: ErgoPayRequestType::Payment,
            address: "9hEQhmYXqBHfRho6GJHV2PBXwTWSE3T4mNFMGDAfeNNCuTzfd3s".into(),
            amount: 100_000_000,
            recipient: "3WvsT8GhFGMYiEPtCvPNvYcE24Kk1of8rRh".into(),
        };
        let req = client.build(params);

        assert!(!req.request_id.is_empty());
        assert!(!req.reduced_tx_hex.is_empty());
    }

    #[test]
    fn client_uri_dynamic() {
        let client = ErgoPayClient::new();
        let req = UriRequest {
            request_id: "abc123".into(),
            endpoint_url: "https://example.com/api".into(),
        };
        let result = client.uri(req);

        assert!(result.uri.starts_with("ergopay:"));
        assert!(result.is_dynamic);
        assert!(result.uri.contains("abc123"));
    }

    #[test]
    fn client_uri_static_fallback() {
        let client = ErgoPayClient::new();
        let req = UriRequest {
            request_id: "test".into(),
            endpoint_url: String::new(),
        };
        let result = client.uri(req);

        assert!(!result.is_dynamic);
        assert!(result.uri.starts_with("ergopay:"));
    }

    #[test]
    fn client_status_returns_result() {
        let client = ErgoPayClient::new();
        let req = StatusRequest {
            request_id: "req001".into(),
            relay_url: "https://relay.ergoplatform.com".into(),
        };
        let result = client.status(req);

        assert_eq!(result.request_id, "req001");
        assert_eq!(result.status, RequestStatus::AwaitingSignature);
        assert!(result.tx_id.is_none());
    }

    #[test]
    fn build_request_serialize_deserialize() {
        let req = BuildRequest {
            request_type: ErgoPayRequestType::Staking,
            address: "9hEQhmYXqBHfRho6GJHV2PBXwTWSE3T4mNFMGDAfeNNCuTzfd3s".into(),
            amount: 500_000_000,
            recipient: "3WvsT8GhFGMYiEPtCvPNvYcE24Kk1of8rRh".into(),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let restored: BuildRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.request_type, ErgoPayRequestType::Staking);
        assert_eq!(restored.amount, 500_000_000);
    }

    #[test]
    fn request_status_display() {
        assert_eq!(RequestStatus::Pending.to_string(), "Pending");
        assert_eq!(RequestStatus::Confirmed.to_string(), "Confirmed");
        assert_eq!(RequestStatus::Failed.to_string(), "Failed");
    }

    #[test]
    fn uri_request_default_values() {
        let req = UriRequest::default();
        assert!(req.request_id.is_empty());
        assert!(!req.endpoint_url.is_empty());
    }

    #[test]
    fn status_request_default_values() {
        let req = StatusRequest::default();
        assert!(req.request_id.is_empty());
        assert!(!req.relay_url.is_empty());
    }
}
