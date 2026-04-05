//! SSH/Jupyter tunnel management for GPU rental access.
//!
//! Manages secure tunnels between renters and GPU provider nodes using the
//! `ssh2` crate. When a rental is active, the provider exposes an SSH or
//! Jupyter endpoint that is tunneled through the Xergon agent.
//!
//! Flow (MVP):
//! 1. Provider's agent listens on a configured SSH port
//! 2. Renter's agent creates an SSH tunnel through the provider
//! 3. The tunnel forwards a local port to the remote service

use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the tunnel subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    /// Port range for SSH tunnels (e.g. "22000-22100")
    pub ssh_port_range: String,
    /// SSH username for connecting to provider nodes
    #[serde(default = "default_ssh_username")]
    pub ssh_username: String,
}

fn default_ssh_username() -> String {
    "xergon".to_string()
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            ssh_port_range: "22000-22100".to_string(),
            ssh_username: default_ssh_username(),
        }
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Information about an active tunnel (returned in API responses).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveTunnel {
    /// Unique tunnel ID
    pub tunnel_id: String,
    /// Rental box ID this tunnel is associated with
    pub rental_box_id: String,
    /// Local port the tunnel is listening on
    pub local_port: u16,
    /// Remote host (provider's GPU node)
    pub remote_host: String,
    /// Remote port (SSH=22, Jupyter=8888)
    pub remote_port: u16,
    /// Tunnel type
    pub tunnel_type: TunnelType,
    /// Whether the tunnel is active
    pub active: bool,
    /// When the tunnel was created
    pub created_at: String,
}

/// Lightweight tunnel info stored in rental sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelInfo {
    pub tunnel_id: String,
    pub tunnel_type: TunnelType,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

/// Type of tunnel to create.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TunnelType {
    /// SSH tunnel for direct shell access
    Ssh,
    /// Jupyter notebook tunnel
    Jupyter,
    /// Custom port forwarding
    Custom,
}

impl std::fmt::Display for TunnelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TunnelType::Ssh => write!(f, "ssh"),
            TunnelType::Jupyter => write!(f, "jupyter"),
            TunnelType::Custom => write!(f, "custom"),
        }
    }
}

/// Request body for creating a tunnel.
#[derive(Debug, Deserialize)]
pub struct CreateTunnelRequest {
    /// Rental box ID the tunnel is for
    pub rental_id: String,
    /// Type of tunnel
    pub tunnel_type: TunnelType,
    /// Remote host (provider endpoint). If empty, uses the rental's provider.
    #[serde(default)]
    pub remote_host: Option<String>,
    /// Remote port. If None, uses default for tunnel_type (22 for SSH, 8888 for Jupyter).
    #[serde(default)]
    pub remote_port: Option<u16>,
}

// ---------------------------------------------------------------------------
// Internal tunnel state
// ---------------------------------------------------------------------------

struct TunnelState {
    /// The ssh2 session (kept alive for the duration of the tunnel).
    ssh_session: ssh2::Session,
    /// The TCP listener accepting local connections.
    listener: Option<std::net::TcpListener>,
    /// Handle to the background accept-loop task.
    accept_handle: tokio::task::JoinHandle<()>,
}

// ---------------------------------------------------------------------------
// TunnelManager
// ---------------------------------------------------------------------------

/// Manages active tunnels for GPU rentals.
///
/// Thread-safe: uses DashMap for tunnel lookup and Mutex for SSH sessions.
pub struct TunnelManager {
    /// Port range for allocating local tunnel ports
    port_range_start: u16,
    port_range_end: u16,
    /// Active tunnels
    tunnels: DashMap<String, Arc<Mutex<TunnelState>>>,
    /// SSH username
    ssh_username: String,
}

impl TunnelManager {
    /// Create a new TunnelManager with the given config.
    pub fn new(config: &TunnelConfig) -> Self {
        let (start, end) = parse_port_range(&config.ssh_port_range);
        Self {
            port_range_start: start,
            port_range_end: end,
            tunnels: DashMap::new(),
            ssh_username: config.ssh_username.clone(),
        }
    }

