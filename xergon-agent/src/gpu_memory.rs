//! GPU memory management for tracking VRAM allocation across devices.
//!
//! Provides fine-grained tracking of GPU memory regions with:
//! - Allocation/deallocation of memory regions per device
//! - Fragmentation analysis and defragmentation suggestions
//! - Memory pressure detection (warns at >90% utilization)
//! - Pre-warm reservation for expected models at startup
//! - Integration points for multi_gpu.rs and model_sharding.rs
//!
//! API endpoints:
//! - GET    /api/gpu-memory/devices           -- device info with memory stats
//! - GET    /api/gpu-memory/allocations        -- current allocations
//! - GET    /api/gpu-memory/available          -- available memory per device
//! - POST   /api/gpu-memory/allocate           -- manual allocation
//! - DELETE /api/gpu-memory/allocate/{region_id} -- deallocate
//! - GET    /api/gpu-memory/fragmentation      -- fragmentation stats
//! - POST   /api/gpu-memory/defrag             -- suggest defragmentation plan

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Information about a single GPU device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDeviceInfo {
    /// Device identifier (e.g., 0, 1, 2).
    pub id: u32,
    /// Device name (e.g., "NVIDIA RTX 4090").
    pub name: String,
    /// Total VRAM in MB.
    pub total_memory_mb: u64,
    /// Driver version string.
    pub driver: String,
    /// Current temperature in Celsius.
    pub temperature: f64,
    /// Current GPU utilization (0.0 - 1.0).
    pub utilization: f64,
}

/// A tracked GPU memory region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMemoryRegion {
    /// Unique region identifier.
    pub id: String,
    /// Device this region is allocated on.
    pub device_id: u32,
    /// Offset in bytes from the start of VRAM.
    pub offset: u64,
    /// Size of this region in MB.
    pub size_mb: u64,
    /// Owner identifier (model_id or "system").
    pub owner: String,
    /// When this allocation was made.
    pub allocated_at: DateTime<Utc>,
}

/// Memory availability info for a single device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMemoryInfo {
    pub device_id: u32,
    pub device_name: String,
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub available_memory_mb: u64,
    pub utilization_ratio: f64,
    pub allocation_count: usize,
}

/// Fragmentation statistics for a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentationStats {
    pub device_id: u32,
    pub total_regions: usize,
    pub free_regions: usize,
    pub largest_free_region_mb: u64,
    pub fragmentation_ratio: f64,
    pub needs_defragmentation: bool,
}

/// A suggested defragmentation action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefragAction {
    pub region_id: String,
    pub device_id: u32,
    pub current_offset: u64,
    pub suggested_offset: u64,
    pub size_mb: u64,
    pub owner: String,
}

/// A complete defragmentation plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefragPlan {
    pub device_id: u32,
    pub total_regions_to_move: usize,
    pub estimated_savings_mb: u64,
    pub actions: Vec<DefragAction>,
}

/// Request body for manual allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocateRequest {
    pub device_id: u32,
    pub size_mb: u64,
    pub owner: String,
}

/// Response for a successful allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocateResponse {
    pub region_id: String,
    pub device_id: u32,
    pub offset: u64,
    pub size_mb: u64,
    pub owner: String,
}

// ---------------------------------------------------------------------------
// GPU Memory Manager
// ---------------------------------------------------------------------------

/// Thread-safe GPU memory manager that tracks allocations across devices.
pub struct GpuMemoryManager {
    /// Known GPU devices.
    devices: DashMap<u32, GpuDeviceInfo>,
    /// Memory regions keyed by region_id.
    regions: DashMap<String, GpuMemoryRegion>,
    /// Used memory per device (device_id -> used_mb).
    used_memory: DashMap<u32, u64>,
    /// Memory pressure threshold (warn when utilization exceeds this ratio).
    pressure_threshold: f64,
    /// Fragmentation threshold (suggest defrag when exceeding this ratio).
    defrag_threshold: f64,
}

