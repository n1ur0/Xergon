//! Real GPU hardware detection for the Xergon agent.
//!
//! Detects NVIDIA GPUs (nvidia-smi / sysfs), AMD GPUs (rocminfo / sysfs),
//! and Apple Silicon (sysctl). Results are cached in a `OnceLock` since
//! hardware doesn't change at runtime.

use serde::Serialize;
use std::sync::OnceLock;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub vram_mb: u64,
    pub vram_used_mb: Option<u64>,
    pub driver: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HardwareInfo {
    pub gpus: Vec<GpuInfo>,
    pub total_vram_mb: u64,
    pub cpu_cores: usize,
    pub ram_gb: f64,
    pub os: String,
}

// ---------------------------------------------------------------------------
// Cache — detection runs at most once
// ---------------------------------------------------------------------------

static HARDWARE_CACHE: OnceLock<HardwareInfo> = OnceLock::new();

/// Returns a reference to the cached `HardwareInfo`.
/// The first call runs the detection; subsequent calls return the cached result.
pub fn detect_hardware() -> &'static HardwareInfo {
    HARDWARE_CACHE.get_or_init(do_detect)
}

// ---------------------------------------------------------------------------
// Detection entry point
// ---------------------------------------------------------------------------

fn do_detect() -> HardwareInfo {
    let os = std::env::consts::OS.to_string();
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let ram_gb = detect_ram();

    let mut gpus: Vec<GpuInfo> = Vec::new();

    // --- NVIDIA ----------------------------------------------------------
    match detect_nvidia_gpus() {
        Ok(list) => {
            if !list.is_empty() {
                info!(count = list.len(), "Detected NVIDIA GPU(s)");
                gpus.extend(list);
            }
        }
        Err(e) => {
            tracing::debug!(error = %e, "NVIDIA GPU detection failed");
        }
    }

    // --- AMD -------------------------------------------------------------
    match detect_amd_gpus() {
        Ok(list) => {
            if !list.is_empty() {
                info!(count = list.len(), "Detected AMD GPU(s)");
                gpus.extend(list);
            }
        }
        Err(e) => {
            tracing::debug!(error = %e, "AMD GPU detection failed");
        }
    }

    // --- Apple Silicon ---------------------------------------------------
    if os == "macos" {
        match detect_apple_silicon(ram_gb) {
            Ok(list) => {
                if !list.is_empty() {
                    info!(count = list.len(), "Detected Apple Silicon GPU");
                    gpus.extend(list);
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "Apple Silicon GPU detection failed");
            }
        }
    }

    let total_vram_mb: u64 = gpus.iter().map(|g| g.vram_mb).sum();

    if gpus.is_empty() {
        info!("No GPUs detected — system has CPU only");
    }

    HardwareInfo {
        gpus,
        total_vram_mb,
        cpu_cores,
        ram_gb,
        os,
    }
}

// ---------------------------------------------------------------------------
// RAM detection
// ---------------------------------------------------------------------------

fn detect_ram() -> f64 {
    if cfg!(target_os = "linux") {
        detect_ram_linux()
    } else if cfg!(target_os = "macos") {
        detect_ram_macos()
    } else {
        0.0
    }
}

fn detect_ram_linux() -> f64 {
    let output = match std::fs::read_to_string("/proc/meminfo") {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "Failed to read /proc/meminfo");
            return 0.0;
        }
    };

    for line in output.lines() {
        if line.starts_with("MemTotal:") {
            // Format: "MemTotal:       16384000 kB"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(kb) = parts[1].parse::<u64>() {
                    return (kb as f64) / (1024.0 * 1024.0); // KB -> GB
                }
            }
        }
    }
    0.0
}

fn detect_ram_macos() -> f64 {
    let output = match std::process::Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            warn!(error = %e, "Failed to run sysctl hw.memsize");
            return 0.0;
        }
    };

    if !output.status.success() {
        return 0.0;
    }

    let bytes_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match bytes_str.parse::<u64>() {
        Ok(b) => (b as f64) / (1024.0 * 1024.0 * 1024.0), // bytes -> GB
        Err(e) => {
            warn!(error = %e, "Failed to parse sysctl hw.memsize output");
            0.0
        }
    }
}