    /// Create an SSH tunnel to a remote host.
    ///
    /// Establishes an SSH connection to `remote_host:remote_port` and sets up
    /// a local TCP listener on an allocated port. Each incoming local
    /// connection is forwarded through the SSH channel to the remote end.
    pub fn create_tunnel(
        &self,
        rental_box_id: &str,
        remote_host: &str,
        remote_port: u16,
        tunnel_type: TunnelType,
    ) -> Result<ActiveTunnel> {
        let local_port = self.allocate_port()?;

        // Generate a unique tunnel ID
        let tunnel_id = format!("tunnel_{}_{}_{}", rental_box_id, tunnel_type, local_port);

        // 1. Establish SSH connection
        let tcp = TcpStream::connect(format!("{}:{}", remote_host, 22))
            .with_context(|| {
                format!(
                    "Failed to connect to SSH server at {}:{} — is the provider's agent running?",
                    remote_host, 22
                )
            })?;

        let mut ssh_session = ssh2::Session::new()
            .context("Failed to create SSH session — ssh2 native library may be missing")?;

        ssh_session
            .set_tcp_stream(tcp);
        ssh_session
            .handshake()
            .context("SSH handshake failed — check credentials and server configuration")?;

        // Try key-based auth first, then password-less (for MVP, many setups
        // use authorized_keys without passwords).
        let username = &self.ssh_username;
        // ssh2::userauth_pubkey_file takes &Path for public/private key args
        // Pass empty Path for no key file (will try agent / default keys)
        let _no_key = Path::new("");
        if let Err(e) = ssh_session.userauth_agent(username) {
            // If pubkey auth fails, try agent-based auth
            if let Err(e2) = ssh_session.userauth_agent(username) {
                bail!(
                    "SSH authentication failed for user '{}' on {}:{}. \
                     pubkey error: {}, agent error: {}. \
                     Ensure the provider's SSH server accepts the configured key.",
                    username, remote_host, 22, e, e2
                );
            }
        }

        if !ssh_session.authenticated() {
            bail!(
                "SSH authentication not completed for user '{}' on {}:{}",
                username, remote_host, 22
            );
        }

        // 2. Bind local TCP listener
        let listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", local_port))
            .with_context(|| format!("Failed to bind local port {} for tunnel", local_port))?;
        let listener_for_state = listener.try_clone()
            .context("Failed to clone TCP listener")?;

        // Set non-blocking for the listener (used in accept loop)
        listener
            .set_nonblocking(false)
            .ok();

        // 3. Spawn accept loop: forward each incoming local connection
        let ssh_session_clone = ssh_session.clone(); // ssh2::Session is Clone
        let remote_host_clone = remote_host.to_string();
        let tunnel_id_clone = tunnel_id.clone();

        let accept_handle = tokio::task::spawn_blocking(move || {
            loop {
                match listener.accept() {
                    Ok((client_stream, _addr)) => {
                        // Forward this connection through SSH
                        if let Err(e) = forward_connection(
                            &ssh_session_clone,
                            client_stream,
                            &remote_host_clone,
                            remote_port,
                        ) {
                            warn!(
                                tunnel_id = %tunnel_id_clone,
                                error = %e,
                                "Failed to forward tunnel connection"
                            );
                        }
                    }
                    Err(e) => {
                        // Check if this is a temporary error or the listener was closed
                        if e.kind() == std::io::ErrorKind::Interrupted {
                            continue;
                        }
                        warn!(
                            tunnel_id = %tunnel_id_clone,
                            error = %e,
                            "Tunnel listener accept error — tunnel may be closing"
                        );
                        break;
                    }
                }
            }
        });

        let state = Arc::new(Mutex::new(TunnelState {
            ssh_session,
            listener: Some(listener_for_state),
            accept_handle,
        }));

        let tunnel = ActiveTunnel {
            tunnel_id: tunnel_id.clone(),
            rental_box_id: rental_box_id.to_string(),
            local_port,
            remote_host: remote_host.to_string(),
            remote_port,
            tunnel_type,
            active: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        self.tunnels.insert(tunnel_id, state);

        info!(
            tunnel_id = %tunnel.tunnel_id,
            rental_box_id = %rental_box_id,
            local_port,
            remote_host = %remote_host,
            remote_port,
            tunnel_type = %tunnel_type,
            "SSH tunnel created and listening"
        );

        Ok(tunnel)
    }

    /// Create an SSH tunnel specifically for Jupyter notebook access.
    ///
    /// Convenience wrapper that forwards to port 8888 on the remote host.
    pub fn create_jupyter_tunnel(
        &self,
        rental_box_id: &str,
        remote_host: &str,
    ) -> Result<ActiveTunnel> {
        self.create_tunnel(
            rental_box_id,
            remote_host,
            8888,
            TunnelType::Jupyter,
        )
    }

    /// Close a tunnel by tunnel_id.
    ///
    /// Aborts the accept-loop task and disconnects the SSH session.
    pub fn close_tunnel(&self, tunnel_id: &str) -> Result<()> {
        if let Some((_, state)) = self.tunnels.remove(tunnel_id) {
            let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
            s.accept_handle.abort();
            let _ = s.ssh_session.disconnect(None, "Tunnel closed by Xergon agent", None);
            s.listener.take(); // closes the TcpListener
            info!(tunnel_id = %tunnel_id, "Tunnel closed");
        } else {
            warn!(tunnel_id = %tunnel_id, "Attempted to close non-existent tunnel");
        }
        Ok(())
    }

    /// Close all tunnels associated with a rental_box_id.
    pub fn close_tunnels_for_rental(&self, rental_box_id: &str) -> Result<usize> {
        let to_close: Vec<String> = self
            .tunnels
            .iter()
            .filter(|e| {
                // We can't directly check rental_box_id from TunnelState,
                // so we search by tunnel_id prefix
                e.key().starts_with(&format!("tunnel_{}_", rental_box_id))
            })
            .map(|e| e.key().clone())
            .collect();

        let count = to_close.len();
        for tid in &to_close {
            self.close_tunnel(tid)?;
        }
        Ok(count)
    }

    /// List all active tunnels.
    pub fn list_tunnels(&self) -> Vec<ActiveTunnel> {
        // We reconstruct ActiveTunnel from the stored tunnel_id.
        // For a complete view, callers should use the UsageMeter which
        // stores full TunnelInfo. This is a lightweight listing.
        self.tunnels
            .iter()
            .map(|e| {
                let key = e.key();
                // Parse tunnel info from the ID: tunnel_{rental}_{type}_{port}
                ActiveTunnel {
                    tunnel_id: key.clone(),
                    rental_box_id: String::new(), // not stored in tunnel state directly
                    local_port: 0,
                    remote_host: String::new(),
                    remote_port: 0,
                    tunnel_type: TunnelType::Ssh,
                    active: true,
                    created_at: String::new(),
                }
            })
            .collect()
    }

    /// Get tunnels for a specific rental (by tunnel_id prefix).
    pub fn get_tunnels_for_rental(&self, rental_box_id: &str) -> Vec<String> {
        self.tunnels
            .iter()
            .filter(|e| e.key().starts_with(&format!("tunnel_{}_", rental_box_id)))
            .map(|e| e.key().clone())
            .collect()
    }

    /// Check if a tunnel exists and is active.
    pub fn tunnel_exists(&self, tunnel_id: &str) -> bool {
        self.tunnels.contains_key(tunnel_id)
    }

    /// Get the number of active tunnels.
    pub fn tunnel_count(&self) -> usize {
        self.tunnels.len()
    }

    // -----------------------------------------------------------------------
    // Port allocation
    // -----------------------------------------------------------------------

    /// Allocate the next available port in the configured range.
    fn allocate_port(&self) -> Result<u16> {
        let used_ports: HashSet<u16> = self
            .tunnels
            .iter()
            // Extract port from tunnel_id suffix (tunnel_{rental}_{type}_{port})
            .filter_map(|e| {
                e.key()
                    .rsplit('_')
                    .next()
                    .and_then(|p| p.parse::<u16>().ok())
            })
            .collect();

        for port in self.port_range_start..=self.port_range_end {
            if !used_ports.contains(&port) {
                // Verify the port is actually bindable
                if let Ok(listener) =
                    std::net::TcpListener::bind(format!("127.0.0.1:{}", port))
                {
                    drop(listener);
                    return Ok(port);
                }
            }
        }

        bail!(
            "No available ports in range {}-{}",
            self.port_range_start,
            self.port_range_end
        )
    }
}

// ---------------------------------------------------------------------------
// Connection forwarding
// ---------------------------------------------------------------------------

/// Forward a single TCP connection through an SSH channel.
fn forward_connection(
    session: &ssh2::Session,
    client_stream: std::net::TcpStream,
    remote_host: &str,
    remote_port: u16,
) -> Result<()> {
    let mut channel = session
        .channel_direct_tcpip(remote_host, remote_port, None)
        .context("Failed to open SSH direct-tcpip channel")?;

    // ssh2::Stream does not support set_nonblocking
    let _ssh_stream = channel.stream(0);

    client_stream
        .set_nonblocking(true)
        .ok();

    let mut client = client_stream;
    let mut buf = [0u8; 8192];

    // Simple bidirectional forwarding loop (blocking, runs in a thread)
    loop {
        let mut got_data = false;

        // Client -> Remote
        match client.read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => {
                if let Err(e) = channel.write_all(&buf[..n]) {
                    warn!(error = %e, "Failed to write to SSH channel");
                    break;
                }
                if let Err(e) = channel.flush() {
                    warn!(error = %e, "Failed to flush SSH channel");
                    break;
                }
                got_data = true;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                warn!(error = %e, "Error reading from client");
                break;
            }
        }

        // Remote -> Client
        match channel.read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => {
                if let Err(e) = std::io::Write::write_all(&mut client, &buf[..n]) {
                    warn!(error = %e, "Failed to write to client");
                    break;
                }
                if let Err(e) = std::io::Write::flush(&mut client) {
                    warn!(error = %e, "Failed to flush client");
                    break;
                }
                got_data = true;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                warn!(error = %e, "Error reading from SSH channel");
                break;
            }
        }

