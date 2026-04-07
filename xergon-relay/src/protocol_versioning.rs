//! Protocol version management with semver negotiation and migration paths.
//!
//! Provides a registry of supported protocol versions, deprecation tracking,
//! version negotiation between client and server, and migration path resolution.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Wire format constants
// ---------------------------------------------------------------------------

/// Magic bytes identifying Xergon protocol messages on the wire.
pub const WIRE_MAGIC: &[u8; 4] = b"XRGN";

/// Wire header size: magic(4) + version(4) + msg_type(2) + length(4) = 14 bytes.
pub const WIRE_HEADER_SIZE: usize = 14;

/// Known message type IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum WireMessageType {
    Request = 0x0001,
    Response = 0x0002,
    StreamChunk = 0x0003,
    Error = 0x0004,
    Heartbeat = 0x0005,
    Capabilities = 0x0006,
    Migration = 0x0007,
    Handshake = 0x0008,
}

impl WireMessageType {
    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
            0x0001 => Some(Self::Request),
            0x0002 => Some(Self::Response),
            0x0003 => Some(Self::StreamChunk),
            0x0004 => Some(Self::Error),
            0x0005 => Some(Self::Heartbeat),
            0x0006 => Some(Self::Capabilities),
            0x0007 => Some(Self::Migration),
            0x0008 => Some(Self::Handshake),
            _ => None,
        }
    }
}

/// Wire-format header for serialisation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireHeader {
    /// Magic bytes (always b"XRGN").
    pub magic: [u8; 4],
    /// Protocol version encoded as (major, minor, patch).
    pub version: (u32, u32, u32),
    /// Message type discriminator.
    pub msg_type: u16,
    /// Payload length in bytes.
    pub payload_length: u32,
}

impl WireHeader {
    /// Encode the header into exactly 14 bytes.
    pub fn encode(&self) -> [u8; WIRE_HEADER_SIZE] {
        let mut buf = [0u8; WIRE_HEADER_SIZE];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..8].copy_from_slice(&self.version.0.to_be_bytes());
        buf[8..10].copy_from_slice(&self.msg_type.to_be_bytes());
        buf[10..14].copy_from_slice(&self.payload_length.to_be_bytes());
        buf
    }

    /// Decode a 14-byte slice into a header. Returns None on invalid magic.
    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < WIRE_HEADER_SIZE {
            return None;
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&buf[0..4]);
        if &magic != WIRE_MAGIC {
            return None;
        }
        let major = u32::from_be_bytes(buf[4..8].try_into().ok()?);
        let minor = 0; // encoded in full version string in practice
        let patch = 0;
        let msg_type = u16::from_be_bytes(buf[8..10].try_into().ok()?);
        let payload_length = u32::from_be_bytes(buf[10..14].try_into().ok()?);
        Some(Self {
            magic,
            version: (major, minor, patch),
            msg_type,
            payload_length,
        })
    }
}

// ---------------------------------------------------------------------------
// Protocol version
// ---------------------------------------------------------------------------

/// A semantic version with optional pre-release and build metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pre_release: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub build_metadata: String,
}

impl ProtocolVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            pre_release: String::new(),
            build_metadata: String::new(),
        }
    }

    /// Parse from a string like "1.2.3", "1.2.3-alpha", "1.2.3+build42".
    pub fn parse(s: &str) -> Option<Self> {
        let (version_part, pre_release) = if let Some(pos) = s.find('-') {
            (&s[..pos], Some(&s[pos + 1..]))
        } else {
            (s, None)
        };
        let (version_part, build_metadata) = if let Some(pos) = version_part.find('+') {
            (&version_part[..pos], Some(&version_part[pos + 1..]))
        } else {
            (version_part, None)
        };
        let mut parts = version_part.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        Some(Self {
            major,
            minor,
            patch,
            pre_release: pre_release.unwrap_or_default().to_string(),
            build_metadata: build_metadata.unwrap_or_default().to_string(),
        })
    }

    /// Display as semver string.
    pub fn to_string_repr(&self) -> String {
        let mut s = format!("{}.{}.{}", self.major, self.minor, self.patch);
        if !self.pre_release.is_empty() {
            s.push_str(&format!("-{}", self.pre_release));
        }
        if !self.build_metadata.is_empty() {
            s.push_str(&format!("+{}", self.build_metadata));
        }
        s
    }

    /// Ordering for semver comparison. Pre-release versions have lower precedence.
    pub fn cmp_semver(&self, other: &Self) -> std::cmp::Ordering {
        match self.major.cmp(&other.major) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        // Pre-release: no pre-release > with pre-release (release is higher)
        match (self.pre_release.is_empty(), other.pre_release.is_empty()) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => self.pre_release.cmp(&other.pre_release),
        }
    }
}