// ---------------------------------------------------------------------------
// NVIDIA GPU detection
// ---------------------------------------------------------------------------

fn detect_nvidia_gpus() -> Result<Vec<GpuInfo>, String> {
    // Primary: nvidia-smi
    match detect_nvidia_smi() {
        Ok(gpus) if !gpus.is_empty() => return Ok(gpus),
        _ => {}
    }

    // Fallback: /sys/class/drm/modalias
    detect_nvidia_sysfs()
}

fn detect_nvidia_smi() -> Result<Vec<GpuInfo>, String> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,memory.used",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .map_err(|e| format!("nvidia-smi exec failed: {e}"))?;

    if !output.status.success() {
        return Err("nvidia-smi exited with non-zero".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // CSV fields: name, memory.total (MiB), memory.used (MiB)
        let fields: Vec<&str> = line.splitn(3, ',').collect();
        if fields.len() < 2 {
            continue;
        }

        let name = fields[0].trim().to_string();
        let vram_mb = fields[1].trim().parse::<u64>().unwrap_or(0);
        let vram_used_mb = if fields.len() >= 3 {
            fields[2].trim().parse::<u64>().ok()
        } else {
            None
        };

        gpus.push(GpuInfo {
            name,
            vram_mb,
            vram_used_mb,
            driver: "nvidia".to_string(),
        });
    }

    Ok(gpus)
}

fn detect_nvidia_sysfs() -> Result<Vec<GpuInfo>, String> {
    let mut gpus = Vec::new();
    let cards_dir = match std::fs::read_dir("/sys/class/drm") {
        Ok(d) => d,
        Err(_) => return Ok(gpus),
    };

    let mut seen_devices: std::collections::HashSet<String> = std::collections::HashSet::new();

    for entry in cards_dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Only look at cardN (not cardN-device or renderD*)
        if !name_str.starts_with("card") || name_str.contains('-') {
            continue;
        }

        let uevent_path = entry.path().join("device/uevent");
        let uevent = match std::fs::read_to_string(&uevent_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let mut modalias = String::new();
        let mut device_name = String::new();

        for line in uevent.lines() {
            if line.starts_with("MODALIAS=") {
                modalias = line.trim_start_matches("MODALIAS=").to_string();
            }
            if line.starts_with("PCI_ID=") {
                device_name = line.trim_start_matches("PCI_ID=").to_string();
            }
        }

        // Check if this is an NVIDIA device (modalias contains "nv" or "nvidia")
        let modalias_lower = modalias.to_lowercase();
        if !modalias_lower.contains("nvidia") && !modalias_lower.contains("pci:v000010de") {
            continue;
        }

        // Deduplicate — multiple DRM nodes can point to the same GPU
        if seen_devices.contains(&device_name) {
            continue;
        }
        seen_devices.insert(device_name.clone());

        // Try to get VRAM from /sys/class/drm/cardN/device/mem_info_vram_total
        let vram_mb = read_nvidia_vram_sysfs(&entry.path());

        gpus.push(GpuInfo {
            name: if device_name.is_empty() {
                format!("NVIDIA GPU (card {})", &name_str[4..])
            } else {
                format!("NVIDIA GPU ({})", device_name)
            },
            vram_mb,
            vram_used_mb: None,
            driver: "nvidia".to_string(),
        });
    }

    Ok(gpus)
}

fn read_nvidia_vram_sysfs(card_path: &std::path::Path) -> u64 {
    // Try the newer sysfs path for VRAM total
    let vram_paths = [
        card_path.join("device/mem_info_vram_total"),
        card_path.join("device/mem_info_vram_used"),
    ];

    // Read VRAM total (first path)
    if let Ok(content) = std::fs::read_to_string(&vram_paths[0]) {
        if let Ok(bytes) = content.trim().parse::<u64>() {
            return bytes / (1024 * 1024); // bytes -> MiB
        }
    }

    // Try reading from /proc/driver/nvidia/gpus/*/information
    if let Ok(entries) = std::fs::read_dir("/proc/driver/nvidia/gpus") {
        for entry in entries.flatten() {
            let info_path = entry.path().join("information");
            if let Ok(content) = std::fs::read_to_string(&info_path) {
                for line in content.lines() {
                    if line.contains("Video Memory") {
                        // Format: "Video Memory:    24564 MB"
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 3 {
                            if let Ok(mb) = parts[2].parse::<u64>() {
                                return mb;
                            }
                        }
                    }
                }
            }
        }
    }

    0
}

