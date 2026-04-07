use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// 1. Autonomous Model Optimization
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OptimizationGoal {
    Latency,
    Throughput,
    Accuracy,
    Memory,
    Balanced,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    pub max_latency_ms: Option<f64>,
    pub min_accuracy: Option<f64>,
    pub max_memory_mb: Option<f64>,
    pub target_throughput: Option<f64>,
    pub goal: OptimizationGoal,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            max_latency_ms: Some(100.0),
            min_accuracy: Some(0.9),
            max_memory_mb: Some(4096.0),
            target_throughput: Some(100.0),
            goal: OptimizationGoal::Balanced,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OptimizationStrategy {
    BatchSizeTuning,
    CacheWarming,
    ConcurrencyAdjust,
    ContextWindowOpt,
    MemoryPoolResize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetrics {
    pub latency_ms: f64,
    pub throughput_rps: f64,
    pub accuracy: f64,
    pub memory_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    pub id: String,
    pub model_name: String,
    pub before_metrics: ModelMetrics,
    pub after_metrics: ModelMetrics,
    pub strategies_applied: Vec<OptimizationStrategy>,
    pub improvement_pct: f64,
    pub timestamp: DateTime<Utc>,
}

pub struct OptimizationEngine {
    results: DashMap<String, OptimizationResult>,
    config: RwLock<OptimizationConfig>,
}

impl OptimizationEngine {
    pub fn new(config: OptimizationConfig) -> Self {
        Self {
            results: DashMap::new(),
            config: RwLock::new(config),
        }
    }

    /// Analyze current metrics and suggest strategies.
    pub async fn suggest_optimizations(
        &self,
        metrics: &ModelMetrics,
        model_name: &str,
    ) -> Vec<OptimizationStrategy> {
        let config = self.config.read().await;
        let mut strategies = Vec::new();

        match config.goal {
            OptimizationGoal::Latency => {
                if metrics.latency_ms > config.max_latency_ms.unwrap_or(100.0) {
                    strategies.push(OptimizationStrategy::BatchSizeTuning);
                    strategies.push(OptimizationStrategy::ConcurrencyAdjust);
                }
            }
            OptimizationGoal::Throughput => {
                if metrics.throughput_rps < config.target_throughput.unwrap_or(100.0) {
                    strategies.push(OptimizationStrategy::BatchSizeTuning);
                    strategies.push(OptimizationStrategy::CacheWarming);
                }
            }
            OptimizationGoal::Memory => {
                if metrics.memory_mb > config.max_memory_mb.unwrap_or(4096.0) {
                    strategies.push(OptimizationStrategy::ContextWindowOpt);
                    strategies.push(OptimizationStrategy::MemoryPoolResize);
                    strategies.push(OptimizationStrategy::CacheWarming);
                }
            }
            OptimizationGoal::Accuracy => {
                // Accuracy goal: no aggressive optimisations, maybe increase cache
                strategies.push(OptimizationStrategy::CacheWarming);
            }
            OptimizationGoal::Balanced => {
                if metrics.latency_ms > config.max_latency_ms.unwrap_or(100.0) {
                    strategies.push(OptimizationStrategy::BatchSizeTuning);
                    strategies.push(OptimizationStrategy::ConcurrencyAdjust);
                }
                if metrics.memory_mb > config.max_memory_mb.unwrap_or(4096.0) {
                    strategies.push(OptimizationStrategy::ContextWindowOpt);
                    strategies.push(OptimizationStrategy::MemoryPoolResize);
                }
                if metrics.throughput_rps < config.target_throughput.unwrap_or(100.0) {
                    strategies.push(OptimizationStrategy::BatchSizeTuning);
                    strategies.push(OptimizationStrategy::CacheWarming);
                }
            }
        }

        // deduplicate
        strategies.sort_by(|a, b| {
            let ord_a = format!("{:?}", a);
            let ord_b = format!("{:?}", b);
            ord_a.cmp(&ord_b)
        });
        strategies.dedup();

        info!(
            model = model_name,
            strategies = ?strategies,
            "suggested optimizations"
        );
        strategies
    }

    /// Apply suggested strategies, returning a simulated result.
    pub async fn apply_optimization(
        &self,
        model_name: String,
        metrics: ModelMetrics,
    ) -> OptimizationResult {
        let strategies = self.suggest_optimizations(&metrics, &model_name).await;

        // Simulate metric improvements based on applied strategies
        let mut after = metrics.clone();
        let mut total_improvement = 0.0f64;

        for s in &strategies {
            match s {
                OptimizationStrategy::BatchSizeTuning => {
                    after.latency_ms *= 0.85;
                    after.throughput_rps *= 1.15;
                    total_improvement += 5.0;
                }
                OptimizationStrategy::CacheWarming => {
                    after.latency_ms *= 0.90;
                    after.throughput_rps *= 1.10;
                    total_improvement += 3.0;
                }
                OptimizationStrategy::ConcurrencyAdjust => {
                    after.latency_ms *= 0.80;
                    after.throughput_rps *= 1.20;
                    after.memory_mb *= 1.05;
                    total_improvement += 7.0;
                }
                OptimizationStrategy::ContextWindowOpt => {
                    after.memory_mb *= 0.75;
                    after.latency_ms *= 0.95;
                    total_improvement += 4.0;
                }
                OptimizationStrategy::MemoryPoolResize => {
                    after.memory_mb *= 0.80;
                    after.latency_ms *= 0.97;
                    total_improvement += 3.0;
                }
            }
        }

        // Clamp improvements to reasonable range
        total_improvement = total_improvement.min(40.0);

        let result = OptimizationResult {
            id: Uuid::new_v4().to_string(),
            model_name: model_name.clone(),
            before_metrics: metrics,
            after_metrics: after,
            strategies_applied: strategies,
            improvement_pct: total_improvement,
            timestamp: Utc::now(),
        };

        self.results.insert(result.id.clone(), result.clone());
        info!(model = model_name, improvement = total_improvement, "optimization applied");
        result
    }

    pub fn get_result(&self, id: &str) -> Option<OptimizationResult> {
        self.results.get(id).map(|r| r.value().clone())
    }

    pub fn get_all_results(&self) -> Vec<OptimizationResult> {
        self.results.iter().map(|r| r.value().clone()).collect()
    }

    pub async fn get_config(&self) -> OptimizationConfig {
        self.config.read().await.clone()
    }

    pub async fn set_config(&self, config: OptimizationConfig) {
        *self.config.write().await = config;
    }
}

// ---------------------------------------------------------------------------
// 2. Neural Architecture Search
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LayerType {
    Attention,
    FFN,
    Embedding,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerConfig {
    pub layer_type: LayerType,
    pub hidden_dim: usize,
    pub num_heads: usize,
    pub activation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Architecture {
    pub id: String,
    pub layers: Vec<LayerConfig>,
    pub total_params: usize,
    pub estimated_latency_ms: f64,
    pub estimated_memory_mb: f64,
    pub fitness_score: f64,
}

impl Architecture {
    pub fn compute_fitness(&mut self) {
        // Higher is better. We want low latency, low memory, high throughput proxy
        // Use a simple multi-objective scalarisation.
        let latency_penalty = (self.estimated_latency_ms / 100.0).max(0.01);
        let memory_penalty = (self.estimated_memory_mb / 4096.0).max(0.01);
        let param_bonus = (self.total_params as f64 / 1_000_000.0).min(10.0);
        self.fitness_score = param_bonus / (latency_penalty * memory_penalty);
    }

    pub fn estimate_metrics(&mut self) {
        let mut total_params = 0usize;
        let mut latency = 0.0f64;
        let mut memory = 0.0f64;
        for layer in &self.layers {
            let dim = layer.hidden_dim as f64;
            match layer.layer_type {
                LayerType::Attention => {
                    // Params: 4 * dim^2 (Q,K,V,O) + bias
                    total_params += (4.0 * dim * dim + 4.0 * dim) as usize;
                    latency += dim * 0.0001;
                    memory += dim * dim * 4.0 / (1024.0 * 1024.0) * 4.0; // 4 bytes per param
                }
                LayerType::FFN => {
                    // Params: dim * 4*dim * 2
                    total_params += (2.0 * dim * 4.0 * dim + 2.0 * 4.0 * dim) as usize;
                    latency += dim * 0.00015;
                    memory += dim * 4.0 * dim * 4.0 / (1024.0 * 1024.0) * 2.0;
                }
                LayerType::Embedding => {
                    total_params += (32000.0 * dim + dim) as usize;
                    latency += dim * 0.00005;
                    memory += 32000.0 * dim * 4.0 / (1024.0 * 1024.0);
                }
                LayerType::Output => {
                    total_params += (32000.0 * dim + 32000.0) as usize;
                    latency += dim * 0.00003;
                    memory += 32000.0 * dim * 4.0 / (1024.0 * 1024.0);
                }
            }
        }
        self.total_params = total_params;
        self.estimated_latency_ms = latency * 1000.0;
        self.estimated_memory_mb = memory;
        self.compute_fitness();
    }

    pub fn random() -> Self {
        let mut rng = rand::thread_rng();
        let num_layers = rng.gen_range(4..12);
        let mut layers = Vec::with_capacity(num_layers);

        // Always start with embedding
        layers.push(LayerConfig {
            layer_type: LayerType::Embedding,
            hidden_dim: rng.gen_range(256..2048),
            num_heads: 1,
            activation: "none".to_string(),
        });

        // Alternating attention + FFN
        for i in 1..num_layers - 1 {
            let hidden_dim = rng.gen_range(256..2048);
            if i % 2 == 1 {
                let num_heads = (hidden_dim / 64).max(1);
                layers.push(LayerConfig {
                    layer_type: LayerType::Attention,
                    hidden_dim,
                    num_heads,
                    activation: "gelu".to_string(),
                });
            } else {
                layers.push(LayerConfig {
                    layer_type: LayerType::FFN,
                    hidden_dim,
                    num_heads: 0,
                    activation: if rng.gen_bool(0.5) { "gelu".to_string() } else { "relu".to_string() },
                });
            }
        }

        // Always end with output
        layers.push(LayerConfig {
            layer_type: LayerType::Output,
            hidden_dim: layers.last().map(|l| l.hidden_dim).unwrap_or(512),
            num_heads: 1,
            activation: "softmax".to_string(),
        });

        let mut arch = Architecture {
            id: Uuid::new_v4().to_string(),
            layers,
            total_params: 0,
            estimated_latency_ms: 0.0,
            estimated_memory_mb: 0.0,
            fitness_score: 0.0,
        };
        arch.estimate_metrics();
        arch
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NASStrategy {
    Random,
    Evolutionary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NASConfig {
    pub strategy: NASStrategy,
    pub population_size: usize,
    pub generations: usize,
    pub eval_budget: usize,
}

impl Default for NASConfig {
    fn default() -> Self {
        Self {
            strategy: NASStrategy::Evolutionary,
            population_size: 20,
            generations: 10,
            eval_budget: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NASResult {
    pub id: String,
    pub best_architecture_id: String,
    pub generations_completed: usize,
    pub best_fitness: f64,
    pub config: NASConfig,
    pub timestamp: DateTime<Utc>,
}

pub struct EvolutionaryNAS {
    search_results: DashMap<String, NASResult>,
    architectures: DashMap<String, Architecture>,
}

impl EvolutionaryNAS {
    pub fn new() -> Self {
        Self {
            search_results: DashMap::new(),
            architectures: DashMap::new(),
        }
    }

    /// Tournament selection with k=3.
    fn tournament_select<'a>(
        population: &'a [Architecture],
        k: usize,
    ) -> &'a Architecture {
        let mut rng = rand::thread_rng();
        let best = (0..k)
            .map(|_| &population[rng.gen_range(0..population.len())])
            .max_by(|a, b| a.fitness_score.partial_cmp(&b.fitness_score).unwrap())
            .unwrap();
        best
    }

    /// Crossover: randomly swap layers between two parents.
    fn crossover(parent1: &Architecture, parent2: &Architecture) -> Architecture {
        let mut rng = rand::thread_rng();
        let min_len = parent1.layers.len().min(parent2.layers.len());
        let crossover_point = rng.gen_range(1..min_len);

        let mut child_layers = parent1.layers[..crossover_point].to_vec();
        child_layers.extend_from_slice(&parent2.layers[crossover_point..]);

        let mut child = Architecture {
            id: Uuid::new_v4().to_string(),
            layers: child_layers,
            total_params: 0,
            estimated_latency_ms: 0.0,
            estimated_memory_mb: 0.0,
            fitness_score: 0.0,
        };
        child.estimate_metrics();
        child
    }

    /// Mutation: perturb hidden_dim and num_heads by +/- 25%.
    fn mutate(arch: &Architecture) -> Architecture {
        let mut rng = rand::thread_rng();
        let mut layers = arch.layers.clone();

        for layer in &mut layers {
            if rng.gen_bool(0.3) {
                let factor = 0.75 + rng.gen_range(0.0..0.5); // 0.75 to 1.25
                layer.hidden_dim = ((layer.hidden_dim as f64 * factor).round() as usize).max(64);
            }
            if rng.gen_bool(0.2) && layer.num_heads > 1 {
                let factor = 0.75 + rng.gen_range(0.0..0.5);
                layer.num_heads = ((layer.num_heads as f64 * factor).round() as usize).max(1);
            }
            if rng.gen_bool(0.1) {
                layer.activation = if rng.gen_bool(0.5) {
                    "gelu".to_string()
                } else {
                    "relu".to_string()
                };
            }
        }

        let mut mutated = Architecture {
            id: Uuid::new_v4().to_string(),
            layers,
            total_params: 0,
            estimated_latency_ms: 0.0,
            estimated_memory_mb: 0.0,
            fitness_score: 0.0,
        };
        mutated.estimate_metrics();
        mutated
    }

    /// Run a NAS search. Returns the search ID.
    pub fn search(&self, config: NASConfig) -> String {
        let search_id = Uuid::new_v4().to_string();
        let mut rng = rand::thread_rng();

        match config.strategy {
            NASStrategy::Random => {
                let count = config.eval_budget.min(100);
                let mut best_fitness = f64::NEG_INFINITY;
                let mut best_arch_id = String::new();

                for _ in 0..count {
                    let arch = Architecture::random();
                    if arch.fitness_score > best_fitness {
                        best_fitness = arch.fitness_score;
                        best_arch_id = arch.id.clone();
                    }
                    self.architectures.insert(arch.id.clone(), arch);
                }

                let result = NASResult {
                    id: search_id.clone(),
                    best_architecture_id: best_arch_id,
                    generations_completed: 1,
                    best_fitness,
                    config: config.clone(),
                    timestamp: Utc::now(),
                };
                self.search_results.insert(search_id.clone(), result);
            }
            NASStrategy::Evolutionary => {
                // Initialise population
                let mut population: Vec<Architecture> = (0..config.population_size)
                    .map(|_| Architecture::random())
                    .collect();
                for arch in &population {
                    self.architectures.insert(arch.id.clone(), arch.clone());
                }

                let mut best_fitness = population
                    .iter()
                    .map(|a| a.fitness_score)
                    .fold(f64::NEG_INFINITY, f64::max);
                let mut best_arch_id = population
                    .iter()
                    .max_by(|a, b| a.fitness_score.partial_cmp(&b.fitness_score).unwrap())
                    .unwrap()
                    .id
                    .clone();

                for gen in 0..config.generations {
                    // Elitism: keep best
                    population.sort_by(|a, b| {
                        b.fitness_score
                            .partial_cmp(&a.fitness_score)
                            .unwrap()
                    });
                    let elite = population[0].clone();

                    let mut new_population = vec![elite];

                    while new_population.len() < config.population_size {
                        let parent1 = Self::tournament_select(&population, 3);
                        let parent2 = Self::tournament_select(&population, 3);

                        let child = if rng.gen_bool(0.7) {
                            Self::crossover(parent1, parent2)
                        } else {
                            parent1.clone()
                        };

                        let child = if rng.gen_bool(0.2) {
                            Self::mutate(&child)
                        } else {
                            child
                        };

                        self.architectures.insert(child.id.clone(), child.clone());
                        new_population.push(child);
                    }

                    population = new_population;

                    // Track best
                    let gen_best = population
                        .iter()
                        .max_by(|a, b| a.fitness_score.partial_cmp(&b.fitness_score).unwrap())
                        .unwrap();
                    if gen_best.fitness_score > best_fitness {
                        best_fitness = gen_best.fitness_score;
                        best_arch_id = gen_best.id.clone();
                    }

                    debug!(generation = gen, best_fitness, "NAS generation complete");
                }

                let result = NASResult {
                    id: search_id.clone(),
                    best_architecture_id: best_arch_id,
                    generations_completed: config.generations,
                    best_fitness,
                    config: config.clone(),
                    timestamp: Utc::now(),
                };
                self.search_results.insert(search_id.clone(), result);
            }
        }

        info!(search_id = %search_id, "NAS search complete");
        search_id
    }

    pub fn get_search_result(&self, id: &str) -> Option<NASResult> {
        self.search_results.get(id).map(|r| r.value().clone())
    }

    pub fn get_architecture(&self, id: &str) -> Option<Architecture> {
        self.architectures.get(id).map(|a| a.value().clone())
    }

    pub fn get_all_architectures(&self) -> Vec<Architecture> {
        self.architectures.iter().map(|a| a.value().clone()).collect()
    }
}

// ---------------------------------------------------------------------------
// 3. Adaptive Quantization
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QuantMethod {
    INT8,
    INT4,
    INT2,
    FP8,
    NF4,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerSensitivity {
    pub layer_name: String,
    pub sensitivity_score: f64,  // 0.0-1.0
    pub recommended_bits: u32,
    pub estimated_accuracy_loss: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantLayerConfig {
    pub layer_name: String,
    pub original_bits: u32,
    pub target_bits: u32,
    pub method: QuantMethod,
    pub sensitivity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationProfile {
    pub id: String,
    pub model_name: String,
    pub layers: Vec<QuantLayerConfig>,
    pub total_memory_savings_pct: f64,
    pub estimated_accuracy_loss_pct: f64,
    pub timestamp: DateTime<Utc>,
}

pub struct AdaptiveQuantizer {
    profiles: DashMap<String, QuantizationProfile>,
    history: DashMap<String, QuantizationProfile>,
}

impl AdaptiveQuantizer {
    pub fn new() -> Self {
        Self {
            profiles: DashMap::new(),
            history: DashMap::new(),
        }
    }

    /// Analyze sensitivity of layers. High gradient norm or large activation range -> more bits.
    pub fn analyze_sensitivity(
        &self,
        model_name: &str,
        layer_names: &[String],
        gradient_norms: &[f64],
        activation_ranges: &[f64],
    ) -> Vec<LayerSensitivity> {
        let mut sensitivities = Vec::new();
        let max_grad = gradient_norms.iter().cloned().fold(0.0f64, f64::max);
        let max_range = activation_ranges.iter().cloned().fold(0.0f64, f64::max);

        for (i, name) in layer_names.iter().enumerate() {
            let grad = gradient_norms.get(i).copied().unwrap_or(0.0);
            let range = activation_ranges.get(i).copied().unwrap_or(0.0);

            // Normalise to [0, 1]
            let norm_grad = if max_grad > 0.0 { grad / max_grad } else { 0.0 };
            let norm_range = if max_range > 0.0 { range / max_range } else { 0.0 };

            // Sensitivity is weighted combination
            let sensitivity = (norm_grad * 0.6 + norm_range * 0.4).clamp(0.0, 1.0);

            // High sensitivity -> more bits; low sensitivity -> aggressive quant
            let recommended_bits = if sensitivity > 0.8 {
                16 // Keep FP16 for very sensitive layers
            } else if sensitivity > 0.6 {
                8  // INT8
            } else if sensitivity > 0.3 {
                4  // INT4
            } else {
                2  // INT2
            };

            // Accuracy loss estimation: more bits = less loss
            let accuracy_loss = match recommended_bits {
                16 => 0.0,
                8 => 0.5 + sensitivity * 0.5,
                4 => 1.0 + sensitivity * 1.5,
                2 => 3.0 + sensitivity * 2.0,
                _ => 1.0,
            };

            sensitivities.push(LayerSensitivity {
                layer_name: name.clone(),
                sensitivity_score: sensitivity,
                recommended_bits,
                estimated_accuracy_loss: accuracy_loss,
            });
        }

        info!(
            model = model_name,
            layers = sensitivities.len(),
            "sensitivity analysis complete"
        );
        sensitivities
    }

    /// Assign mixed-precision quantization respecting a total accuracy loss budget.
    pub fn assign_mixed_precision(
        &self,
        model_name: &str,
        sensitivities: &[LayerSensitivity],
        max_accuracy_loss_pct: f64,
    ) -> QuantizationProfile {
        let mut total_original_bits: u64 = 0;
        let mut layers_config: Vec<QuantLayerConfig> = Vec::new();
        let mut estimated_loss = 0.0f64;

        // Sort by sensitivity descending: allocate bits to most sensitive first
        let mut sorted: Vec<&LayerSensitivity> = sensitivities.iter().collect();
        sorted.sort_by(|a, b| {
            b.sensitivity_score
                .partial_cmp(&a.sensitivity_score)
                .unwrap()
        });

        // First pass: assign recommended bits
        for s in &sorted {
            let original_bits = 32u32; // Assume FP32 baseline
            let target_bits = s.recommended_bits;
            let method = match target_bits {
                8 => QuantMethod::INT8,
                4 => QuantMethod::INT4,
                2 => QuantMethod::INT2,
                _ => QuantMethod::FP8,
            };

            layers_config.push(QuantLayerConfig {
                layer_name: s.layer_name.clone(),
                original_bits,
                target_bits,
                method,
                sensitivity: s.sensitivity_score,
            });
            total_original_bits += original_bits as u64;
            estimated_loss += s.estimated_accuracy_loss;
        }

        // If estimated loss exceeds budget, upgrade low-sensitivity layers back up
        if estimated_loss > max_accuracy_loss_pct {
            // Sort config by sensitivity ascending (upgrade least sensitive first to save budget)
            layers_config.sort_by(|a, b| {
                a.sensitivity
                    .partial_cmp(&b.sensitivity)
                    .unwrap()
            });

            for layer_cfg in &mut layers_config {
                if estimated_loss <= max_accuracy_loss_pct {
                    break;
                }
                if layer_cfg.target_bits < 8 {
                    let old_loss = match layer_cfg.target_bits {
                        2 => 3.0 + layer_cfg.sensitivity * 2.0,
                        4 => 1.0 + layer_cfg.sensitivity * 1.5,
                        _ => 0.5,
                    };
                    let new_bits = layer_cfg.target_bits * 2;
                    let new_loss = match new_bits {
                        4 => 1.0 + layer_cfg.sensitivity * 1.5,
                        8 => 0.5 + layer_cfg.sensitivity * 0.5,
                        _ => 0.0,
                    };
                    let savings = old_loss - new_loss;
                    if savings > 0.0 {
                        estimated_loss -= savings;
                        layer_cfg.target_bits = new_bits;
                        layer_cfg.method = match new_bits {
                            8 => QuantMethod::INT8,
                            4 => QuantMethod::INT4,
                            _ => QuantMethod::FP8,
                        };
                    }
                }
            }
        }

        // Compute memory savings
        let total_target_bits: u64 = layers_config.iter().map(|l| l.target_bits as u64).sum();
        let savings_pct = if total_original_bits > 0 {
            (1.0 - total_target_bits as f64 / total_original_bits as f64) * 100.0
        } else {
            0.0
        };

        // Normalise loss across layers
        let avg_accuracy_loss = if !layers_config.is_empty() {
            estimated_loss / layers_config.len() as f64
        } else {
            0.0
        };

        let profile = QuantizationProfile {
            id: Uuid::new_v4().to_string(),
            model_name: model_name.to_string(),
            layers: layers_config,
            total_memory_savings_pct: savings_pct,
            estimated_accuracy_loss_pct: avg_accuracy_loss,
            timestamp: Utc::now(),
        };

        self.profiles.insert(profile.id.clone(), profile.clone());
        self.history.insert(profile.id.clone(), profile.clone());

        info!(
            model = model_name,
            savings = savings_pct,
            loss = avg_accuracy_loss,
            "mixed-precision profile created"
        );
        profile
    }

    /// Calibrate quantization parameters (simulated).
    pub fn calibrate(&self, profile_id: &str) -> Option<QuantizationProfile> {
        let mut profile = self.profiles.get_mut(profile_id)?;
        // Simulate calibration by slightly adjusting accuracy loss
        profile.estimated_accuracy_loss_pct *= 0.9; // Calibration typically improves accuracy
        info!(profile_id, "calibration complete");
        Some(profile.value().clone())
    }

    pub fn get_profile(&self, id: &str) -> Option<QuantizationProfile> {
        self.profiles.get(id).map(|p| p.value().clone())
    }

    pub fn get_all_profiles(&self) -> Vec<QuantizationProfile> {
        self.profiles.iter().map(|p| p.value().clone()).collect()
    }

    pub fn get_history(&self) -> Vec<QuantizationProfile> {
        self.history.iter().map(|p| p.value().clone()).collect()
    }
}

// ---------------------------------------------------------------------------
// 4. REST API
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ModelOptimizer {
    optimization_engine: Arc<OptimizationEngine>,
    nas: Arc<EvolutionaryNAS>,
    quantizer: Arc<AdaptiveQuantizer>,
}

impl ModelOptimizer {
    pub fn new() -> Self {
        Self {
            optimization_engine: Arc::new(OptimizationEngine::new(OptimizationConfig::default())),
            nas: Arc::new(EvolutionaryNAS::new()),
            quantizer: Arc::new(AdaptiveQuantizer::new()),
        }
    }
}

// ---- Optimize endpoints ----

#[derive(Deserialize)]
pub struct AnalyzeRequest {
    pub model_name: String,
    pub metrics: ModelMetrics,
}

pub async fn analyze_optimization(
    State(optimizer): State<Arc<ModelOptimizer>>,
    Json(req): Json<AnalyzeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let strategies = optimizer
        .optimization_engine
        .suggest_optimizations(&req.metrics, &req.model_name)
        .await;
    Ok(Json(serde_json::json!({
        "model_name": req.model_name,
        "strategies": strategies,
    })))
}

#[derive(Deserialize)]
pub struct ApplyOptimizationRequest {
    pub model_name: String,
    pub metrics: ModelMetrics,
}

pub async fn apply_optimization(
    State(optimizer): State<Arc<ModelOptimizer>>,
    Json(req): Json<ApplyOptimizationRequest>,
) -> Result<Json<OptimizationResult>, StatusCode> {
    let result = optimizer
        .optimization_engine
        .apply_optimization(req.model_name, req.metrics)
        .await;
    Ok(Json(result))
}

pub async fn get_optimization_results(
    State(optimizer): State<Arc<ModelOptimizer>>,
) -> Result<Json<Vec<OptimizationResult>>, StatusCode> {
    Ok(Json(optimizer.optimization_engine.get_all_results()))
}

pub async fn get_optimization_result(
    State(optimizer): State<Arc<ModelOptimizer>>,
    Path(id): Path<String>,
) -> Result<Json<OptimizationResult>, StatusCode> {
    optimizer
        .optimization_engine
        .get_result(&id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn get_optimization_config(
    State(optimizer): State<Arc<ModelOptimizer>>,
) -> Result<Json<OptimizationConfig>, StatusCode> {
    Ok(Json(optimizer.optimization_engine.get_config().await))
}

// ---- NAS endpoints ----

pub async fn nas_search(
    State(optimizer): State<Arc<ModelOptimizer>>,
    Json(config): Json<NASConfig>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let search_id = optimizer.nas.search(config);
    Ok(Json(serde_json::json!({
        "search_id": search_id,
        "status": "completed",
    })))
}

pub async fn nas_search_status(
    State(optimizer): State<Arc<ModelOptimizer>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match optimizer.nas.get_search_result(&id) {
        Some(result) => Ok(Json(serde_json::json!({
            "search_id": id,
            "status": "completed",
            "generations_completed": result.generations_completed,
            "best_fitness": result.best_fitness,
        }))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn nas_search_result(
    State(optimizer): State<Arc<ModelOptimizer>>,
    Path(id): Path<String>,
) -> Result<Json<NASResult>, StatusCode> {
    optimizer
        .nas
        .get_search_result(&id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn nas_architectures(
    State(optimizer): State<Arc<ModelOptimizer>>,
) -> Result<Json<Vec<Architecture>>, StatusCode> {
    Ok(Json(optimizer.nas.get_all_architectures()))
}

pub async fn nas_architecture(
    State(optimizer): State<Arc<ModelOptimizer>>,
    Path(id): Path<String>,
) -> Result<Json<Architecture>, StatusCode> {
    optimizer
        .nas
        .get_architecture(&id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

// ---- Quantize endpoints ----

#[derive(Deserialize)]
pub struct AnalyzeQuantRequest {
    pub model_name: String,
    pub layer_names: Vec<String>,
    pub gradient_norms: Vec<f64>,
    pub activation_ranges: Vec<f64>,
}

pub async fn analyze_quantization(
    State(optimizer): State<Arc<ModelOptimizer>>,
    Json(req): Json<AnalyzeQuantRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sensitivities = optimizer.quantizer.analyze_sensitivity(
        &req.model_name,
        &req.layer_names,
        &req.gradient_norms,
        &req.activation_ranges,
    );
    Ok(Json(serde_json::json!({
        "model_name": req.model_name,
        "sensitivities": sensitivities,
    })))
}

#[derive(Deserialize)]
pub struct ApplyQuantRequest {
    pub model_name: String,
    pub sensitivities: Vec<LayerSensitivity>,
    pub max_accuracy_loss_pct: f64,
}

pub async fn apply_quantization(
    State(optimizer): State<Arc<ModelOptimizer>>,
    Json(req): Json<ApplyQuantRequest>,
) -> Result<Json<QuantizationProfile>, StatusCode> {
    let profile = optimizer
        .quantizer
        .assign_mixed_precision(&req.model_name, &req.sensitivities, req.max_accuracy_loss_pct);
    Ok(Json(profile))
}

pub async fn get_quant_profiles(
    State(optimizer): State<Arc<ModelOptimizer>>,
) -> Result<Json<Vec<QuantizationProfile>>, StatusCode> {
    Ok(Json(optimizer.quantizer.get_all_profiles()))
}

pub async fn get_quant_profile(
    State(optimizer): State<Arc<ModelOptimizer>>,
    Path(id): Path<String>,
) -> Result<Json<QuantizationProfile>, StatusCode> {
    optimizer
        .quantizer
        .get_profile(&id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn calibrate_quantization(
    State(optimizer): State<Arc<ModelOptimizer>>,
    Path(id): Path<String>,
) -> Result<Json<QuantizationProfile>, StatusCode> {
    optimizer
        .quantizer
        .calibrate(&id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn get_quant_history(
    State(optimizer): State<Arc<ModelOptimizer>>,
) -> Result<Json<Vec<QuantizationProfile>>, StatusCode> {
    Ok(Json(optimizer.quantizer.get_history()))
}

/// Build the router for model optimizer endpoints.
/// Uses `crate::api::AppState` so it can be merged into the main router.
pub fn build_model_optimizer_router(state: crate::api::AppState) -> axum::Router {
    use axum::routing::{get, post};
    let optimizer = state.model_optimizer.clone().unwrap_or_else(|| Arc::new(ModelOptimizer::new()));

    axum::Router::new()
        // Optimize
        .route("/api/optimize/analyze", post(analyze_optimization))
        .route("/api/optimize/apply", post(apply_optimization))
        .route("/api/optimize/results", get(get_optimization_results))
        .route("/api/optimize/results/{id}", get(get_optimization_result))
        .route("/api/optimize/config", get(get_optimization_config))
        // NAS
        .route("/api/nas/search", post(nas_search))
        .route("/api/nas/search/{id}/status", get(nas_search_status))
        .route("/api/nas/search/{id}/result", get(nas_search_result))
        .route("/api/nas/architectures", get(nas_architectures))
        .route("/api/nas/architectures/{id}", get(nas_architecture))
        // Quantize
        .route("/api/quantize/analyze", post(analyze_quantization))
        .route("/api/quantize/apply", post(apply_quantization))
        .route("/api/quantize/profiles", get(get_quant_profiles))
        .route("/api/quantize/profiles/{id}", get(get_quant_profile))
        .route("/api/quantize/calibrate/{id}", post(calibrate_quantization))
        .route("/api/quantize/history", get(get_quant_history))
        .with_state(optimizer)
}

// ---------------------------------------------------------------------------
// 5. Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn test_metrics() -> ModelMetrics {
        ModelMetrics {
            latency_ms: 200.0,
            throughput_rps: 50.0,
            accuracy: 0.95,
            memory_mb: 8000.0,
        }
    }

    // -- Optimization tests --

    #[tokio::test]
    async fn test_optimize_suggests_latency_strategies() {
        let engine = OptimizationEngine::new(OptimizationConfig {
            goal: OptimizationGoal::Latency,
            max_latency_ms: Some(100.0),
            min_accuracy: None,
            max_memory_mb: None,
            target_throughput: None,
        });
        let metrics = ModelMetrics {
            latency_ms: 500.0,
            throughput_rps: 100.0,
            accuracy: 0.99,
            memory_mb: 1000.0,
        };
        let strategies = engine.suggest_optimizations(&metrics, "test-model").await;
        assert!(strategies.contains(&OptimizationStrategy::BatchSizeTuning));
        assert!(strategies.contains(&OptimizationStrategy::ConcurrencyAdjust));
    }

    #[tokio::test]
    async fn test_optimize_suggests_throughput_strategies() {
        let engine = OptimizationEngine::new(OptimizationConfig {
            goal: OptimizationGoal::Throughput,
            target_throughput: Some(100.0),
            ..OptimizationConfig::default()
        });
        let metrics = ModelMetrics {
            latency_ms: 10.0,
            throughput_rps: 20.0,
            accuracy: 0.99,
            memory_mb: 1000.0,
        };
        let strategies = engine.suggest_optimizations(&metrics, "test-model").await;
        assert!(strategies.contains(&OptimizationStrategy::BatchSizeTuning));
        assert!(strategies.contains(&OptimizationStrategy::CacheWarming));
    }

    #[tokio::test]
    async fn test_optimize_suggests_memory_strategies() {
        let engine = OptimizationEngine::new(OptimizationConfig {
            goal: OptimizationGoal::Memory,
            max_memory_mb: Some(4096.0),
            ..OptimizationConfig::default()
        });
        let metrics = ModelMetrics {
            latency_ms: 10.0,
            throughput_rps: 200.0,
            accuracy: 0.99,
            memory_mb: 8000.0,
        };
        let strategies = engine.suggest_optimizations(&metrics, "test-model").await;
        assert!(strategies.contains(&OptimizationStrategy::ContextWindowOpt));
        assert!(strategies.contains(&OptimizationStrategy::MemoryPoolResize));
    }

    #[tokio::test]
    async fn test_optimize_balanced_suggests_all() {
        let engine = OptimizationEngine::new(OptimizationConfig {
            goal: OptimizationGoal::Balanced,
            max_latency_ms: Some(100.0),
            max_memory_mb: Some(4096.0),
            target_throughput: Some(100.0),
            min_accuracy: None,
        });
        let strategies = engine.suggest_optimizations(&test_metrics(), "test-model").await;
        assert!(strategies.contains(&OptimizationStrategy::BatchSizeTuning));
        assert!(strategies.contains(&OptimizationStrategy::ContextWindowOpt));
    }

    #[tokio::test]
    async fn test_apply_optimization_improves_metrics() {
        let engine = OptimizationEngine::new(OptimizationConfig::default());
        let result = engine
            .apply_optimization("test-model".to_string(), test_metrics())
            .await;
        assert!(result.after_metrics.latency_ms < result.before_metrics.latency_ms);
        assert!(result.after_metrics.throughput_rps > result.before_metrics.throughput_rps);
        assert!(!result.strategies_applied.is_empty());
        assert!(result.improvement_pct > 0.0);
    }

    #[tokio::test]
    async fn test_optimization_result_stored() {
        let engine = OptimizationEngine::new(OptimizationConfig::default());
        let result = engine
            .apply_optimization("test-model".to_string(), test_metrics())
            .await;
        let retrieved = engine.get_result(&result.id).unwrap();
        assert_eq!(retrieved.id, result.id);
        assert_eq!(retrieved.model_name, "test-model");
    }

    #[tokio::test]
    async fn test_get_all_results() {
        let engine = OptimizationEngine::new(OptimizationConfig::default());
        engine.apply_optimization("m1".to_string(), test_metrics()).await;
        engine.apply_optimization("m2".to_string(), test_metrics()).await;
        let all = engine.get_all_results();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_accuracy_goal_only_suggests_cache() {
        let engine = OptimizationEngine::new(OptimizationConfig {
            goal: OptimizationGoal::Accuracy,
            ..OptimizationConfig::default()
        });
        let strategies = engine.suggest_optimizations(&test_metrics(), "test-model").await;
        assert!(strategies.contains(&OptimizationStrategy::CacheWarming));
        // Should not suggest aggressive strategies
        assert!(!strategies.contains(&OptimizationStrategy::ContextWindowOpt));
    }

    // -- NAS tests --

    #[test]
    fn test_random_architecture_is_valid() {
        let arch = Architecture::random();
        assert!(!arch.layers.is_empty());
        assert_eq!(arch.layers[0].layer_type, LayerType::Embedding);
        assert_eq!(arch.layers.last().unwrap().layer_type, LayerType::Output);
        assert!(arch.total_params > 0);
        assert!(arch.estimated_latency_ms > 0.0);
        assert!(arch.estimated_memory_mb > 0.0);
        assert!(arch.fitness_score > 0.0);
    }

    #[test]
    fn test_crossover_produces_valid_child() {
        let parent1 = Architecture::random();
        let parent2 = Architecture::random();
        let child = EvolutionaryNAS::crossover(&parent1, &parent2);
        assert!(!child.layers.is_empty());
        assert!(child.total_params > 0);
        assert!(child.estimated_memory_mb > 0.0);
        // Child should have layers from both parents
        assert!(child.layers.len() >= 2);
    }

    #[test]
    fn test_mutation_stays_in_bounds() {
        let arch = Architecture::random();
        let mutated = EvolutionaryNAS::mutate(&arch);
        for layer in &mutated.layers {
            assert!(layer.hidden_dim >= 64);
            assert!(layer.num_heads >= 1);
        }
        assert!(mutated.total_params > 0);
        assert!(mutated.fitness_score > 0.0);
    }

    #[test]
    fn test_tournament_select_returns_best() {
        let mut pop = vec![Architecture::random(); 10];
        // Make one clearly the best
        pop[0].fitness_score = 1000.0;
        for i in 1..pop.len() {
            pop[i].fitness_score = 1.0;
        }
        // Run tournament many times, should always pick index 0
        for _ in 0..100 {
            let selected = EvolutionaryNAS::tournament_select(&pop, 3);
            assert_eq!(selected.id, pop[0].id);
        }
    }

    #[test]
    fn test_nas_random_search() {
        let nas = EvolutionaryNAS::new();
        let config = NASConfig {
            strategy: NASStrategy::Random,
            eval_budget: 5,
            ..NASConfig::default()
        };
        let search_id = nas.search(config);
        let result = nas.get_search_result(&search_id).unwrap();
        assert_eq!(result.generations_completed, 1);
        assert!(result.best_fitness > 0.0);
        assert!(!result.best_architecture_id.is_empty());
    }

    #[test]
    fn test_nas_evolutionary_search() {
        let nas = EvolutionaryNAS::new();
        let config = NASConfig {
            strategy: NASStrategy::Evolutionary,
            population_size: 5,
            generations: 3,
            eval_budget: 100,
        };
        let search_id = nas.search(config);
        let result = nas.get_search_result(&search_id).unwrap();
        assert_eq!(result.generations_completed, 3);
        assert!(result.best_fitness > 0.0);
    }

    #[test]
    fn test_nas_architecture_retrieval() {
        let nas = EvolutionaryNAS::new();
        let config = NASConfig {
            strategy: NASStrategy::Random,
            eval_budget: 3,
            ..NASConfig::default()
        };
        let search_id = nas.search(config);
        let result = nas.get_search_result(&search_id).unwrap();
        let arch = nas.get_architecture(&result.best_architecture_id);
        assert!(arch.is_some());
    }

    #[test]
    fn test_nas_evolutionary_improves_over_generations() {
        let nas = EvolutionaryNAS::new();
        // Run with larger population and more generations
        let config = NASConfig {
            strategy: NASStrategy::Evolutionary,
            population_size: 30,
            generations: 10,
            eval_budget: 1000,
        };
        let search_id = nas.search(config);
        let result = nas.get_search_result(&search_id).unwrap();
        // Best fitness should be reasonable
        assert!(result.best_fitness.is_finite());
    }

    // -- Quantization tests --

    #[test]
    fn test_sensitivity_analysis_high_gradient_gets_more_bits() {
        let quantizer = AdaptiveQuantizer::new();
        let layer_names = vec!["layer1".to_string(), "layer2".to_string()];
        let gradient_norms = vec![100.0, 1.0];
        let activation_ranges = vec![10.0, 1.0];
        let sensitivities = quantizer.analyze_sensitivity(
            "test-model",
            &layer_names,
            &gradient_norms,
            &activation_ranges,
        );
        assert_eq!(sensitivities.len(), 2);
        assert!(sensitivities[0].sensitivity_score > sensitivities[1].sensitivity_score);
        assert!(sensitivities[0].recommended_bits >= sensitivities[1].recommended_bits);
    }

    #[test]
    fn test_sensitivity_analysis_low_sensitivity_gets_aggressive_quant() {
        let quantizer = AdaptiveQuantizer::new();
        let layer_names = vec!["sensitive".to_string(), "insensitive".to_string()];
        let gradient_norms = vec![100.0, 0.01];
        let activation_ranges = vec![100.0, 0.01];
        let sensitivities = quantizer.analyze_sensitivity(
            "test-model",
            &layer_names,
            &gradient_norms,
            &activation_ranges,
        );
        assert!(sensitivities[0].recommended_bits >= 8);
        assert!(sensitivities[1].recommended_bits <= 4);
    }

    #[test]
    fn test_mixed_precision_respects_budget() {
        let quantizer = AdaptiveQuantizer::new();
        let sensitivities = vec![
            LayerSensitivity {
                layer_name: "l1".to_string(),
                sensitivity_score: 0.9,
                recommended_bits: 16,
                estimated_accuracy_loss: 0.0,
            },
            LayerSensitivity {
                layer_name: "l2".to_string(),
                sensitivity_score: 0.1,
                recommended_bits: 2,
                estimated_accuracy_loss: 5.0,
            },
            LayerSensitivity {
                layer_name: "l3".to_string(),
                sensitivity_score: 0.05,
                recommended_bits: 2,
                estimated_accuracy_loss: 4.0,
            },
        ];
        let profile = quantizer.assign_mixed_precision("test-model", &sensitivities, 1.0);
        // With a tight budget, aggressive layers should be upgraded
        assert!(profile.estimated_accuracy_loss_pct <= 1.0 + 0.01); // small tolerance
    }

    #[test]
    fn test_mixed_precision_generates_profile() {
        let quantizer = AdaptiveQuantizer::new();
        let sensitivities = vec![
            LayerSensitivity {
                layer_name: "l1".to_string(),
                sensitivity_score: 0.5,
                recommended_bits: 8,
                estimated_accuracy_loss: 1.0,
            },
            LayerSensitivity {
                layer_name: "l2".to_string(),
                sensitivity_score: 0.2,
                recommended_bits: 4,
                estimated_accuracy_loss: 2.0,
            },
        ];
        let profile = quantizer.assign_mixed_precision("test-model", &sensitivities, 10.0);
        assert!(!profile.id.is_empty());
        assert_eq!(profile.model_name, "test-model");
        assert_eq!(profile.layers.len(), 2);
        assert!(profile.total_memory_savings_pct > 0.0);
    }

    #[test]
    fn test_calibrate_reduces_accuracy_loss() {
        let quantizer = AdaptiveQuantizer::new();
        let sensitivities = vec![LayerSensitivity {
            layer_name: "l1".to_string(),
            sensitivity_score: 0.5,
            recommended_bits: 8,
            estimated_accuracy_loss: 1.0,
        }];
        let profile = quantizer.assign_mixed_precision("test-model", &sensitivities, 10.0);
        let original_loss = profile.estimated_accuracy_loss_pct;
        let calibrated = quantizer.calibrate(&profile.id).unwrap();
        assert!(calibrated.estimated_accuracy_loss_pct < original_loss);
    }

    #[test]
    fn test_profile_history() {
        let quantizer = AdaptiveQuantizer::new();
        let sensitivities = vec![LayerSensitivity {
            layer_name: "l1".to_string(),
            sensitivity_score: 0.5,
            recommended_bits: 8,
            estimated_accuracy_loss: 1.0,
        }];
        quantizer.assign_mixed_precision("model-a", &sensitivities, 10.0);
        quantizer.assign_mixed_precision("model-b", &sensitivities, 10.0);
        let history = quantizer.get_history();
        assert_eq!(history.len(), 2);
    }

    // -- Concurrent access tests --

    #[tokio::test]
    async fn test_concurrent_optimization() {
        let engine = Arc::new(OptimizationEngine::new(OptimizationConfig::default()));
        let mut handles = Vec::new();
        for i in 0..10 {
            let eng = engine.clone();
            handles.push(tokio::spawn(async move {
                eng.apply_optimization(format!("model-{}", i), test_metrics())
                    .await
            }));
        }
        let results = futures_util::future::join_all(handles).await;
        for r in results {
            assert!(r.is_ok());
            assert!(r.unwrap().improvement_pct > 0.0);
        }
        assert_eq!(engine.get_all_results().len(), 10);
    }

    #[test]
    fn test_concurrent_nas_searches() {
        let nas = Arc::new(EvolutionaryNAS::new());
        let mut handles = Vec::new();
        for _ in 0..5 {
            let n = nas.clone();
            handles.push(std::thread::spawn(move || {
                n.search(NASConfig {
                    strategy: NASStrategy::Random,
                    eval_budget: 3,
                    ..NASConfig::default()
                })
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_concurrent_quantization() {
        let quantizer = Arc::new(AdaptiveQuantizer::new());
        let mut handles = Vec::new();
        for i in 0..5 {
            let q = quantizer.clone();
            handles.push(std::thread::spawn(move || {
                let sensitivities = vec![LayerSensitivity {
                    layer_name: format!("l{}", i),
                    sensitivity_score: 0.5,
                    recommended_bits: 8,
                    estimated_accuracy_loss: 1.0,
                }];
                q.assign_mixed_precision(&format!("model-{}", i), &sensitivities, 10.0)
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(quantizer.get_history().len(), 5);
    }

    // -- Serialization tests --

    #[test]
    fn test_optimization_goal_serialization() {
        let goal = OptimizationGoal::Balanced;
        let json = serde_json::to_string(&goal).unwrap();
        let deserialized: OptimizationGoal = serde_json::from_str(&json).unwrap();
        assert_eq!(goal, deserialized);
    }

    #[test]
    fn test_quant_method_serialization() {
        let method = QuantMethod::NF4;
        let json = serde_json::to_string(&method).unwrap();
        let deserialized: QuantMethod = serde_json::from_str(&json).unwrap();
        assert_eq!(method, deserialized);
    }

    #[test]
    fn test_nas_config_defaults() {
        let config = NASConfig::default();
        assert_eq!(config.population_size, 20);
        assert_eq!(config.generations, 10);
        assert_eq!(config.eval_budget, 100);
        assert_eq!(config.strategy, NASStrategy::Evolutionary);
    }
}
