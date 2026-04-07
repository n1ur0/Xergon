use std::collections::HashMap;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ===========================================================================
// ComparisonDimension
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ComparisonDimension {
    Performance,
    Cost,
    Latency,
    Quality,
    Privacy,
    Features,
}

impl ComparisonDimension {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Performance => "performance",
            Self::Cost => "cost",
            Self::Latency => "latency",
            Self::Quality => "quality",
            Self::Privacy => "privacy",
            Self::Features => "features",
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::Performance,
            Self::Cost,
            Self::Latency,
            Self::Quality,
            Self::Privacy,
            Self::Features,
        ]
    }
}

// ===========================================================================
// NormalizationMethod
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum NormalizationMethod {
    MinMax,
    ZScore,
}

// ===========================================================================
// ModelSpec
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelSpec {
    pub model_id: String,
    pub name: String,
    pub provider: String,
    pub dimensions: HashMap<String, f64>,
    pub features: Vec<String>,
    pub pricing: ModelPricing,
    pub benchmarks: HashMap<String, f64>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ModelPricing {
    pub input_price_per_million: f64,
    pub output_price_per_million: f64,
    pub currency: String,
}

// ===========================================================================
// ComparisonConfig
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ComparisonConfig {
    pub weights: HashMap<String, f64>,
    pub normalization: NormalizationMethod,
    pub include_benchmarks: bool,
}

impl Default for ComparisonConfig {
    fn default() -> Self {
        let mut weights = HashMap::new();
        weights.insert(ComparisonDimension::Performance.as_str().to_string(), 0.25);
        weights.insert(ComparisonDimension::Cost.as_str().to_string(), 0.20);
        weights.insert(ComparisonDimension::Latency.as_str().to_string(), 0.15);
        weights.insert(ComparisonDimension::Quality.as_str().to_string(), 0.25);
        weights.insert(ComparisonDimension::Privacy.as_str().to_string(), 0.05);
        weights.insert(ComparisonDimension::Features.as_str().to_string(), 0.10);
        Self {
            weights,
            normalization: NormalizationMethod::MinMax,
            include_benchmarks: true,
        }
    }
}

// ===========================================================================
// ComparisonResult
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ComparisonResult {
    pub comparison_id: String,
    pub model_id: String,
    pub scores: HashMap<String, f64>,
    pub overall_score: f64,
    pub rank: usize,
    pub highlights: Vec<String>,
    pub weaknesses: Vec<String>,
}

// ===========================================================================
// ComparisonMatrix
// ===========================================================================

const CONFIG_KEY: &str = "default";

#[derive(Debug)]
pub struct ComparisonMatrix {
    models: DashMap<String, ModelSpec>,
    results_cache: DashMap<String, Vec<ComparisonResult>>,
    config: DashMap<String, ComparisonConfig>,
}

impl ComparisonMatrix {
    pub fn new() -> Self {
        let dm = DashMap::new();
        dm.insert(CONFIG_KEY.to_string(), ComparisonConfig::default());
        Self {
            models: DashMap::new(),
            results_cache: DashMap::new(),
            config: dm,
        }
    }

    fn get_config(&self) -> ComparisonConfig {
        self.config
            .get(CONFIG_KEY)
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    fn set_config(&self, cfg: ComparisonConfig) {
        self.config.insert(CONFIG_KEY.to_string(), cfg);
    }

    /// Add a model spec to the matrix.
    pub fn add_model(&self, spec: ModelSpec) -> Result<(), String> {
        if spec.model_id.is_empty() {
            return Err("model_id cannot be empty".to_string());
        }
        self.models.insert(spec.model_id.clone(), spec);
        Ok(())
    }

    /// Remove a model spec from the matrix.
    pub fn remove_model(&self, model_id: &str) -> bool {
        self.models.remove(model_id).is_some()
    }

    /// Compare a set of models using the current config.
    pub fn compare(&self, model_ids: &[&str]) -> Result<Vec<ComparisonResult>, String> {
        let config = self.get_config();
        self.compare_with_config(model_ids, &config)
    }

    /// Get the full matrix of all registered models.
    pub fn get_matrix(&self) -> Vec<ModelSpec> {
        self.models.iter().map(|m| m.value().clone()).collect()
    }

    /// Get a cached comparison result.
    pub fn get_result(&self, comparison_id: &str) -> Option<Vec<ComparisonResult>> {
        self.results_cache.get(comparison_id).map(|r| r.clone())
    }

    /// Get a recommendation for a specific use case by adjusting weights.
    pub fn get_recommendation(
        &self,
        model_ids: &[&str],
        use_case: &str,
    ) -> Result<Vec<ComparisonResult>, String> {
        let use_case_lower = use_case.to_lowercase();
        let mut custom_config = self.get_config();

        // Adjust weights based on use case
        match use_case_lower.as_str() {
            "production" | "reliability" => {
                custom_config.weights.insert(
                    ComparisonDimension::Performance.as_str().to_string(),
                    0.35,
                );
                custom_config
                    .weights
                    .insert(ComparisonDimension::Quality.as_str().to_string(), 0.30);
                custom_config
                    .weights
                    .insert(ComparisonDimension::Cost.as_str().to_string(), 0.10);
            }
            "cost_optimized" | "budget" => {
                custom_config
                    .weights
                    .insert(ComparisonDimension::Cost.as_str().to_string(), 0.45);
                custom_config.weights.insert(
                    ComparisonDimension::Latency.as_str().to_string(),
                    0.20,
                );
            }
            "privacy_first" | "privacy" => {
                custom_config
                    .weights
                    .insert(ComparisonDimension::Privacy.as_str().to_string(), 0.40);
                custom_config.weights.insert(
                    ComparisonDimension::Performance.as_str().to_string(),
                    0.20,
                );
            }
            "low_latency" | "realtime" => {
                custom_config.weights.insert(
                    ComparisonDimension::Latency.as_str().to_string(),
                    0.40,
                );
                custom_config.weights.insert(
                    ComparisonDimension::Performance.as_str().to_string(),
                    0.30,
                );
            }
            _ => {}
        }

        // Normalize weights
        let total: f64 = custom_config.weights.values().sum();
        if total > 0.0 {
            for v in custom_config.weights.values_mut() {
                *v /= total;
            }
        }

        self.compare_with_config(model_ids, &custom_config)
    }

    /// Update dimension weights.
    pub fn update_weights(&self, weights: HashMap<String, f64>) {
        let mut cfg = self.get_config();
        for (k, v) in weights {
            cfg.weights.insert(k, v);
        }
        self.set_config(cfg);
    }

    /// Export comparison data as a serializable value.
    pub fn export_comparison(
        &self,
        comparison_id: &str,
    ) -> Result<serde_json::Value, String> {
        let results = self
            .get_result(comparison_id)
            .ok_or("comparison_id not found")?;
        Ok(serde_json::to_value(&results).unwrap_or_default())
    }

    /// Get the current config (public).
    pub fn get_public_config(&self) -> ComparisonConfig {
        self.get_config()
    }

    /// Get available dimensions.
    pub fn get_dimensions(&self) -> Vec<ComparisonDimension> {
        ComparisonDimension::all()
    }

    // -- Internal methods --

    fn compare_with_config(
        &self,
        model_ids: &[&str],
        config: &ComparisonConfig,
    ) -> Result<Vec<ComparisonResult>, String> {
        if model_ids.is_empty() {
            return Err("no model_ids provided".to_string());
        }

        // Collect specs
        let specs: Vec<ModelSpec> = model_ids
            .iter()
            .filter_map(|id| self.models.get(*id).map(|m| m.clone()))
            .collect();

        if specs.is_empty() {
            return Err("none of the specified models found".to_string());
        }

        // Collect all dimension keys across models
        let dim_keys: Vec<String> = specs
            .iter()
            .flat_map(|s| s.dimensions.keys().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Normalize scores
        let normalized: Vec<HashMap<String, f64>> = match config.normalization {
            NormalizationMethod::MinMax => Self::normalize_minmax(&specs, &dim_keys),
            NormalizationMethod::ZScore => Self::normalize_zscore(&specs, &dim_keys),
        };

        // Compute weighted scores
        // Generate a single comparison_id for both cache and results
        let comparison_id = uuid::Uuid::new_v4().to_string();

        let mut results: Vec<ComparisonResult> = Vec::new();
        for (i, spec) in specs.iter().enumerate() {
            let scores = &normalized[i];
            let overall = Self::compute_weighted_score(scores, &config.weights);

            let (highlights, weaknesses) = Self::find_highlights_weaknesses(scores, &dim_keys);

            results.push(ComparisonResult {
                comparison_id: comparison_id.clone(),
                model_id: spec.model_id.clone(),
                scores: scores.clone(),
                overall_score: overall,
                rank: 0,
                highlights,
                weaknesses,
            });
        }

        // Sort by overall score descending
        results.sort_by(|a, b| {
            b.overall_score
                .partial_cmp(&a.overall_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Assign ranks
        for (rank, result) in results.iter_mut().enumerate() {
            result.rank = rank + 1;
        }

        self.results_cache
            .insert(comparison_id.clone(), results.clone());

        Ok(results)
    }

    fn normalize_minmax(
        specs: &[ModelSpec],
        dim_keys: &[String],
    ) -> Vec<HashMap<String, f64>> {
        let mut result = Vec::with_capacity(specs.len());

        let mut min_max: HashMap<String, (f64, f64)> = HashMap::new();
        for key in dim_keys {
            let values: Vec<f64> = specs
                .iter()
                .filter_map(|s| s.dimensions.get(key).copied())
                .collect();
            if values.len() >= 2 {
                let mut min_val = f64::INFINITY;
                let mut max_val = f64::NEG_INFINITY;
                for &v in &values {
                    if v < min_val {
                        min_val = v;
                    }
                    if v > max_val {
                        max_val = v;
                    }
                }
                min_max.insert(key.clone(), (min_val, max_val));
            } else if values.len() == 1 {
                min_max.insert(key.clone(), (values[0], values[0]));
            }
        }

        for spec in specs {
            let mut normalized = HashMap::new();
            for key in dim_keys {
                let val = spec.dimensions.get(key).copied().unwrap_or(0.0);
                if let Some(&(min, max)) = min_max.get(key) {
                    if (max - min).abs() < f64::EPSILON {
                        normalized.insert(key.clone(), 1.0);
                    } else {
                        normalized.insert(key.clone(), (val - min) / (max - min));
                    }
                } else {
                    normalized.insert(key.clone(), 0.0);
                }
            }
            result.push(normalized);
        }

        result
    }

    fn normalize_zscore(
        specs: &[ModelSpec],
        dim_keys: &[String],
    ) -> Vec<HashMap<String, f64>> {
        let mut result = Vec::with_capacity(specs.len());

        let mut stats: HashMap<String, (f64, f64)> = HashMap::new();
        for key in dim_keys {
            let values: Vec<f64> = specs
                .iter()
                .filter_map(|s| s.dimensions.get(key).copied())
                .collect();
            if values.len() > 1 {
                let mean = values.iter().sum::<f64>() / values.len() as f64;
                let variance =
                    values.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                        / (values.len() - 1) as f64;
                let std_dev = variance.sqrt();
                stats.insert(key.clone(), (mean, std_dev));
            }
        }

        for spec in specs {
            let mut normalized = HashMap::new();
            for key in dim_keys {
                let val = spec.dimensions.get(key).copied().unwrap_or(0.0);
                if let Some(&(mean, std_dev)) = stats.get(key) {
                    if std_dev < f64::EPSILON {
                        normalized.insert(key.clone(), 0.0);
                    } else {
                        normalized.insert(key.clone(), (val - mean) / std_dev);
                    }
                } else {
                    normalized.insert(key.clone(), 0.0);
                }
            }
            result.push(normalized);
        }

        result
    }

    fn compute_weighted_score(
        scores: &HashMap<String, f64>,
        weights: &HashMap<String, f64>,
    ) -> f64 {
        let mut total = 0.0;
        let mut weight_sum = 0.0;
        for (key, &val) in scores {
            let weight = weights.get(key).copied().unwrap_or(0.0);
            total += val * weight;
            weight_sum += weight;
        }
        if weight_sum > 0.0 {
            total / weight_sum
        } else {
            0.0
        }
    }

    fn find_highlights_weaknesses(
        scores: &HashMap<String, f64>,
        dim_keys: &[String],
    ) -> (Vec<String>, Vec<String>) {
        let mut highlights = Vec::new();
        let mut weaknesses = Vec::new();

        for key in dim_keys {
            let val = scores.get(key).copied().unwrap_or(0.0);
            if val >= 0.8 {
                highlights.push(format!("Strong {}: {:.1}", key, val * 100.0));
            } else if val < 0.3 {
                weaknesses.push(format!("Weak {}: {:.1}", key, val * 100.0));
            }
        }

        (highlights, weaknesses)
    }
}

impl Default for ComparisonMatrix {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Request / Response DTOs
// ===========================================================================

#[derive(Deserialize)]
pub struct CompareRequest {
    pub model_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateWeightsRequest {
    pub weights: HashMap<String, f64>,
}

#[derive(Deserialize)]
pub struct RecommendRequest {
    pub model_ids: Vec<String>,
    pub use_case: String,
}

#[derive(Deserialize)]
pub struct AddModelRequest {
    pub model_id: String,
    pub name: String,
    pub provider: String,
    pub dimensions: HashMap<String, f64>,
    #[serde(default)]
    pub features: Vec<String>,
    pub pricing: Option<ModelPricing>,
    #[serde(default)]
    pub benchmarks: HashMap<String, f64>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_matrix() -> ComparisonMatrix {
        ComparisonMatrix::new()
    }

    fn make_spec(
        id: &str,
        name: &str,
        perf: f64,
        cost: f64,
        latency: f64,
        quality: f64,
    ) -> ModelSpec {
        let mut dimensions = HashMap::new();
        dimensions.insert(ComparisonDimension::Performance.as_str().to_string(), perf);
        dimensions.insert(ComparisonDimension::Cost.as_str().to_string(), cost);
        dimensions.insert(ComparisonDimension::Latency.as_str().to_string(), latency);
        dimensions.insert(ComparisonDimension::Quality.as_str().to_string(), quality);
        dimensions.insert(ComparisonDimension::Privacy.as_str().to_string(), 0.7);
        dimensions.insert(ComparisonDimension::Features.as_str().to_string(), 0.8);
        ModelSpec {
            model_id: id.to_string(),
            name: name.to_string(),
            provider: "test_provider".to_string(),
            dimensions,
            features: vec!["chat".to_string(), "completion".to_string()],
            pricing: ModelPricing::default(),
            benchmarks: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_add_and_get_models() {
        let matrix = make_matrix();
        matrix
            .add_model(make_spec("m1", "Model A", 0.9, 0.7, 0.8, 0.85))
            .unwrap();
        matrix
            .add_model(make_spec("m2", "Model B", 0.7, 0.9, 0.6, 0.75))
            .unwrap();
        let all = matrix.get_matrix();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_remove_model() {
        let matrix = make_matrix();
        matrix
            .add_model(make_spec("m1", "Model A", 0.9, 0.7, 0.8, 0.85))
            .unwrap();
        assert!(matrix.remove_model("m1"));
        assert_eq!(matrix.get_matrix().len(), 0);
        assert!(!matrix.remove_model("nonexistent"));
    }

    #[test]
    fn test_compare_two_models() {
        let matrix = make_matrix();
        matrix
            .add_model(make_spec("m1", "Model A", 0.95, 0.5, 0.8, 0.9))
            .unwrap();
        matrix
            .add_model(make_spec("m2", "Model B", 0.7, 0.9, 0.6, 0.75))
            .unwrap();

        let results = matrix.compare(&["m1", "m2"]).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].rank, 1);
        assert!(results[0].overall_score > 0.0);
    }

    #[test]
    fn test_compare_ranks() {
        let matrix = make_matrix();
        matrix
            .add_model(make_spec("m1", "Model A", 0.99, 0.99, 0.99, 0.99))
            .unwrap();
        matrix
            .add_model(make_spec("m2", "Model B", 0.1, 0.1, 0.1, 0.1))
            .unwrap();

        let results = matrix.compare(&["m1", "m2"]).unwrap();
        assert_eq!(results[0].model_id, "m1");
        assert_eq!(results[0].rank, 1);
        assert_eq!(results[1].model_id, "m2");
        assert_eq!(results[1].rank, 2);
    }

    #[test]
    fn test_compare_empty_ids() {
        let matrix = make_matrix();
        let result = matrix.compare(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_compare_nonexistent_models() {
        let matrix = make_matrix();
        let result = matrix.compare(&["fake1", "fake2"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cached_results() {
        let matrix = make_matrix();
        matrix
            .add_model(make_spec("m1", "Model A", 0.9, 0.7, 0.8, 0.85))
            .unwrap();
        let results = matrix.compare(&["m1"]).unwrap();
        let comp_id = &results[0].comparison_id;
        let cached = matrix.get_result(comp_id);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1);
    }

    #[test]
    fn test_highlights_and_weaknesses() {
        let matrix = make_matrix();
        matrix
            .add_model(make_spec("m1", "Model A", 0.95, 0.1, 0.2, 0.9))
            .unwrap();
        matrix
            .add_model(make_spec("m2", "Model B", 0.5, 0.95, 0.9, 0.5))
            .unwrap();
        let results = matrix.compare(&["m1", "m2"]).unwrap();
        // At least one model should have highlights
        let has_highlights = results
            .iter()
            .any(|r| !r.highlights.is_empty());
        let has_weaknesses = results
            .iter()
            .any(|r| !r.weaknesses.is_empty());
        assert!(has_highlights, "expected highlights in at least one model");
        assert!(has_weaknesses, "expected weaknesses in at least one model");
    }

    #[test]
    fn test_recommendation_production() {
        let matrix = make_matrix();
        matrix
            .add_model(make_spec("m1", "Model A", 0.95, 0.5, 0.7, 0.9))
            .unwrap();
        matrix
            .add_model(make_spec("m2", "Model B", 0.7, 0.95, 0.9, 0.7))
            .unwrap();

        let results = matrix
            .get_recommendation(&["m1", "m2"], "production")
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].model_id, "m1");
    }

    #[test]
    fn test_recommendation_cost_optimized() {
        let matrix = make_matrix();
        matrix
            .add_model(make_spec("m1", "Model A", 0.95, 0.1, 0.7, 0.9))
            .unwrap();
        matrix
            .add_model(make_spec("m2", "Model B", 0.5, 0.95, 0.9, 0.6))
            .unwrap();

        let results = matrix
            .get_recommendation(&["m1", "m2"], "cost_optimized")
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].model_id, "m2");
    }

    #[test]
    fn test_update_weights() {
        let matrix = make_matrix();
        let mut new_weights = HashMap::new();
        new_weights.insert(
            ComparisonDimension::Performance.as_str().to_string(),
            0.5,
        );
        new_weights.insert(ComparisonDimension::Cost.as_str().to_string(), 0.5);
        matrix.update_weights(new_weights);
        let config = matrix.get_public_config();
        assert_eq!(
            config
                .weights
                .get(ComparisonDimension::Performance.as_str())
                .copied()
                .unwrap(),
            0.5
        );
    }

    #[test]
    fn test_export_comparison() {
        let matrix = make_matrix();
        matrix
            .add_model(make_spec("m1", "Model A", 0.9, 0.7, 0.8, 0.85))
            .unwrap();
        let results = matrix.compare(&["m1"]).unwrap();
        let comp_id = &results[0].comparison_id;
        let exported = matrix.export_comparison(comp_id).unwrap();
        assert!(exported.is_array());
    }
}