// ---------------------------------------------------------------------------
// AMD GPU detection
// ---------------------------------------------------------------------------

fn detect_amd_gpus() -> Result<Vec<GpuInfo>, String> {
    // Primary: rocminfo
    match detect_amd_rocminfo() {
        Ok(gpus) if !gpus.is_empty() => return Ok(gpus),
        _ => {}
    }

    // Fallback: /sys/class/kfd
    detect_amd_sysfs()
}

fn detect_amd_rocminfo() -> Result<Vec<GpuInfo>, String> {
    let output = std::process::Command::new("rocminfo")
        .output()
        .map_err(|e| format!("rocminfo exec failed: {e}"))?;

    if !output.status.success() {
        return Err("rocminfo exited with non-zero".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();

    // rocminfo groups output into "Agent N" blocks.
    // GPU agents have a "Marketing Name" (or fall back to "Name" with gfx*) and
    // a "Global Memory Size" under a "Memory:" subsection.

    let mut current_name: Option<String> = None;
    let mut current_marketing_name: Option<String> = None;
    let mut current_vram_bytes: Option<u64> = None;
    let mut in_memory_section = false;
    let mut is_gpu_agent = false;

    for line in stdout.lines() {
        let trimmed = line.trim();

        // New agent block
        if trimmed.starts_with("Agent ") {
            // Flush previous agent if it was a GPU
            if is_gpu_agent && (current_vram_bytes.is_some() || current_marketing_name.is_some()) {
                let display_name = current_marketing_name.clone().unwrap_or_else(|| {
                    current_name
                        .clone()
                        .unwrap_or_else(|| "AMD GPU".to_string())
                });
                let vram_mb = current_vram_bytes.map(|b| b / (1024 * 1024)).unwrap_or(0);

                gpus.push(GpuInfo {
                    name: display_name,
                    vram_mb,
                    vram_used_mb: None,
                    driver: "amd".to_string(),
                });
            }

            // Reset state for new agent
            current_name = None;
            current_marketing_name = None;
            current_vram_bytes = None;
            in_memory_section = false;
            is_gpu_agent = false;
            continue;
        }

        // Track sections
        if trimmed == "CPU:" {
            in_memory_section = false;
            continue;
        }
        if trimmed == "Memory:" {
            in_memory_section = true;
            continue;
        }

        // Parse fields
        if let Some(value) = trimmed.strip_prefix("Name:") {
            let val = value.trim().to_string();
            if val.starts_with("gfx") {
                is_gpu_agent = true;
            }
            current_name = Some(val);
        }

        if let Some(value) = trimmed.strip_prefix("Marketing Name:") {
            let val = value.trim().to_string();
            if !val.is_empty() {
                is_gpu_agent = true;
                current_marketing_name = Some(val);
            }
        }

        if in_memory_section {
            if let Some(value) = trimmed.strip_prefix("Global Memory Size:") {
                let val_str = value.trim();
                // Value may have trailing units or just be a number
                let num_str = val_str.split_whitespace().next().unwrap_or(val_str);
                if let Ok(bytes) = num_str.parse::<u64>() {
                    current_vram_bytes = Some(bytes);
                }
            }
        }
    }

    // Flush last agent
    if is_gpu_agent && (current_vram_bytes.is_some() || current_marketing_name.is_some()) {
        let display_name = current_marketing_name
            .unwrap_or_else(|| current_name.unwrap_or_else(|| "AMD GPU".to_string()));
        let vram_mb = current_vram_bytes.map(|b| b / (1024 * 1024)).unwrap_or(0);

        gpus.push(GpuInfo {
            name: display_name,
            vram_mb,
            vram_used_mb: None,
            driver: "amd".to_string(),
        });
    }

    Ok(gpus)
}

fn detect_amd_sysfs() -> Result<Vec<GpuInfo>, String> {
    let mut gpus = Vec::new();
    let topology_path = std::path::Path::new("/sys/class/kfd/kfd/topology/nodes");

    if !topology_path.exists() {
        return Ok(gpus);
    }

    let entries = match std::fs::read_dir(topology_path) {
        Ok(d) => d,
        Err(_) => return Ok(gpus),
    };

    for node_entry in entries.flatten() {
        let node_path = node_entry.path();

        // Check for GPU properties (CPU nodes don't have mem_banks with GPU sizes)
        // Try to read the node name first
        let name = read_amd_node_name(&node_path);

        let mem_banks_path = node_path.join("mem_banks");
        if !mem_banks_path.exists() {
            continue;
        }

        let mut total_vram_bytes: u64 = 0;

        if let Ok(bank_entries) = std::fs::read_dir(&mem_banks_path) {
            for bank_entry in bank_entries.flatten() {
                let props_path = bank_entry.path().join("properties");
                if let Ok(content) = std::fs::read_to_string(&props_path) {
                    for line in content.lines() {
                        if line.starts_with("size") {
                            // Format: "size = 0x0000000580000000"
                            if let Some(hex_str) = line.split('=').nth(1) {
                                let hex_str = hex_str.trim().trim_start_matches("0x");
                                if let Ok(bytes) = u64::from_str_radix(hex_str, 16) {
                                    total_vram_bytes += bytes;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Only add if we found VRAM (GPU nodes have significant memory)
        if total_vram_bytes > 0 {
            let vram_mb = total_vram_bytes / (1024 * 1024);
            gpus.push(GpuInfo {
                name: name.unwrap_or_else(|| "AMD GPU".to_string()),
                vram_mb,
                vram_used_mb: None,
                driver: "amd".to_string(),
            });
        }
    }

    Ok(gpus)
}

fn read_amd_node_name(node_path: &std::path::Path) -> Option<String> {
    // Try to read the GPU name from sysfs
    // Path: /sys/class/kfd/kfd/topology/nodes/N/name
    let name_path = node_path.join("name");
    if let Ok(name) = std::fs::read_to_string(&name_path) {
        let name = name.trim().to_string();
        if !name.is_empty() {
            return Some(format!("AMD GPU ({name})"));
        }
    }

    // Try to get the system-board name or just return a generic name
    // Check if this node has a "properties" file with GPU info
    let props_path = node_path.join("properties");
    if let Ok(content) = std::fs::read_to_string(&props_path) {
        for line in content.lines() {
            if line.starts_with("name") {
                if let Some(val) = line.split('=').nth(1) {
                    let val = val.trim().trim_matches('"').to_string();
                    if !val.is_empty() {
                        return Some(format!("AMD GPU ({val})"));
                    }
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Apple Silicon detection
// ---------------------------------------------------------------------------

fn detect_apple_silicon(ram_gb: f64) -> Result<Vec<GpuInfo>, String> {
    // Check if running on Apple Silicon (macOS + ARM64)
    let is_arm64 = match std::process::Command::new("sysctl")
        .args(["-n", "hw.optional.arm64"])
        .output()
    {
        Ok(output) => {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "1"
        }
        Err(_) => false,
    };

    if !is_arm64 {
        return Ok(vec![]);
    }

    // Get the specific chip name
    let chip_name = match std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "Apple Silicon".to_string(),
    };

    // Apple Silicon uses unified memory — total RAM is the VRAM pool
    let vram_mb = (ram_gb * 1024.0) as u64;

    Ok(vec![GpuInfo {
        name: chip_name,
        vram_mb,
        vram_used_mb: None,
        driver: "apple".to_string(),
    }])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_hardware_returns_cached_ref() {
        let a = detect_hardware() as *const HardwareInfo;
        let b = detect_hardware() as *const HardwareInfo;
        assert_eq!(
            a, b,
            "detect_hardware should return the same pointer (cached)"
        );
    }

    #[test]
    fn hardware_info_has_os_and_cores() {
        let hw = detect_hardware();
        assert!(!hw.os.is_empty());
        assert!(hw.cpu_cores >= 1);
        assert!(hw.ram_gb >= 0.0);
    }
}