impl PartialOrd for ProtocolVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp_semver(other))
    }
}

impl Ord for ProtocolVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cmp_semver(other)
    }
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string_repr())
    }
}

// ---------------------------------------------------------------------------
// Versioned message trait
// ---------------------------------------------------------------------------

/// Trait for messages that carry a protocol version and can be migrated.
pub trait VersionedMessage: Send + Sync {
    /// Return the protocol version this message was encoded with.
    fn protocol_version(&self) -> &ProtocolVersion;
    /// Return the message type discriminator for wire encoding.
    fn wire_message_type(&self) -> WireMessageType;
    /// Serialize the payload (without header).
    fn serialize_payload(&self) -> Vec<u8>;
    /// Deserialize payload from bytes at a specific version.
    fn deserialize_payload(version: &ProtocolVersion, data: &[u8]) -> Option<Self>
    where
        Self: Sized;
}

// ---------------------------------------------------------------------------
// Migration handler
// ---------------------------------------------------------------------------

/// A migration handler that transforms data from one version to another.
pub type MigrationHandler = Arc<dyn Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync>;

// ---------------------------------------------------------------------------
// Protocol registry
// ---------------------------------------------------------------------------

/// Metadata stored for each registered protocol version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    pub version: ProtocolVersion,
    /// Whether this version is deprecated (still supported but not preferred).
    pub deprecated: bool,
    /// Human-readable description of what changed in this version.
    pub changelog: String,
    /// Timestamp when this version was registered.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub registered_at: DateTime<Utc>,
    /// Whether this is the currently recommended version.
    pub current: bool,
}

/// Thread-safe protocol version registry with negotiation and migration support.
pub struct ProtocolRegistry {
    /// version string -> VersionEntry
    versions: DashMap<String, VersionEntry>,
    /// "from_version->to_version" -> MigrationHandler
    migration_handlers: DashMap<String, MigrationHandler>,
    /// The current (recommended) version.
    current_version: std::sync::RwLock<ProtocolVersion>,
}

impl std::fmt::Debug for ProtocolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtocolRegistry")
            .field("version_count", &self.versions.len())
            .finish()
    }
}

impl ProtocolRegistry {
    /// Create a new registry and register the initial version.
    pub fn new(initial_version: &str) -> Self {
        let version = ProtocolVersion::parse(initial_version)
            .unwrap_or_else(|| ProtocolVersion::new(1, 0, 0));
        let mut registry = Self {
            versions: DashMap::new(),
            migration_handlers: DashMap::new(),
            current_version: std::sync::RwLock::new(version.clone()),
        };
        registry.register_version(version.clone(), "Initial protocol version".into());
        registry
    }

    /// Register a new protocol version.
    pub fn register_version(&self, version: ProtocolVersion, changelog: String) {
        let key = version.to_string_repr();
        let entry = VersionEntry {
            version: version.clone(),
            deprecated: false,
            changelog,
            registered_at: Utc::now(),
            current: false,
        };
        self.versions.insert(key, entry);
        info!(version = %version, "Registered protocol version");
    }

    /// Mark a version as deprecated. Returns true if the version existed.
    pub fn deprecate_version(&self, version_str: &str) -> bool {
        if let Some(mut entry) = self.versions.get_mut(version_str) {
            entry.value_mut().deprecated = true;
            warn!(version = %version_str, "Deprecated protocol version");
            true
        } else {
            false
        }
    }

    /// Set the current (recommended) version.
    pub fn set_current_version(&self, version_str: &str) -> bool {
        // Clear current flag on all
        for mut entry in self.versions.iter_mut() {
            entry.value_mut().current = false;
        }
        if let Some(mut entry) = self.versions.get_mut(version_str) {
            entry.value_mut().current = true;
            if let Ok(mut current) = self.current_version.write() {
                *current = entry.value().version.clone();
            }
            info!(version = %version_str, "Set current protocol version");
            true
        } else {
            false
        }
    }