impl GpuMemoryManager {
    /// Create a new GPU memory manager with default thresholds.
    pub fn new() -> Self {
        Self {
            devices: DashMap::new(),
            regions: DashMap::new(),
            used_memory: DashMap::new(),
            pressure_threshold: 0.90, // warn at 90% utilization
            defrag_threshold: 0.50,  // suggest defrag at 50% fragmentation
        }
    }

    /// Create a new GPU memory manager with custom thresholds.
    pub fn with_thresholds(pressure_threshold: f64, defrag_threshold: f64) -> Self {
        Self {
            devices: DashMap::new(),
            regions: DashMap::new(),
            used_memory: DashMap::new(),
            pressure_threshold: pressure_threshold.clamp(0.0, 1.0),
            defrag_threshold: defrag_threshold.clamp(0.0, 1.0),
        }
    }

    /// Register a GPU device.
    pub fn register_device(&self, device: GpuDeviceInfo) {
        let id = device.id;
        self.devices.insert(id, device);
        self.used_memory.entry(id).or_insert(0);
        info!(device_id = id, "GPU device registered");
    }

    /// Unregister a GPU device and free all its allocations.
    pub fn unregister_device(&self, device_id: u32) {
        let freed_regions: Vec<String> = self
            .regions
            .iter()
            .filter(|r| r.value().device_id == device_id)
            .map(|r| r.key().clone())
            .collect();

        let mut freed_mb = 0u64;
        for region_id in &freed_regions {
            if let Some((_, region)) = self.regions.remove(region_id) {
                freed_mb += region.size_mb;
            }
        }

        if let Some(mut used) = self.used_memory.get_mut(&device_id) {
            *used = used.saturating_sub(freed_mb);
        }

        self.devices.remove(&device_id);
        info!(
            device_id,
            freed_regions = freed_regions.len(),
            freed_mb,
            "GPU device unregistered"
        );
    }

    /// Allocate a memory region on a specific device.
    /// Returns the region ID on success, or an error string.
    pub fn allocate(&self, device_id: u32, size_mb: u64, owner: &str) -> Result<String, String> {
        // Check device exists
        let device = self
            .devices
            .get(&device_id)
            .ok_or_else(|| format!("Device {} not found", device_id))?;

        let total = device.total_memory_mb;
        let used = self.used_memory.get(&device_id).map(|v| *v).unwrap_or(0);

        if used + size_mb > total {
            let available = total.saturating_sub(used);
            return Err(format!(
                "Insufficient memory: requested {} MB, available {} MB",
                size_mb, available
            ));
        }

        // Compute offset (place at end of current used region)
        let offset = (used * 1024 * 1024) as u64; // Convert MB to bytes for offset

        // Update used memory
        if let Some(mut used_entry) = self.used_memory.get_mut(&device_id) {
            *used_entry += size_mb;
        }

        let region_id = uuid::Uuid::new_v4().to_string();
        let region = GpuMemoryRegion {
            id: region_id.clone(),
            device_id,
            offset,
            size_mb,
            owner: owner.to_string(),
            allocated_at: Utc::now(),
        };

        self.regions.insert(region_id.clone(), region);

        // Check memory pressure
        let new_used = used + size_mb;
        let utilization = new_used as f64 / total as f64;
        if utilization > self.pressure_threshold {
            warn!(
                device_id,
                utilization = (utilization * 100.0) as u32,
                "GPU memory pressure warning"
            );
        }

        debug!(
            device_id,
            region_id = %region_id,
            size_mb,
            owner,
            "GPU memory allocated"
        );

        Ok(region_id)
    }

    /// Deallocate a memory region by ID.
    /// Returns the freed size in MB.
    pub fn deallocate(&self, region_id: &str) -> Result<u64, String> {
        let (region_id_owned, region) = self
            .regions
            .remove(region_id)
            .ok_or_else(|| format!("Region {} not found", region_id))?;

        let device_id = region.device_id;
        let size_mb = region.size_mb;

        if let Some(mut used) = self.used_memory.get_mut(&device_id) {
            *used = used.saturating_sub(size_mb);
        }

        debug!(
            device_id,
            region_id = %region_id_owned,
            size_mb,
            owner = %region.owner,
            "GPU memory deallocated"
        );

        Ok(size_mb)
    }

