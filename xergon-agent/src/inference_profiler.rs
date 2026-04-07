use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilerConfig {
    pub enabled: bool,
    pub sample_rate: f64,
    pub max_profiles: usize,
    pub retention: u64, // seconds
    pub include_tensor_stats: bool,
    pub detailed_memory: bool,
}

impl Default for ProfilerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sample_rate: 0.1,
            max_profiles: 10_000,
            retention: 3600,
            include_tensor_stats: false,
            detailed_memory: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateProfilerConfigRequest {
    pub enabled: Option<bool>,
    pub sample_rate: Option<f64>,
    pub max_profiles: Option<usize>,
    pub retention: Option<u64>,
    pub include_tensor_stats: Option<bool>,
    pub detailed_memory: Option<bool>,
}

// ---------------------------------------------------------------------------
// Profile data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceProfile {
    pub id: String,
    pub model: String,
    pub request_id: String,
    pub total_latency_ms: f64,
    pub phases: Vec<ProfilePhase>,
    pub memory_stats: MemoryProfile,
    pub gpu_stats: GpuProfile,
    pub tensor_stats: Option<Vec<TensorStat>>,
    pub tokens_input: u32,
    pub tokens_output: u32,
    pub batch_size: u32,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilePhase {
    pub name: String,
    pub duration_ms: f64,
    pub start_offset_ms: f64,
    pub details: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryProfile {
    pub peak_rss_mb: u64,
    pub peak_gpu_mb: u64,
    pub allocation_count: u32,
    pub deallocation_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuProfile {
    pub utilization_percent: f64,
    pub memory_used_mb: u64,
    pub temperature_c: f64,
    pub power_watts: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorStat {
    pub layer_name: String,
    pub shape: Vec<usize>,
    pub dtype: String,
    pub size_bytes: u64,
    pub compute_time_ms: f64,
}

// ---------------------------------------------------------------------------
// Aggregated stats
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilerAggregatedStats {
    pub profiles_collected: u64,
    pub avg_total_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub avg_tokens_per_sec: f64,
    pub memory_peak_mb: u64,
    pub config: ProfilerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileComparison {
    pub profile_a: InferenceProfile,
    pub profile_b: InferenceProfile,
    pub latency_diff_ms: f64,
    pub latency_diff_pct: f64,
    pub phase_breakdown: Vec<PhaseComparison>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseComparison {
    pub phase: String,
    pub a_ms: f64,
    pub b_ms: f64,
    pub diff_ms: f64,
    pub diff_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSummary {
    pub model: String,
    pub profile_count: u64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub avg_tokens_per_sec: f64,
    pub avg_gpu_utilization: f64,
}

// ---------------------------------------------------------------------------
// InferenceProfiler
// ---------------------------------------------------------------------------

pub struct InferenceProfiler {
    config: tokio::sync::RwLock<ProfilerConfig>,
    profiles: DashMap<String, InferenceProfile>,
    stats: ProfilerStats,
}

struct ProfilerStats {
    profiles_collected: AtomicU64,
    avg_total_latency_us: AtomicU64,
    p50_latency_us: AtomicU64,
    p99_latency_us: AtomicU64,
    avg_tokens_per_sec: AtomicU64,
    memory_peak_mb: AtomicU64,
}

impl InferenceProfiler {
    pub fn new(config: ProfilerConfig) -> Self {
        Self {
            config: tokio::sync::RwLock::new(config),
            profiles: DashMap::new(),
            stats: ProfilerStats {
                profiles_collected: AtomicU64::new(0),
                avg_total_latency_us: AtomicU64::new(0),
                p50_latency_us: AtomicU64::new(0),
                p99_latency_us: AtomicU64::new(0),
                avg_tokens_per_sec: AtomicU64::new(0),
                memory_peak_mb: AtomicU64::new(0),
            },
        }
    }

    pub fn default() -> Self {
        Self::new(ProfilerConfig::default())
    }

    /// Record a profile. Returns whether it was sampled (accepted).
    pub fn record(&self, profile: InferenceProfile) -> bool {
        let cfg = self.config.blocking_read();
        if !cfg.enabled {
            return false;
        }
        use rand::Rng;
        let sampled = rand::thread_rng().gen_bool(cfg.sample_rate);
        if !sampled {
            return false;
        }
        let id = profile.id.clone();
        self.profiles.insert(id, profile);
        self.stats.profiles_collected.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Create a new profile builder.
    pub fn start_profile(&self, model: &str, request_id: &str) -> ProfileBuilder {
        ProfileBuilder {
            id: uuid::Uuid::new_v4().to_string(),
            model: model.to_string(),
            request_id: request_id.to_string(),
            start: Instant::now(),
            phases: Vec::new(),
            phase_start: None,
            tokens_input: 0,
            tokens_output: 0,
            batch_size: 1,
            memory_stats: MemoryProfile {
                peak_rss_mb: 0,
                peak_gpu_mb: 0,
                allocation_count: 0,
                deallocation_count: 0,
            },
            gpu_stats: GpuProfile {
                utilization_percent: 0.0,
                memory_used_mb: 0,
                temperature_c: 0.0,
                power_watts: 0.0,
            },
            tensor_stats: None,
        }
    }

    pub async fn get_stats(&self) -> ProfilerAggregatedStats {
        let cfg = self.config.read().await;
        let count = self.stats.profiles_collected.load(Ordering::Relaxed);
        ProfilerAggregatedStats {
            profiles_collected: count,
            avg_total_latency_ms: self.stats.avg_total_latency_us.load(Ordering::Relaxed) as f64 / 1000.0,
            p50_latency_ms: self.stats.p50_latency_us.load(Ordering::Relaxed) as f64 / 1000.0,
            p99_latency_ms: self.stats.p99_latency_us.load(Ordering::Relaxed) as f64 / 1000.0,
            avg_tokens_per_sec: self.stats.avg_tokens_per_sec.load(Ordering::Relaxed) as f64,
            memory_peak_mb: self.stats.memory_peak_mb.load(Ordering::Relaxed),
            config: cfg.clone(),
        }
    }

    pub fn get_profile(&self, id: &str) -> Option<InferenceProfile> {
        self.profiles.get(id).map(|r| r.value().clone())
    }

    pub fn list_profiles(&self, offset: usize, limit: usize) -> Vec<InferenceProfile> {
        self.profiles
            .iter()
            .skip(offset)
            .take(limit)
            .map(|r| r.value().clone())
            .collect()
    }

    pub async fn get_model_summary(&self, model: &str) -> ModelSummary {
        let model_profiles: Vec<InferenceProfile> = self
            .profiles
            .iter()
            .filter(|r| r.model == model)
            .map(|r| r.value().clone())
            .collect();

        let count = model_profiles.len() as u64;
        if count == 0 {
            return ModelSummary {
                model: model.to_string(),
                profile_count: 0,
                avg_latency_ms: 0.0,
                p50_latency_ms: 0.0,
                p99_latency_ms: 0.0,
                avg_tokens_per_sec: 0.0,
                avg_gpu_utilization: 0.0,
            };
        }

        let mut latencies: Vec<f64> = model_profiles.iter().map(|p| p.total_latency_ms).collect();
        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let avg_latency = latencies.iter().sum::<f64>() / count as f64;
        let p50 = latencies[(latencies.len() / 2).min(latencies.len() - 1)];
        let p99 = latencies[(latencies.len() * 99 / 100).min(latencies.len() - 1)];

        let total_tokens: u32 = model_profiles.iter().map(|p| p.tokens_output).sum();
        let total_time_s: f64 = model_profiles.iter().map(|p| p.total_latency_ms / 1000.0).sum();
        let tps = if total_time_s > 0.0 { total_tokens as f64 / total_time_s } else { 0.0 };

        let avg_gpu: f64 = if count > 0 {
            model_profiles.iter().map(|p| p.gpu_stats.utilization_percent).sum::<f64>() / count as f64
        } else {
            0.0
        };

        ModelSummary {
            model: model.to_string(),
            profile_count: count,
            avg_latency_ms: avg_latency,
            p50_latency_ms: p50,
            p99_latency_ms: p99,
            avg_tokens_per_sec: tps,
            avg_gpu_utilization: avg_gpu,
        }
    }

    pub fn compare_profiles(&self, id_a: &str, id_b: &str) -> Result<ProfileComparison, String> {
        let a = self.profiles.get(id_a).map(|r| r.value().clone())
            .ok_or_else(|| format!("Profile {} not found", id_a))?;
        let b = self.profiles.get(id_b).map(|r| r.value().clone())
            .ok_or_else(|| format!("Profile {} not found", id_b))?;

        let latency_diff = a.total_latency_ms - b.total_latency_ms;
        let latency_diff_pct = if b.total_latency_ms > 0.0 {
            (latency_diff / b.total_latency_ms) * 100.0
        } else {
            0.0
        };

        let all_phases: std::collections::HashSet<&str> =
            a.phases.iter().map(|p| p.name.as_str())
                .chain(b.phases.iter().map(|p| p.name.as_str()))
                .collect();

        let phase_breakdown: Vec<PhaseComparison> = all_phases.into_iter().map(|name| {
            let a_phase = a.phases.iter().find(|p| p.name == name);
            let b_phase = b.phases.iter().find(|p| p.name == name);
            let a_ms = a_phase.map(|p| p.duration_ms).unwrap_or(0.0);
            let b_ms = b_phase.map(|p| p.duration_ms).unwrap_or(0.0);
            let diff = a_ms - b_ms;
            let pct = if b_ms > 0.0 { (diff / b_ms) * 100.0 } else { 0.0 };
            PhaseComparison {
                phase: name.to_string(),
                a_ms,
                b_ms,
                diff_ms: diff,
                diff_pct: pct,
            }
        }).collect();

        Ok(ProfileComparison {
            profile_a: a,
            profile_b: b,
            latency_diff_ms: latency_diff,
            latency_diff_pct,
            phase_breakdown,
        })
    }

    pub async fn manual_collect(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let profile = InferenceProfile {
            id: id.clone(),
            model: "manual".to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            total_latency_ms: 0.0,
            phases: Vec::new(),
            memory_stats: MemoryProfile {
                peak_rss_mb: 0,
                peak_gpu_mb: 0,
                allocation_count: 0,
                deallocation_count: 0,
            },
            gpu_stats: GpuProfile {
                utilization_percent: 0.0,
                memory_used_mb: 0,
                temperature_c: 0.0,
                power_watts: 0.0,
            },
            tensor_stats: None,
            tokens_input: 0,
            tokens_output: 0,
            batch_size: 1,
            timestamp: Utc::now(),
        };
        self.profiles.insert(id.clone(), profile);
        self.stats.profiles_collected.fetch_add(1, Ordering::Relaxed);
        id
    }

    pub fn clear_profiles(&self) -> usize {
        let count = self.profiles.len();
        self.profiles.clear();
        count
    }

    pub async fn update_config(&self, update: UpdateProfilerConfigRequest) -> ProfilerConfig {
        let mut cfg = self.config.write().await;
        if let Some(v) = update.enabled { cfg.enabled = v; }
        if let Some(v) = update.sample_rate { cfg.sample_rate = v.clamp(0.0, 1.0); }
        if let Some(v) = update.max_profiles { cfg.max_profiles = v; }
        if let Some(v) = update.retention { cfg.retention = v; }
        if let Some(v) = update.include_tensor_stats { cfg.include_tensor_stats = v; }
        if let Some(v) = update.detailed_memory { cfg.detailed_memory = v; }
        cfg.clone()
    }
}

// ---------------------------------------------------------------------------
// Profile builder (helper for constructing profiles)
// ---------------------------------------------------------------------------

pub struct ProfileBuilder {
    id: String,
    model: String,
    request_id: String,
    start: Instant,
    phases: Vec<ProfilePhase>,
    phase_start: Option<(String, Instant)>,
    tokens_input: u32,
    tokens_output: u32,
    batch_size: u32,
    memory_stats: MemoryProfile,
    gpu_stats: GpuProfile,
    tensor_stats: Option<Vec<TensorStat>>,
}

impl ProfileBuilder {
    pub fn begin_phase(&mut self, name: &str) {
        let offset = self.start.elapsed();
        self.phase_start = Some((name.to_string(), Instant::now()));
        self.phases.push(ProfilePhase {
            name: name.to_string(),
            duration_ms: 0.0,
            start_offset_ms: offset.as_secs_f64() * 1000.0,
            details: HashMap::new(),
        });
    }

    pub fn end_phase(&mut self, details: Option<HashMap<String, serde_json::Value>>) {
        if let Some((_, phase_start)) = self.phase_start.take() {
            if let Some(last) = self.phases.last_mut() {
                last.duration_ms = phase_start.elapsed().as_secs_f64() * 1000.0;
                if let Some(d) = details {
                    last.details = d;
                }
            }
        }
    }

    pub fn set_tokens(&mut self, input: u32, output: u32) {
        self.tokens_input = input;
        self.tokens_output = output;
    }

    pub fn set_batch_size(&mut self, size: u32) {
        self.batch_size = size;
    }

    pub fn set_memory(&mut self, peak_rss_mb: u64, peak_gpu_mb: u64, allocs: u32, deallocs: u32) {
        self.memory_stats = MemoryProfile {
            peak_rss_mb,
            peak_gpu_mb,
            allocation_count: allocs,
            deallocation_count: deallocs,
        };
    }

    pub fn set_gpu(&mut self, util: f64, mem_mb: u64, temp: f64, power: f64) {
        self.gpu_stats = GpuProfile {
            utilization_percent: util,
            memory_used_mb: mem_mb,
            temperature_c: temp,
            power_watts: power,
        };
    }

    pub fn finish(self) -> InferenceProfile {
        InferenceProfile {
            id: self.id,
            model: self.model,
            request_id: self.request_id,
            total_latency_ms: self.start.elapsed().as_secs_f64() * 1000.0,
            phases: self.phases,
            memory_stats: self.memory_stats,
            gpu_stats: self.gpu_stats,
            tensor_stats: self.tensor_stats,
            tokens_input: self.tokens_input,
            tokens_output: self.tokens_output,
            batch_size: self.batch_size,
            timestamp: Utc::now(),
        }
    }
}