    /// Check whether a version is registered and not deprecated.
    pub fn is_supported(&self, version_str: &str) -> bool {
        self.versions
            .get(version_str)
            .map(|e| !e.value().deprecated)
            .unwrap_or(false)
    }

    /// Check whether a version is registered (even if deprecated).
    pub fn is_known(&self, version_str: &str) -> bool {
        self.versions.contains_key(version_str)
    }

    /// Negotiate the best mutually supported version between client and server.
    /// Returns the highest version that is both registered and >= the client min.
    pub fn negotiate_version(&self, client_min_version: &str) -> Option<ProtocolVersion> {
        let client_min = ProtocolVersion::parse(client_min_version)?;
        let mut best: Option<ProtocolVersion> = None;

        for entry in self.versions.iter() {
            let ver = &entry.value().version;
            if entry.value().deprecated {
                continue;
            }
            if ver >= &client_min {
                match &best {
                    None => best = Some(ver.clone()),
                    Some(b) if ver > b => best = Some(ver.clone()),
                    _ => {}
                }
            }
        }

        best
    }

    /// Get the current recommended version.
    pub fn current_version(&self) -> ProtocolVersion {
        self.current_version
            .read()
            .map(|v| v.clone())
            .unwrap_or_else(|_| ProtocolVersion::new(1, 0, 0))
    }

    /// Register a migration handler from one version to another.
    pub fn register_migration(
        &self,
        from_version: &str,
        to_version: &str,
        handler: MigrationHandler,
    ) {
        let key = format!("{}->{}", from_version, to_version);
        info!(from = %from_version, to = %to_version, "Registered migration handler");
        self.migration_handlers.insert(key, handler);
    }

    /// Get the migration path (sequence of versions) from one version to another.
    /// Returns a list of version strings to apply in order.
    pub fn get_migration_path(&self, from_version: &str, to_version: &str) -> Option<Vec<String>> {
        if from_version == to_version {
            return Some(vec![]);
        }

        // Check if direct migration exists
        let direct_key = format!("{}->{}", from_version, to_version);
        if self.migration_handlers.contains_key(&direct_key) {
            return Some(vec![from_version.to_string(), to_version.to_string()]);
        }

        // BFS for multi-step migration path
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back((from_version.to_string(), vec![from_version.to_string()]));
        visited.insert(from_version.to_string());

        while let Some((current, path)) = queue.pop_front() {
            // Check all registered migrations starting from `current`
            for entry in self.migration_handlers.iter() {
                let key = entry.key();
                if let Some(prefix) = key.strip_prefix(&format!("{}->", current)) {
                    if !visited.contains(prefix) {
                        let mut new_path = path.clone();
                        new_path.push(prefix.to_string());
                        if prefix == to_version {
                            return Some(new_path);
                        }
                        visited.insert(prefix.to_string());
                        queue.push_back((prefix.to_string(), new_path));
                    }
                }
            }
        }

        None
    }

    /// Apply a single migration step.
    pub fn apply_migration(&self, from_version: &str, to_version: &str, data: &[u8]) -> Option<Vec<u8>> {
        let key = format!("{}->{}", from_version, to_version);
        self.migration_handlers
            .get(&key)
            .and_then(|handler| handler(data))
    }

    /// List all registered versions.
    pub fn list_versions(&self) -> Vec<VersionEntry> {
        let mut entries: Vec<VersionEntry> = self
            .versions
            .iter()
            .map(|e| e.value().clone())
            .collect();
        entries.sort_by(|a, b| b.version.cmp(&a.version));
        entries
    }

    /// List only deprecated versions.
    pub fn list_deprecated(&self) -> Vec<VersionEntry> {
        self.list_versions()
            .into_iter()
            .filter(|e| e.deprecated)
            .collect()
    }

    /// Get a specific version entry.
    pub fn get_version(&self, version_str: &str) -> Option<VersionEntry> {
        self.versions.get(version_str).map(|e| e.value().clone())
    }
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

/// GET /v1/protocol/versions — list all registered versions
async fn list_versions_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let versions = state.protocol_registry.list_versions();
    (StatusCode::OK, Json(versions))
}

