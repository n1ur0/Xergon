//! Multi-GPU inference management for the Xergon agent.
//!
//! Detects available GPUs, manages tensor/pipeline parallel configurations,
//! load-balances inference requests, and coordinates VRAM across devices.
//!
//! API:
//! - GET   /api/gpu/devices -- list GPU devices with status
//! - GET   /api/gpu/config  -- current multi-GPU config
//! - PATCH /api/gpu/config  -- update config
//! - GET   /api/gpu/usage   -- real-time GPU usage across all devices

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDevice {
    pub id: u32,
    pub name: String,
    pub vram_mb: u64,
    pub vram_used_mb: u64,
    pub driver: String,
    pub active_inferences: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiGpuConfig {
    pub enabled: bool,
    pub tensor_parallel: bool,
    pub pipeline_parallel: bool,
    pub devices: Vec<u32>,
}

impl Default for MultiGpuConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            tensor_parallel: false,
            pipeline_parallel: false,
            devices: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateGpuConfigRequest {
    pub enabled: Option<bool>,
    pub tensor_parallel: Option<bool>,
    pub pipeline_parallel: Option<bool>,
    pub devices: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuUsageInfo {
    pub device_id: u32,
    pub device_name: String,
    pub vram_total_mb: u64,
    pub vram_used_mb: u64,
    pub vram_free_mb: u64,
    pub utilization_percent: f64,
    pub active_inferences: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalanceResult {
    pub device_id: u32,
    pub device_name: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Multi-GPU manager
// ---------------------------------------------------------------------------

pub struct MultiGpuManager {
    config: RwLock<MultiGpuConfig>,
    devices: DashMap<u32, GpuDevice>,
    /// Per-device model assignments for pipeline parallel
    pipeline_assignments: DashMap<String, u32>, // model_name -> device_id
    fallback_enabled: AtomicBool,
}

impl MultiGpuManager {
    pub fn new() -> Self {
        let manager = Self {
            config: RwLock::new(MultiGpuConfig::default()),
            devices: DashMap::new(),
            pipeline_assignments: DashMap::new(),
            fallback_enabled: AtomicBool::new(true),
        };

        manager.detect_devices();
        manager
    }

    /// Detect available GPUs from hardware.rs and populate device list.
    fn detect_devices(&self) {
        let hw = crate::hardware::detect_hardware();

        for (idx, gpu) in hw.gpus.iter().enumerate() {
            let device = GpuDevice {
                id: idx as u32,
                name: gpu.name.clone(),
                vram_mb: gpu.vram_mb,
                vram_used_mb: gpu.vram_used_mb.unwrap_or(0),
                driver: gpu.driver.clone(),
                active_inferences: 0,
            };
            info!(
                device_id = device.id,
                name = %device.name,
                vram_mb = device.vram_mb,
                driver = %device.driver,
                "Detected GPU device"
            );
            self.devices.insert(device.id, device);
        }
    }

    /// Refresh GPU VRAM usage by re-querying hardware.
    pub fn refresh_usage(&self) {
        let hw = crate::hardware::detect_hardware();

        for (idx, gpu) in hw.gpus.iter().enumerate() {
            if let Some(mut device) = self.devices.get_mut(&(idx as u32)) {
                device.vram_used_mb = gpu.vram_used_mb.unwrap_or(0);
            }
        }
    }

    /// List all detected GPU devices.
    pub fn list_devices(&self) -> Vec<GpuDevice> {
        self.devices.iter().map(|r| r.value().clone()).collect()
    }

    /// Add a test device (only available in test builds).
    #[cfg(test)]
    pub fn add_test_device(&self, device: GpuDevice) {
        self.devices.insert(device.id, device);
    }

    /// Get current multi-GPU configuration.
    pub async fn get_config(&self) -> MultiGpuConfig {
        self.config.read().await.clone()
    }

    /// Update multi-GPU configuration.
    pub async fn update_config(&self, req: UpdateGpuConfigRequest) -> Result<MultiGpuConfig, String> {
        let mut config = self.config.write().await;

        if let Some(enabled) = req.enabled {
            config.enabled = enabled;
        }
        if let Some(tensor_parallel) = req.tensor_parallel {
            if tensor_parallel && config.pipeline_parallel {
                return Err("Cannot enable both tensor and pipeline parallel simultaneously".into());
            }
            config.tensor_parallel = tensor_parallel;
        }
        if let Some(pipeline_parallel) = req.pipeline_parallel {
            if pipeline_parallel && config.tensor_parallel {
                return Err("Cannot enable both tensor and pipeline parallel simultaneously".into());
            }
            config.pipeline_parallel = pipeline_parallel;
        }
        if let Some(devices) = req.devices {
            // Validate device IDs
            for &dev_id in &devices {
                if !self.devices.contains_key(&dev_id) {
                    return Err(format!("GPU device {} not found", dev_id));
                }
            }
            if devices.is_empty() && config.enabled {
                return Err("At least one device must be specified when multi-GPU is enabled".into());
            }
            config.devices = devices;
        }

        // If enabling multi-GPU without specified devices, use all available
        if config.enabled && config.devices.is_empty() {
            config.devices = self.devices.iter().map(|r| *r.key()).collect();
        }

        info!(
            enabled = config.enabled,
            tensor_parallel = config.tensor_parallel,
            pipeline_parallel = config.pipeline_parallel,
            devices = ?config.devices,
            "Multi-GPU configuration updated"
        );

        Ok(config.clone())
    }

    /// Get real-time GPU usage across all devices.
    pub fn get_usage(&self) -> Vec<GpuUsageInfo> {
        self.refresh_usage();
        self.devices
            .iter()
            .map(|r| {
                let d = r.value();
                GpuUsageInfo {
                    device_id: d.id,
                    device_name: d.name.clone(),
                    vram_total_mb: d.vram_mb,
                    vram_used_mb: d.vram_used_mb,
                    vram_free_mb: d.vram_mb.saturating_sub(d.vram_used_mb),
                    utilization_percent: if d.vram_mb > 0 {
                        (d.vram_used_mb as f64 / d.vram_mb as f64) * 100.0
                    } else {
                        0.0
                    },
                    active_inferences: d.active_inferences,
                }
            })
            .collect()
    }

    /// Assign a model to a specific GPU device (pipeline parallel).
    pub fn assign_model_to_device(&self, model_name: &str, device_id: u32) -> Result<(), String> {
        if !self.devices.contains_key(&device_id) {
            return Err(format!("GPU device {} not found", device_id));
        }
        self.pipeline_assignments
            .insert(model_name.to_string(), device_id);
        info!(
            model = %model_name,
            device_id = device_id,
            "Assigned model to GPU device (pipeline parallel)"
        );
        Ok(())
    }

    /// Remove a model assignment.
    pub fn remove_model_assignment(&self, model_name: &str) {
        self.pipeline_assignments.remove(model_name);
    }

    /// Load-balance an inference request to the least-loaded GPU.
    pub fn select_device(&self, model_name: &str) -> LoadBalanceResult {
        let config = self.config_blocking();

        // If multi-GPU is not enabled, use device 0 (or pipeline assignment)
        if !config.enabled {
            if let Some(dev_id) = self.pipeline_assignments.get(model_name) {
                let dev_id = *dev_id;
                if let Some(d) = self.devices.get(&dev_id) {
                    return LoadBalanceResult {
                        device_id: d.id,
                        device_name: d.name.clone(),
                        reason: "pipeline assignment".into(),
                    };
                }
            }
            // Fallback to first available device
            if let Some(d) = self.devices.iter().next() {
                return LoadBalanceResult {
                    device_id: d.id,
                    device_name: d.name.clone(),
                    reason: "single-GPU fallback".into(),
                };
            }
            return LoadBalanceResult {
                device_id: 0,
                device_name: "cpu".into(),
                reason: "no GPUs available".into(),
            };
        }

        // Pipeline parallel: check if model has a fixed assignment
        if config.pipeline_parallel {
            if let Some(dev_id) = self.pipeline_assignments.get(model_name) {
                let dev_id = *dev_id;
                if let Some(d) = self.devices.get(&dev_id) {
                    return LoadBalanceResult {
                        device_id: d.id,
                        device_name: d.name.clone(),
                        reason: "pipeline parallel assignment".into(),
                    };
                }
            }
        }

        // Load balance: pick device with lowest active inferences
        let mut best_device: Option<(u32, String, u32, u64)> = None; // (id, name, active, vram_free)

        for entry in self.devices.iter() {
            let d = entry.value();

            // Filter to configured devices only
            if !config.devices.is_empty() && !config.devices.contains(&d.id) {
                continue;
            }

            let vram_free = d.vram_mb.saturating_sub(d.vram_used_mb);

            match &best_device {
                None => {
                    best_device = Some((d.id, d.name.clone(), d.active_inferences, vram_free));
                }
                Some((_, _, best_active, best_vram)) => {
                    // Prefer device with fewer active inferences
                    // Break ties by more free VRAM
                    if d.active_inferences < *best_active
                        || (d.active_inferences == *best_active && vram_free > *best_vram)
                    {
                        best_device = Some((d.id, d.name.clone(), d.active_inferences, vram_free));
                    }
                }
            }
        }

        if let Some((id, name, _, _)) = best_device {
            let reason = if config.tensor_parallel {
                "tensor parallel load balance".into()
            } else {
                "least-loaded device".into()
            };
            LoadBalanceResult {
                device_id: id,
                device_name: name,
                reason,
            }
        } else {
            // No configured devices found, fallback
            if self.fallback_enabled.load(Ordering::Relaxed) {
                if let Some(d) = self.devices.iter().next() {
                    return LoadBalanceResult {
                        device_id: d.id,
                        device_name: d.name.clone(),
                        reason: "multi-GPU fallback".into(),
                    };
                }
            }
            LoadBalanceResult {
                device_id: 0,
                device_name: "cpu".into(),
                reason: "no GPUs available".into(),
            }
        }
    }

    /// Track that an inference started on a device.
    pub fn inference_started(&self, device_id: u32) {
        if let Some(mut d) = self.devices.get_mut(&device_id) {
            d.active_inferences += 1;
        }
    }

    /// Track that an inference completed on a device.
    pub fn inference_completed(&self, device_id: u32) {
        if let Some(mut d) = self.devices.get_mut(&device_id) {
            d.active_inferences = d.active_inferences.saturating_sub(1);
        }
    }

    /// Check if a model can fit on available GPUs with current config.
    pub fn can_load_model(&self, model_vram_mb: u64) -> Result<bool, String> {
        let config = self.config_blocking();

        if self.devices.is_empty() {
            return Err("No GPUs available".into());
        }

        if config.tensor_parallel {
            // Sum free VRAM across all configured devices
            let total_free: u64 = config
                .devices
                .iter()
                .filter_map(|id| self.devices.get(id))
                .map(|d| d.vram_mb.saturating_sub(d.vram_used_mb))
                .sum();
            Ok(total_free >= model_vram_mb)
        } else {
            // Find single device with enough free VRAM
            for entry in self.devices.iter() {
                let d = entry.value();
                if d.vram_mb.saturating_sub(d.vram_used_mb) >= model_vram_mb {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }

    /// Get the list of devices to use for tensor parallel inference.
    pub fn get_tensor_parallel_devices(&self) -> Vec<u32> {
        let config = self.config_blocking();
        if config.enabled && config.tensor_parallel {
            config.devices.clone()
        } else {
            vec![]
        }
    }

    /// Non-blocking config read for internal use.
    fn config_blocking(&self) -> MultiGpuConfig {
        self.config.try_read().map(|g| g.clone()).unwrap_or_default()
    }

    /// Check if fallback is enabled.
    pub fn is_fallback_enabled(&self) -> bool {
        self.fallback_enabled.load(Ordering::Relaxed)
    }

    /// Set fallback mode.
    pub fn set_fallback(&self, enabled: bool) {
        self.fallback_enabled.store(enabled, Ordering::Relaxed);
    }
}
