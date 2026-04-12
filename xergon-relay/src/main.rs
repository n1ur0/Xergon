mod auth;
mod config;
mod handlers;
mod heartbeat;
mod provider;
mod registration;
mod settlement;
mod types;

use axum::serve;
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::Config;
use crate::handlers::create_router;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
    
    let config = Config::load(&config_path).unwrap_or_else(|e| {
        eprintln!("Failed to load config from {}: {}", config_path, e);
        eprintln!("Using default configuration...");
        Config {
            server: crate::config::ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 9090,
            },
            providers: vec![],
        }
    });

    // Create router
    let app = create_router(config.clone());

    // Start server
    let host_ip: std::net::IpAddr = config.server.host
        .parse()
        .unwrap_or(std::net::Ipv4Addr::UNSPECIFIED.into());
    let addr = SocketAddr::from((host_ip, config.server.port));

    info!("Starting Xergon Relay on {}", addr);
    info!("Endpoints:");
    info!("  POST /register - Register a new provider");
    info!("  POST /heartbeat - Send heartbeat from provider");
    info!("  GET /providers - List registered providers");
    info!("  POST /v1/chat/completions - Chat completions (requires API key)");
    info!("  GET /health - Health check");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    serve(listener, app).await.unwrap();
}