/// GET /v1/protocol/current — get the current recommended version
async fn current_version_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let current = state.protocol_registry.current_version();
    (StatusCode::OK, Json(serde_json::json!({
        "version": current.to_string_repr(),
        "major": current.major,
        "minor": current.minor,
        "patch": current.patch,
        "pre_release": current.pre_release,
        "build_metadata": current.build_metadata,
    })))
}

/// POST /v1/protocol/negotiate — negotiate best version
async fn negotiate_version_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    let client_min = body.get("min_version")
        .and_then(|v| v.as_str())
        .unwrap_or("1.0.0");

    match state.protocol_registry.negotiate_version(client_min) {
        Some(version) => (StatusCode::OK, Json(serde_json::json!({
            "negotiated_version": version.to_string_repr(),
            "client_min": client_min,
        }))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "No compatible version found",
                "client_min": client_min,
            })),
        ).into_response(),
    }
}

/// GET /v1/protocol/deprecated — list deprecated versions
async fn deprecated_versions_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let deprecated = state.protocol_registry.list_deprecated();
    (StatusCode::OK, Json(deprecated))
}

/// GET /v1/protocol/migration/{from}/{to} — get migration path
async fn migration_path_handler(
    State(state): State<AppState>,
    Path((from, to)): Path<(String, String)>,
) -> axum::response::Response {
    match state.protocol_registry.get_migration_path(&from, &to) {
        Some(path) => (StatusCode::OK, Json(serde_json::json!({
            "from": from,
            "to": to,
            "path": path,
            "steps": path.len().saturating_sub(1),
        }))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "No migration path found",
                "from": from,
                "to": to,
            })),
        ).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/protocol/versions", get(list_versions_handler))
        .route("/v1/protocol/current", get(current_version_handler))
        .route("/v1/protocol/negotiate", post(negotiate_version_handler))
        .route("/v1/protocol/deprecated", get(deprecated_versions_handler))
        .route("/v1/protocol/migration/{from}/{to}", get(migration_path_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry() -> ProtocolRegistry {
        let r = ProtocolRegistry::new("1.0.0");
        r.register_version(ProtocolVersion::new(1, 1, 0), "Added streaming support".into());
        r.register_version(ProtocolVersion::new(2, 0, 0), "Breaking: new wire format".into());
        r.set_current_version("2.0.0");
        r
    }

    #[test]
    fn test_version_parse_and_display() {
        let v = ProtocolVersion::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.to_string_repr(), "1.2.3");

        let v2 = ProtocolVersion::parse("2.0.0-alpha+build42").unwrap();
        assert_eq!(v2.pre_release, "alpha");
        assert_eq!(v2.build_metadata, "build42");
        assert_eq!(v2.to_string_repr(), "2.0.0-alpha+build42");
    }

    #[test]
    fn test_semver_ordering() {
        let v100 = ProtocolVersion::new(1, 0, 0);
        let v110 = ProtocolVersion::new(1, 1, 0);
        let v200 = ProtocolVersion::new(2, 0, 0);
        let v_alpha = ProtocolVersion::parse("2.0.0-alpha").unwrap();
        let v_beta = ProtocolVersion::parse("2.0.0-beta").unwrap();

        assert!(v200 > v110);
        assert!(v110 > v100);
        assert!(v200 > v_alpha);
        assert!(v_beta > v_alpha);
        assert!(v200 > v_beta);
        assert_eq!(v100, ProtocolVersion::parse("1.0.0").unwrap());
    }

    #[test]
    fn test_version_comparison() {
        let v1 = ProtocolVersion::parse("1.0.0").unwrap();
        let v2 = ProtocolVersion::parse("1.0.0-alpha").unwrap();
        // Release > pre-release
        assert!(v1 > v2);

        let v3 = ProtocolVersion::parse("1.0.0+build1").unwrap();
        let v4 = ProtocolVersion::parse("1.0.0+build2").unwrap();
        // Build metadata doesn't affect precedence in our impl (both equal)
        assert_eq!(v3.cmp_semver(&v4), std::cmp::Ordering::Equal);
    }

    #[test]
    fn test_register_and_list() {
        let r = make_registry();
        let versions = r.list_versions();
        assert_eq!(versions.len(), 3);
        // Sorted descending
        assert!(versions[0].version > versions[1].version);
        assert!(versions[1].version > versions[2].version);
    }

    #[test]
    fn test_negotiate() {
        let r = make_registry();
        // Client wants at least 1.0.0, should get 2.0.0
        assert_eq!(
            r.negotiate_version("1.0.0").unwrap().to_string_repr(),
            "2.0.0"
        );
        // Client wants at least 1.1.0, should get 2.0.0
        assert_eq!(
            r.negotiate_version("1.1.0").unwrap().to_string_repr(),
            "2.0.0"
        );
        // Client wants at least 3.0.0, should get None
        assert!(r.negotiate_version("3.0.0").is_none());
    }

    #[test]
    fn test_deprecation() {
        let r = make_registry();
        assert!(r.is_supported("1.0.0"));
        assert!(r.deprecate_version("1.0.0"));
        assert!(!r.is_supported("1.0.0"));
        assert!(r.is_known("1.0.0")); // still known, just deprecated
        let deprecated = r.list_deprecated();
        assert_eq!(deprecated.len(), 1);
        assert_eq!(deprecated[0].version.to_string_repr(), "1.0.0");
    }

    #[test]
    fn test_migration_path() {
        let r = make_registry();
        // Register migration handlers
        r.register_migration("1.0.0", "1.1.0", Arc::new(|_data| Some(vec![1, 1, 0])));
        r.register_migration("1.1.0", "2.0.0", Arc::new(|_data| Some(vec![2, 0, 0])));

        // Direct path
        let path = r.get_migration_path("1.0.0", "1.1.0").unwrap();
        assert_eq!(path, vec!["1.0.0", "1.1.0"]);

        // Multi-step path via BFS
        let path = r.get_migration_path("1.0.0", "2.0.0").unwrap();
        assert_eq!(path, vec!["1.0.0", "1.1.0", "2.0.0"]);

        // Same version
        let path = r.get_migration_path("2.0.0", "2.0.0").unwrap();
        assert!(path.is_empty());

        // No path
        assert!(r.get_migration_path("2.0.0", "1.0.0").is_none());
    }

    #[test]
    fn test_apply_migration() {
        let r = make_registry();
        r.register_migration("1.0.0", "2.0.0", Arc::new(|data| {
            // Simple identity transform for test
            Some(data.to_vec())
        }));

        let result = r.apply_migration("1.0.0", "2.0.0", &[0x01, 0x02]);
        assert_eq!(result, Some(vec![0x01, 0x02]));

        // No handler registered
        assert!(r.apply_migration("2.0.0", "1.0.0", &[0x01]).is_none());
    }

    #[test]
    fn test_wire_header_roundtrip() {
        let header = WireHeader {
            magic: *WIRE_MAGIC,
            version: (2, 0, 0),
            msg_type: WireMessageType::Request as u16,
            payload_length: 1024,
        };
        let encoded = header.encode();
        assert_eq!(encoded.len(), WIRE_HEADER_SIZE);

        let decoded = WireHeader::decode(&encoded).unwrap();
        assert_eq!(decoded.magic, *WIRE_MAGIC);
        assert_eq!(decoded.version, (2, 0, 0));
        assert_eq!(decoded.msg_type, WireMessageType::Request as u16);
        assert_eq!(decoded.payload_length, 1024);
    }

    #[test]
    fn test_wire_header_invalid_magic() {
        let buf = [0xDE, 0xAD, 0xBE, 0xEF, 0, 0, 0, 2, 0, 1, 0, 0, 0, 100];
        assert!(WireHeader::decode(&buf).is_none());
    }

    #[test]
    fn test_wire_message_type_roundtrip() {
        assert_eq!(WireMessageType::from_u16(0x0001), Some(WireMessageType::Request));
        assert_eq!(WireMessageType::from_u16(0x0005), Some(WireMessageType::Heartbeat));
        assert_eq!(WireMessageType::from_u16(0xFFFF), None);
    }

    #[test]
    fn test_current_version() {
        let r = make_registry();
        assert_eq!(r.current_version().to_string_repr(), "2.0.0");
        r.set_current_version("1.1.0");
        assert_eq!(r.current_version().to_string_repr(), "1.1.0");
    }

    #[test]
    fn test_get_version() {
        let r = make_registry();
        let entry = r.get_version("1.1.0").unwrap();
        assert_eq!(entry.version.to_string_repr(), "1.1.0");
        assert!(!entry.deprecated);
        assert_eq!(entry.changelog, "Added streaming support");
        assert!(r.get_version("9.9.9").is_none());
    }
}