    /// Get available memory in MB for a specific device.
    pub fn get_available(&self, device_id: u32) -> Option<u64> {
        let device = self.devices.get(&device_id)?;
        let used = self.used_memory.get(&device_id).map(|v| *v).unwrap_or(0);
        Some(device.total_memory_mb.saturating_sub(used))
    }

    /// Get used memory in MB for a specific device.
    pub fn get_used(&self, device_id: u32) -> Option<u64> {
        self.used_memory.get(&device_id).map(|v| *v)
    }

    /// Get total memory in MB for a specific device.
    pub fn get_total(&self, device_id: u32) -> Option<u64> {
        self.devices.get(&device_id).map(|d| d.total_memory_mb)
    }

    /// Get memory info for all devices.
    pub fn get_device_memory_info(&self) -> Vec<DeviceMemoryInfo> {
        self.devices
            .iter()
            .map(|device| {
                let d = device.value();
                let used = self.used_memory.get(&d.id).map(|v| *v).unwrap_or(0);
                let allocation_count = self
                    .regions
                    .iter()
                    .filter(|r| r.value().device_id == d.id)
                    .count();
                DeviceMemoryInfo {
                    device_id: d.id,
                    device_name: d.name.clone(),
                    total_memory_mb: d.total_memory_mb,
                    used_memory_mb: used,
                    available_memory_mb: d.total_memory_mb.saturating_sub(used),
                    utilization_ratio: if d.total_memory_mb > 0 {
                        used as f64 / d.total_memory_mb as f64
                    } else {
                        0.0
                    },
                    allocation_count,
                }
            })
            .collect()
    }

    /// Get all current allocations.
    pub fn get_allocations(&self) -> Vec<GpuMemoryRegion> {
        self.regions.iter().map(|r| r.value().clone()).collect()
    }

    /// Get allocations for a specific device.
    pub fn get_device_allocations(&self, device_id: u32) -> Vec<GpuMemoryRegion> {
        self.regions
            .iter()
            .filter(|r| r.value().device_id == device_id)
            .map(|r| r.value().clone())
            .collect()
    }

    /// Get available memory for all devices.
    pub fn get_all_available(&self) -> HashMap<u32, u64> {
        self.devices
            .iter()
            .filter_map(|device| {
                let d = device.value();
                let used = self.used_memory.get(&d.id).map(|v| *v).unwrap_or(0);
                Some((d.id, d.total_memory_mb.saturating_sub(used)))
            })
            .collect()
    }

    /// Calculate fragmentation ratio for a device.
    /// Fragmentation is estimated as 1 - (largest_free_region / total_free).
    /// A value > 0.5 indicates high fragmentation.
    pub fn get_fragmentation(&self, device_id: u32) -> Option<FragmentationStats> {
        let device = self.devices.get(&device_id)?;
        let total = device.total_memory_mb;
        let used = self.used_memory.get(&device_id).map(|v| *v).unwrap_or(0);
        let free_mb = total.saturating_sub(used);

        if free_mb == 0 {
            return Some(FragmentationStats {
                device_id,
                total_regions: self
                    .regions
                    .iter()
                    .filter(|r| r.value().device_id == device_id)
                    .count(),
                free_regions: 0,
                largest_free_region_mb: 0,
                fragmentation_ratio: 0.0,
                needs_defragmentation: false,
            });
        }

        let mut device_regions: Vec<GpuMemoryRegion> = self
            .regions
            .iter()
            .filter(|r| r.value().device_id == device_id)
            .map(|r| r.value().clone())
            .collect();

        // Sort by offset to compute gaps
        device_regions.sort_by_key(|r| r.offset);

        // Compute gaps between regions
        let mut gaps: Vec<u64> = Vec::new();
        let mut current_end = 0u64;

        for region in &device_regions {
            let region_end = region.offset + (region.size_mb * 1024 * 1024);
            if region.offset > current_end {
                gaps.push(region.offset - current_end);
            }
            current_end = current_end.max(region_end);
        }

        // Check gap at the end
        let total_bytes = total * 1024 * 1024;
        if current_end < total_bytes {
            gaps.push(total_bytes - current_end);
        }

        let largest_free_region_mb = gaps
            .iter()
            .map(|&g| g / (1024 * 1024))
            .max()
            .unwrap_or(0);

        let fragmentation_ratio = if free_mb > 0 {
            1.0 - (largest_free_region_mb as f64 / free_mb as f64)
        } else {
            0.0
        };

        Some(FragmentationStats {
            device_id,
            total_regions: device_regions.len(),
            free_regions: gaps.len(),
            largest_free_region_mb,
            fragmentation_ratio,
            needs_defragmentation: fragmentation_ratio > self.defrag_threshold,
        })
    }