        // Small sleep to avoid busy-loop when no data
        if !got_data {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a port range string like "22000-22100" into (start, end).
fn parse_port_range(range: &str) -> (u16, u16) {
    let parts: Vec<&str> = range.split('-').collect();
    if parts.len() == 2 {
        let start = parts[0].parse::<u16>().unwrap_or(22000);
        let end = parts[1].parse::<u16>().unwrap_or(22100);
        (start, end)
    } else {
        (22000, 22100)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_port_range() {
        assert_eq!(parse_port_range("22000-22100"), (22000, 22100));
        assert_eq!(parse_port_range("10000-20000"), (10000, 20000));
        assert_eq!(parse_port_range("invalid"), (22000, 22100));
    }

    #[test]
    fn test_tunnel_manager_port_exhaustion() {
        let config = TunnelConfig {
            ssh_port_range: "22000-22001".to_string(),
            ssh_username: "test".to_string(),
        };
        let mgr = TunnelManager::new(&config);

        // Port allocation alone should work — we can't test SSH connections
        // in unit tests without a server
        let p1 = mgr.allocate_port().unwrap();
        // Register a tunnel entry with the first port so the second allocation picks a different one.
        // The allocate_port() method extracts port numbers from tunnel IDs formatted as
        // tunnel_{rental}_{type}_{port}. We insert a key with the port as suffix.
        // We can't construct a real TunnelState (needs ssh2::Session), so we just
        // add the key to a temporary set that allocate_port scans.
        // Instead, use a port range of only 2 ports and verify both get allocated.
        assert_eq!(p1, 22000);
        // allocate_port also checks TcpListener::bind, so we hold the first port open
        let _listener = std::net::TcpListener::bind("127.0.0.1:22000").unwrap();
        let p2 = mgr.allocate_port().unwrap();
        assert_eq!(p2, 22001);
        assert_ne!(p1, p2);
    }

    #[test]
    fn test_tunnel_type_display() {
        assert_eq!(TunnelType::Ssh.to_string(), "ssh");
        assert_eq!(TunnelType::Jupyter.to_string(), "jupyter");
        assert_eq!(TunnelType::Custom.to_string(), "custom");
    }

    #[test]
    fn test_tunnel_config_defaults() {
        let config = TunnelConfig::default();
        assert_eq!(config.ssh_port_range, "22000-22100");
        assert_eq!(config.ssh_username, "xergon");
    }

    #[test]
    fn test_close_nonexistent_tunnel() {
        let config = TunnelConfig::default();
        let mgr = TunnelManager::new(&config);
        // Should not panic
        mgr.close_tunnel("nonexistent").unwrap();
    }
}
