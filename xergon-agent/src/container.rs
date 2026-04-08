//! Container runtime management for the Xergon agent.
//!
//! Manages Docker containers for model serving, including lifecycle,
//! GPU passthrough, port mapping, health checks, and log streaming.
//!
//! API:
//! - POST   /api/containers/create    -- create and start container
//! - GET    /api/containers           -- list containers
//! - GET    /api/containers/{id}      -- container details
//! - POST   /api/containers/{id}/stop -- stop container
//! - POST   /api/containers/{id}/start -- start stopped container
//! - DELETE /api/containers/{id}      -- remove container
//! - GET    /api/containers/{id}/logs -- stream container logs

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ContainerStatus {
    Creating,
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    pub image: String,
    pub model: String,
    pub gpu_device: Option<u32>,
    pub port: u16,
    pub env: HashMap<String, String>,
    pub memory_limit: String,
    pub cpu_limit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    pub id: String,
    pub config: ContainerConfig,
    pub status: ContainerStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub health_status: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateContainerRequest {
    pub image: String,
    pub model: String,
    pub gpu_device: Option<u32>,
    pub port: Option<u16>,
    pub env: Option<HashMap<String, String>>,
    pub memory_limit: Option<String>,
    pub cpu_limit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerLogsQuery {
    pub tail: Option<u32>,
    pub follow: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerHealthCheck {
    pub container_id: String,
    pub status: String,
    pub last_check: DateTime<Utc>,
    pub response_time_ms: Option<f64>,
}

// ---------------------------------------------------------------------------
// Container manager
// ---------------------------------------------------------------------------

const DEFAULT_PORT_RANGE_START: u16 = 9000;
const DEFAULT_PORT_RANGE_END: u16 = 9999;

pub struct ContainerManager {
    containers: DashMap<String, Container>,
    /// Port allocation tracker
    used_ports: DashMap<u16, String>, // port -> container_id
    next_port: AtomicU16,
    /// Whether Docker is available on this system
    docker_available: bool,
}

impl ContainerManager {
    pub fn new() -> Self {
        let docker_available = Self::check_docker_available();
        if docker_available {
            info!("Docker runtime detected and available");
        } else {
            warn!("Docker runtime not detected -- container management will be simulated");
        }

        Self {
            containers: DashMap::new(),
            used_ports: DashMap::new(),
            next_port: AtomicU16::new(DEFAULT_PORT_RANGE_START),
            docker_available,
        }
    }

    /// Check if Docker is available.
    fn check_docker_available() -> bool {
        std::process::Command::new("docker")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Allocate an available port.
    fn allocate_port(&self) -> u16 {
        loop {
            let port = self.next_port.fetch_add(1, Ordering::Relaxed);
            let port = if port > DEFAULT_PORT_RANGE_END {
                self.next_port
                    .store(DEFAULT_PORT_RANGE_START, Ordering::Relaxed);
                DEFAULT_PORT_RANGE_START
            } else {
                port
            };
            if !self.used_ports.contains_key(&port) {
                return port;
            }
        }
    }

    /// Create and start a container.
    pub async fn create_container(
        &self,
        req: CreateContainerRequest,
    ) -> Result<Container, String> {
        if req.image.is_empty() {
            return Err("Container image is required".into());
        }
        if req.model.is_empty() {
            return Err("Model name is required".into());
        }

        // Validate port
        let port = if let Some(p) = req.port {
            if self.used_ports.contains_key(&p) {
                return Err(format!("Port {} is already in use", p));
            }
            p
        } else {
            self.allocate_port()
        };

        // Validate memory limit format (e.g., "4g", "512m")
        let env_map = req.env.clone().unwrap_or_default();
        let memory_limit = req.memory_limit.clone().unwrap_or_else(|| "4g".into());
        if !Self::validate_resource_limit(&memory_limit) {
            return Err(format!(
                "Invalid memory limit format: {}. Use format like '4g' or '512m'",
                memory_limit
            ));
        }

        // Validate CPU limit format (e.g., "2.0", "4")
        let cpu_limit = req.cpu_limit.clone().unwrap_or_else(|| "2.0".into());
        if cpu_limit.parse::<f64>().is_err() {
            return Err(format!("Invalid CPU limit: {}. Must be a number", cpu_limit));
        }

        let container_id = uuid::Uuid::new_v4().to_string();

        let container = Container {
            id: container_id.clone(),
            config: ContainerConfig {
                image: req.image.clone(),
                model: req.model.clone(),
                gpu_device: req.gpu_device,
                port,
                env: env_map.clone(),
                memory_limit: memory_limit.clone(),
                cpu_limit: cpu_limit.clone(),
            },
            status: ContainerStatus::Creating,
            created_at: Utc::now(),
            started_at: None,
            stopped_at: None,
            health_status: None,
            error: None,
        };

        self.used_ports.insert(port, container_id.clone());
        self.containers.insert(container_id.clone(), container);

        info!(
            container_id = %container_id,
            image = %req.image,
            model = %req.model,
            port = port,
            "Creating container"
        );

        // Start the container via Docker
        match self
            .start_docker_container(&container_id, port, &req.image, &req.model, req.gpu_device, &env_map, &memory_limit, &cpu_limit)
            .await
        {
            Ok(()) => {
                if let Some(mut c) = self.containers.get_mut(&container_id) {
                    c.status = ContainerStatus::Running;
                    c.started_at = Some(Utc::now());
                    c.health_status = Some("starting".into());
                }
                info!(
                    container_id = %container_id,
                    port = port,
                    "Container started successfully"
                );
            }
            Err(e) => {
                if let Some(mut c) = self.containers.get_mut(&container_id) {
                    c.status = ContainerStatus::Error;
                    c.error = Some(e.clone());
                }
                error!(
                    container_id = %container_id,
                    error = %e,
                    "Failed to start container"
                );
            }
        }

        Ok(self.containers.get(&container_id).unwrap().value().clone())
    }

    /// Actually start a Docker container.
    async fn start_docker_container(
        &self,
        container_id: &str,
        port: u16,
        image: &str,
        model: &str,
        gpu_device: Option<u32>,
        env: &HashMap<String, String>,
        memory_limit: &str,
        cpu_limit: &str,
    ) -> Result<(), String> {
        if !self.docker_available {
            // Simulate container start in non-Docker environments
            debug!(
                container_id = %container_id,
                "Simulating container start (Docker not available)"
            );
            return Ok(());
        }

        let mut cmd = tokio::process::Command::new("docker");
        cmd.arg("run")
            .arg("-d")
            .arg("--name")
            .arg(format!("xergon-{}", container_id))
            .arg("-p")
            .arg(format!("{}:8080", port))
            .arg("-e")
            .arg(format!("XERGON_MODEL={}", model))
            .arg("-e")
            .arg(format!("XERGON_CONTAINER_ID={}", container_id))
            .arg("--memory")
            .arg(memory_limit)
            .arg("--cpus")
            .arg(cpu_limit);

        // GPU passthrough
        if let Some(gpu_id) = gpu_device {
            cmd.arg("--gpus").arg(format!("device={}", gpu_id));
        }

        // Environment variables
        for (key, value) in env {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }

        cmd.arg(image);

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to execute docker command: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Docker run failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        debug!(
            container_id = %container_id,
            docker_id = %stdout.trim(),
            "Docker container started"
        );

        Ok(())
    }

    /// List all containers.
    pub fn list_containers(&self) -> Vec<Container> {
        self.containers.iter().map(|r| r.value().clone()).collect()
    }

    /// Get a specific container.
    pub fn get_container(&self, id: &str) -> Option<Container> {
        self.containers.get(id).map(|r| r.value().clone())
    }

    /// Stop a running container.
    pub async fn stop_container(&self, id: &str) -> Result<Container, String> {
        let mut container = self
            .containers
            .get_mut(id)
            .ok_or_else(|| format!("Container {} not found", id))?;

        match container.status {
            ContainerStatus::Running => {}
            ContainerStatus::Stopped => return Err("Container is already stopped".into()),
            ContainerStatus::Creating => return Err("Container is still being created".into()),
            ContainerStatus::Error => return Err("Cannot stop a container in error state".into()),
        }

        // Stop via Docker
        if self.docker_available {
            let docker_name = format!("xergon-{}", id);
            let output = tokio::process::Command::new("docker")
                .arg("stop")
                .arg(&docker_name)
                .output()
                .await;

            match output {
                Ok(o) if o.status.success() => {
                    debug!(container_id = %id, "Docker container stopped");
                }
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    warn!(
                        container_id = %id,
                        error = %stderr,
                        "Docker stop failed (simulating stop)"
                    );
                }
                Err(e) => {
                    warn!(
                        container_id = %id,
                        error = %e,
                        "Docker stop command failed (simulating stop)"
                    );
                }
            }
        }

        container.status = ContainerStatus::Stopped;
        container.stopped_at = Some(Utc::now());
        container.health_status = None;

        info!(container_id = %id, "Container stopped");
        Ok(container.value().clone())
    }

    /// Start a stopped container.
    pub async fn start_container(&self, id: &str) -> Result<Container, String> {
        let mut container = self
            .containers
            .get_mut(id)
            .ok_or_else(|| format!("Container {} not found", id))?;

        match container.status {
            ContainerStatus::Stopped => {}
            ContainerStatus::Running => return Err("Container is already running".into()),
            ContainerStatus::Creating => return Err("Container is still being created".into()),
            ContainerStatus::Error => return Err("Cannot start a container in error state".into()),
        }

        // Start via Docker
        if self.docker_available {
            let docker_name = format!("xergon-{}", id);
            let output = tokio::process::Command::new("docker")
                .arg("start")
                .arg(&docker_name)
                .output()
                .await;

            match output {
                Ok(o) if o.status.success() => {
                    debug!(container_id = %id, "Docker container started");
                }
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    warn!(
                        container_id = %id,
                        error = %stderr,
                        "Docker start failed (simulating start)"
                    );
                }
                Err(e) => {
                    warn!(
                        container_id = %id,
                        error = %e,
                        "Docker start command failed (simulating start)"
                    );
                }
            }
        }

        container.status = ContainerStatus::Running;
        container.started_at = Some(Utc::now());
        container.stopped_at = None;
        container.error = None;
        container.health_status = Some("starting".into());

        info!(container_id = %id, "Container started");
        Ok(container.value().clone())
    }

    /// Remove a container.
    pub async fn remove_container(&self, id: &str) -> Result<(), String> {
        let container = self
            .containers
            .get(id)
            .ok_or_else(|| format!("Container {} not found", id))?;

        // Stop first if running
        if container.status == ContainerStatus::Running {
            drop(container);
            self.stop_container(id).await?;
        }

        // Remove via Docker
        if self.docker_available {
            let docker_name = format!("xergon-{}", id);
            let _ = tokio::process::Command::new("docker")
                .arg("rm")
                .arg("-f")
                .arg(&docker_name)
                .output()
                .await;
        }

        // Free port
        if let Some((_, c)) = self.containers.remove(id) {
            self.used_ports.remove(&c.config.port);
        }

        info!(container_id = %id, "Container removed");
        Ok(())
    }

    /// Get container logs.
    pub async fn get_container_logs(
        &self,
        id: &str,
        tail: Option<u32>,
    ) -> Result<String, String> {
        let container = self
            .containers
            .get(id)
            .ok_or_else(|| format!("Container {} not found", id))?;

        if container.status == ContainerStatus::Creating {
            return Err("Container is still being created".into());
        }

        if !self.docker_available {
            return Ok(format!(
                "[Simulated logs for container {} (Docker not available)]\n\
                 Model: {}\n\
                 Image: {}\n\
                 Status: {:?}\n\
                 Created: {}\n",
                id,
                container.config.model,
                container.config.image,
                container.status,
                container.created_at
            ));
        }

        let docker_name = format!("xergon-{}", id);
        let mut cmd = tokio::process::Command::new("docker");
        cmd.arg("logs");

        if let Some(tail_lines) = tail {
            cmd.arg("--tail").arg(tail_lines.to_string());
        }

        cmd.arg(&docker_name);

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to execute docker logs: {}", e))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Run a health check on a container.
    pub async fn health_check(&self, id: &str) -> Result<ContainerHealthCheck, String> {
        let container = self
            .containers
            .get(id)
            .ok_or_else(|| format!("Container {} not found", id))?;

        if container.status != ContainerStatus::Running {
            return Err(format!(
                "Cannot health check container in {:?} state",
                container.status
            ));
        }

        let port = container.config.port;
        let start = std::time::Instant::now();

        // Try HTTP health check on the container's port
        let url = format!("http://127.0.0.1:{}/health", port);
        let result = reqwest::get(&url).await;

        let response_time = start.elapsed().as_millis() as f64;

        let (status, health) = match result {
            Ok(resp) if resp.status().is_success() => ("healthy", Some(response_time)),
            Ok(_resp) => ("unhealthy", Some(response_time)),
            Err(_) => ("unreachable", None),
        };

        // Update container health status
        if let Some(mut c) = self.containers.get_mut(id) {
            c.health_status = Some(status.into());
        }

        Ok(ContainerHealthCheck {
            container_id: id.to_string(),
            status: status.into(),
            last_check: Utc::now(),
            response_time_ms: health,
        })
    }

    /// Run health checks on all running containers.
    pub async fn health_check_all(&self) -> Vec<ContainerHealthCheck> {
        let running: Vec<String> = self
            .containers
            .iter()
            .filter(|r| r.value().status == ContainerStatus::Running)
            .map(|r| r.key().clone())
            .collect();

        let mut results = Vec::new();
        for id in running {
            match self.health_check(&id).await {
                Ok(check) => results.push(check),
                Err(e) => {
                    debug!(container_id = %id, error = %e, "Health check failed");
                }
            }
        }
        results
    }

    /// Validate a resource limit string (e.g., "4g", "512m").
    fn validate_resource_limit(limit: &str) -> bool {
        let lower = limit.to_lowercase();
        if lower.ends_with('g') {
            lower.trim_end_matches('g').parse::<u64>().is_ok()
        } else if lower.ends_with('m') {
            lower.trim_end_matches('m').parse::<u64>().is_ok()
        } else {
            limit.parse::<u64>().is_ok()
        }
    }
}