    /// Generate a defragmentation plan for a device.
    /// The plan suggests compacting all regions to remove gaps.
    pub fn suggest_defrag(&self, device_id: u32) -> Result<DefragPlan, String> {
        let device = self
            .devices
            .get(&device_id)
            .ok_or_else(|| format!("Device {} not found", device_id))?;

        let mut device_regions: Vec<GpuMemoryRegion> = self
            .regions
            .iter()
            .filter(|r| r.value().device_id == device_id)
            .map(|r| r.value().clone())
            .collect();

        // Sort by current offset
        device_regions.sort_by_key(|r| r.offset);

        let mut actions = Vec::new();
        let mut suggested_offset = 0u64;

        for region in &device_regions {
            let current_offset = region.offset;
            if current_offset != suggested_offset {
                actions.push(DefragAction {
                    region_id: region.id.clone(),
                    device_id,
                    current_offset,
                    suggested_offset,
                    size_mb: region.size_mb,
                    owner: region.owner.clone(),
                });
            }
            suggested_offset += region.size_mb * 1024 * 1024;
        }

        // Estimate savings: the gap at the end after compaction
        let total_bytes = device.total_memory_mb * 1024 * 1024;
        let used_bytes = suggested_offset;
        let estimated_savings_mb = if total_bytes > used_bytes {
            (total_bytes - used_bytes) / (1024 * 1024)
        } else {
            0
        };

        Ok(DefragPlan {
            device_id,
            total_regions_to_move: actions.len(),
            estimated_savings_mb,
            actions,
        })
    }

    /// Pre-warm: reserve memory for expected models at startup.
    /// Returns a map of model_id -> region_id for each successful reservation.
    pub fn pre_warm(&self, reservations: Vec<(u32, u64, &str)>) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for (device_id, size_mb, owner) in reservations {
            match self.allocate(device_id, size_mb, owner) {
                Ok(region_id) => {
                    info!(device_id, size_mb, owner, region_id = %region_id, "Pre-warm allocation");
                    result.insert(owner.to_string(), region_id);
                }
                Err(e) => {
                    warn!(device_id, size_mb, owner, error = %e, "Pre-warm allocation failed");
                }
            }
        }
        result
    }

    /// Check if a device is under memory pressure.
    pub fn is_under_pressure(&self, device_id: u32) -> bool {
        if let Some(device) = self.devices.get(&device_id) {
            let used = self.used_memory.get(&device_id).map(|v| *v).unwrap_or(0);
            let utilization = used as f64 / device.total_memory_mb as f64;
            utilization > self.pressure_threshold
        } else {
            false
        }
    }

    /// Get all registered devices.
    pub fn get_devices(&self) -> Vec<GpuDeviceInfo> {
        self.devices.iter().map(|d| d.value().clone()).collect()
    }

    /// Get the number of registered devices.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Get the number of active allocations.
    pub fn allocation_count(&self) -> usize {
        self.regions.len()
    }
}
