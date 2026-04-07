use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// TemplateCategory
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TemplateCategory {
    Llm,
    ImageGen,
    Audio,
    Embeddings,
    Code,
    Multimodal,
    Custom,
}

impl std::fmt::Display for TemplateCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Llm => write!(f, "LLM"),
            Self::ImageGen => write!(f, "ImageGen"),
            Self::Audio => write!(f, "Audio"),
            Self::Embeddings => write!(f, "Embeddings"),
            Self::Code => write!(f, "Code"),
            Self::Multimodal => write!(f, "Multimodal"),
            Self::Custom => write!(f, "Custom"),
        }
    }
}

// ---------------------------------------------------------------------------
// ResourceSpec
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ResourceSpec {
    pub min_ram_gb: f64,
    pub min_vram_gb: f64,
    pub min_gpu_count: u32,
    pub recommended_gpu: String,
    pub cpu_cores: u32,
    pub disk_gb: f64,
}

// ---------------------------------------------------------------------------
// ModelConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ModelConfig {
    pub context_size: u32,
    pub batch_size: u32,
    pub max_tokens: u32,
    pub temperature: f64,
    pub repeat_penalty: f64,
    pub gpu_layers: u32,
    pub quantization: String,
    pub rope_freq_base: f64,
    pub rope_scale: f64,
}

// ---------------------------------------------------------------------------
// DeploymentTemplate
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct DeploymentTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: TemplateCategory,
    pub model_id: String,
    pub model_version: String,
    pub resource_spec: ResourceSpec,
    pub config: ModelConfig,
    pub backend: String,
    pub tags: Vec<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub is_official: bool,
    downloads: AtomicU64,
    rating_sum: AtomicU64,
    rating_count: AtomicU64,
}

impl DeploymentTemplate {
    pub fn avg_rating(&self) -> f64 {
        let count = self.rating_count.load(Ordering::Relaxed);
        if count == 0 {
            return 0.0;
        }
        let sum = self.rating_sum.load(Ordering::Relaxed);
        sum as f64 / count as f64
    }

    pub fn downloads(&self) -> u64 {
        self.downloads.load(Ordering::Relaxed)
    }

    pub fn rating_count(&self) -> u64 {
        self.rating_count.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> DeploymentTemplateSnapshot {
        DeploymentTemplateSnapshot {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            category: self.category.clone(),
            model_id: self.model_id.clone(),
            model_version: self.model_version.clone(),
            resource_spec: self.resource_spec.clone(),
            config: self.config.clone(),
            backend: self.backend.clone(),
            tags: self.tags.clone(),
            created_by: self.created_by.clone(),
            created_at: self.created_at,
            is_official: self.is_official,
            downloads: self.downloads.load(Ordering::Relaxed),
            avg_rating: self.avg_rating(),
            rating_count: self.rating_count.load(Ordering::Relaxed),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeploymentTemplateSnapshot {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: TemplateCategory,
    pub model_id: String,
    pub model_version: String,
    pub resource_spec: ResourceSpec,
    pub config: ModelConfig,
    pub backend: String,
    pub tags: Vec<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub is_official: bool,
    pub downloads: u64,
    pub avg_rating: f64,
    pub rating_count: u64,
}

// ---------------------------------------------------------------------------
// TemplateFilter
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TemplateFilter {
    pub category: Option<TemplateCategory>,
    pub backend: Option<String>,
    pub min_rating: f64,
    pub tags: Vec<String>,
    pub search: String,
    pub is_official: Option<bool>,
}

// ---------------------------------------------------------------------------
// TemplateMetrics
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TemplateMetrics {
    pub total_templates: u64,
    pub total_categories: u32,
    pub total_downloads: u64,
    pub avg_rating: f64,
    pub official_templates: u64,
}

// ---------------------------------------------------------------------------
// TemplateRegistry
// ---------------------------------------------------------------------------

pub struct TemplateRegistry {
    templates: DashMap<String, Arc<DeploymentTemplate>>,
    categories: DashMap<String, Vec<String>>,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        let registry = Self {
            templates: DashMap::new(),
            categories: DashMap::new(),
        };
        registry.seed_defaults()
    }

    fn seed_defaults(self) -> Self {
        let mut reg = self;

        // Qwen3.5-4B
        reg.register(DeploymentTemplate {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Qwen3.5-4B Optimized".to_string(),
            description: "Lightweight LLM for fast inference, good for chat and code completion".to_string(),
            category: TemplateCategory::Llm,
            model_id: "qwen3.5-4b".to_string(),
            model_version: "1.0.0".to_string(),
            resource_spec: ResourceSpec { min_ram_gb: 8.0, min_vram_gb: 6.0, min_gpu_count: 1, recommended_gpu: "RTX 3060".to_string(), cpu_cores: 4, disk_gb: 10.0 },
            config: ModelConfig { context_size: 8192, batch_size: 32, max_tokens: 4096, temperature: 0.7, repeat_penalty: 1.1, gpu_layers: 0, quantization: "q4_0".to_string(), rope_freq_base: 10000.0, rope_scale: 1.0 },
            backend: "ollama".to_string(),
            tags: vec!["llm".into(), "chat".into(), "fast".into()],
            created_by: "xergon".to_string(),
            created_at: Utc::now(),
            is_official: true,
            downloads: AtomicU64::new(0),
            rating_sum: AtomicU64::new(0),
            rating_count: AtomicU64::new(0),
        }).ok();

        // Qwen3.5-32B
        reg.register(DeploymentTemplate {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Qwen3.5-32B Balanced".to_string(),
            description: "High-quality LLM for complex reasoning, code generation, and analysis".to_string(),
            category: TemplateCategory::Llm,
            model_id: "qwen3.5-32b".to_string(),
            model_version: "1.0.0".to_string(),
            resource_spec: ResourceSpec { min_ram_gb: 32.0, min_vram_gb: 24.0, min_gpu_count: 1, recommended_gpu: "RTX 4090".to_string(), cpu_cores: 8, disk_gb: 40.0 },
            config: ModelConfig { context_size: 8192, batch_size: 16, max_tokens: 4096, temperature: 0.7, repeat_penalty: 1.1, gpu_layers: 0, quantization: "q4_0".to_string(), rope_freq_base: 10000.0, rope_scale: 1.0 },
            backend: "ollama".to_string(),
            tags: vec!["llm".into(), "reasoning".into(), "code".into()],
            created_by: "xergon".to_string(),
            created_at: Utc::now(),
            is_official: true,
            downloads: AtomicU64::new(0),
            rating_sum: AtomicU64::new(0),
            rating_count: AtomicU64::new(0),
        }).ok();

        // Llama-3.1-8B
        reg.register(DeploymentTemplate {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Llama-3.1-8B Instruct".to_string(),
            description: "Meta's Llama 3.1 8B instruction-tuned model for general-purpose tasks".to_string(),
            category: TemplateCategory::Llm,
            model_id: "llama-3.1-8b".to_string(),
            model_version: "1.0.0".to_string(),
            resource_spec: ResourceSpec { min_ram_gb: 16.0, min_vram_gb: 10.0, min_gpu_count: 1, recommended_gpu: "RTX 3070".to_string(), cpu_cores: 4, disk_gb: 16.0 },
            config: ModelConfig { context_size: 8192, batch_size: 32, max_tokens: 4096, temperature: 0.6, repeat_penalty: 1.1, gpu_layers: 0, quantization: "q4_0".to_string(), rope_freq_base: 500000.0, rope_scale: 1.0 },
            backend: "ollama".to_string(),
            tags: vec!["llm".into(), "instruct".into(), "general".into()],
            created_by: "xergon".to_string(),
            created_at: Utc::now(),
            is_official: true,
            downloads: AtomicU64::new(0),
            rating_sum: AtomicU64::new(0),
            rating_count: AtomicU64::new(0),
        }).ok();

        // Mistral-7B
        reg.register(DeploymentTemplate {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Mistral-7B v0.3".to_string(),
            description: "Efficient 7B model with strong performance across benchmarks".to_string(),
            category: TemplateCategory::Llm,
            model_id: "mistral-7b".to_string(),
            model_version: "0.3.0".to_string(),
            resource_spec: ResourceSpec { min_ram_gb: 14.0, min_vram_gb: 8.0, min_gpu_count: 1, recommended_gpu: "RTX 3060".to_string(), cpu_cores: 4, disk_gb: 14.0 },
            config: ModelConfig { context_size: 8192, batch_size: 32, max_tokens: 4096, temperature: 0.7, repeat_penalty: 1.1, gpu_layers: 0, quantization: "q4_0".to_string(), rope_freq_base: 10000.0, rope_scale: 1.0 },
            backend: "ollama".to_string(),
            tags: vec!["llm".into(), "efficient".into(), "general".into()],
            created_by: "xergon".to_string(),
            created_at: Utc::now(),
            is_official: true,
            downloads: AtomicU64::new(0),
            rating_sum: AtomicU64::new(0),
            rating_count: AtomicU64::new(0),
        }).ok();

        // DeepSeek-Coder-7B
        reg.register(DeploymentTemplate {
            id: uuid::Uuid::new_v4().to_string(),
            name: "DeepSeek-Coder-7B".to_string(),
            description: "Specialized code generation model trained on 87 programming languages".to_string(),
            category: TemplateCategory::Code,
            model_id: "deepseek-coder-7b".to_string(),
            model_version: "1.0.0".to_string(),
            resource_spec: ResourceSpec { min_ram_gb: 16.0, min_vram_gb: 10.0, min_gpu_count: 1, recommended_gpu: "RTX 3070".to_string(), cpu_cores: 4, disk_gb: 16.0 },
            config: ModelConfig { context_size: 16384, batch_size: 16, max_tokens: 8192, temperature: 0.2, repeat_penalty: 1.0, gpu_layers: 0, quantization: "q4_0".to_string(), rope_freq_base: 10000.0, rope_scale: 1.0 },
            backend: "ollama".to_string(),
            tags: vec!["code".into(), "programming".into(), "autocompletion".into()],
            created_by: "xergon".to_string(),
            created_at: Utc::now(),
            is_official: true,
            downloads: AtomicU64::new(0),
            rating_sum: AtomicU64::new(0),
            rating_count: AtomicU64::new(0),
        }).ok();

        // Whisper-Large
        reg.register(DeploymentTemplate {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Whisper-Large v3".to_string(),
            description: "OpenAI's Whisper large model for high-quality speech-to-text transcription".to_string(),
            category: TemplateCategory::Audio,
            model_id: "whisper-large".to_string(),
            model_version: "3.0.0".to_string(),
            resource_spec: ResourceSpec { min_ram_gb: 16.0, min_vram_gb: 10.0, min_gpu_count: 1, recommended_gpu: "RTX 3080".to_string(), cpu_cores: 4, disk_gb: 20.0 },
            config: ModelConfig { context_size: 448, batch_size: 16, max_tokens: 448, temperature: 0.0, repeat_penalty: 1.0, gpu_layers: 0, quantization: "f16".to_string(), rope_freq_base: 10000.0, rope_scale: 1.0 },
            backend: "ollama".to_string(),
            tags: vec!["audio".into(), "stt".into(), "transcription".into()],
            created_by: "xergon".to_string(),
            created_at: Utc::now(),
            is_official: true,
            downloads: AtomicU64::new(0),
            rating_sum: AtomicU64::new(0),
            rating_count: AtomicU64::new(0),
        }).ok();

        // SDXL
        reg.register(DeploymentTemplate {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Stable Diffusion XL".to_string(),
            description: "High-resolution image generation with SDXL 1.0".to_string(),
            category: TemplateCategory::ImageGen,
            model_id: "sdxl".to_string(),
            model_version: "1.0.0".to_string(),
            resource_spec: ResourceSpec { min_ram_gb: 16.0, min_vram_gb: 12.0, min_gpu_count: 1, recommended_gpu: "RTX 3080".to_string(), cpu_cores: 4, disk_gb: 25.0 },
            config: ModelConfig { context_size: 1024, batch_size: 1, max_tokens: 77, temperature: 1.0, repeat_penalty: 1.0, gpu_layers: 0, quantization: "f16".to_string(), rope_freq_base: 10000.0, rope_scale: 1.0 },
            backend: "custom".to_string(),
            tags: vec!["image".into(), "generation".into(), "diffusion".into()],
            created_by: "xergon".to_string(),
            created_at: Utc::now(),
            is_official: true,
            downloads: AtomicU64::new(0),
            rating_sum: AtomicU64::new(0),
            rating_count: AtomicU64::new(0),
        }).ok();

        // BGE-Large
        reg.register(DeploymentTemplate {
            id: uuid::Uuid::new_v4().to_string(),
            name: "BGE-Large-Embeddings v1.5".to_string(),
            description: "Multilingual text embedding model for RAG and semantic search".to_string(),
            category: TemplateCategory::Embeddings,
            model_id: "bge-large".to_string(),
            model_version: "1.5.0".to_string(),
            resource_spec: ResourceSpec { min_ram_gb: 4.0, min_vram_gb: 2.0, min_gpu_count: 1, recommended_gpu: "any".to_string(), cpu_cores: 2, disk_gb: 5.0 },
            config: ModelConfig { context_size: 512, batch_size: 64, max_tokens: 512, temperature: 0.0, repeat_penalty: 1.0, gpu_layers: 0, quantization: "f16".to_string(), rope_freq_base: 10000.0, rope_scale: 1.0 },
            backend: "ollama".to_string(),
            tags: vec!["embeddings".into(), "rag".into(), "semantic-search".into()],
            created_by: "xergon".to_string(),
            created_at: Utc::now(),
            is_official: true,
            downloads: AtomicU64::new(0),
            rating_sum: AtomicU64::new(0),
            rating_count: AtomicU64::new(0),
        }).ok();

        reg
    }

    pub fn register(&mut self, template: DeploymentTemplate) -> Result<DeploymentTemplateSnapshot, String> {
        let id = template.id.clone();
        let cat = template.category.to_string();
        let snapshot = template.snapshot();
        self.templates.insert(id.clone(), Arc::new(template));

        if let Some(mut existing) = self.categories.get_mut(&cat) {
            if !existing.value().contains(&id) {
                existing.value_mut().push(id);
            }
        } else {
            self.categories.insert(cat, vec![id]);
        }

        Ok(snapshot)
    }

    pub fn get(&self, id: &str) -> Option<DeploymentTemplateSnapshot> {
        self.templates.get(id).map(|t| t.snapshot())
    }

    pub fn search(&self, filter: &TemplateFilter) -> Vec<DeploymentTemplateSnapshot> {
        let mut results: Vec<DeploymentTemplateSnapshot> = self
            .templates
            .iter()
            .map(|t| t.snapshot())
            .filter(|t| {
                if let Some(ref cat) = filter.category {
                    if &t.category != cat {
                        return false;
                    }
                }
                if let Some(ref backend) = filter.backend {
                    if &t.backend != backend {
                        return false;
                    }
                }
                if let Some(official) = filter.is_official {
                    if t.is_official != official {
                        return false;
                    }
                }
                if t.avg_rating < filter.min_rating {
                    return false;
                }
                if !filter.tags.is_empty() {
                    let has_tag = filter.tags.iter().any(|tag| {
                        t.tags.iter().any(|t_tag| t_tag.to_lowercase() == tag.to_lowercase())
                    });
                    if !has_tag {
                        return false;
                    }
                }
                if !filter.search.is_empty() {
                    let search_lower = filter.search.to_lowercase();
                    let matches = t.name.to_lowercase().contains(&search_lower)
                        || t.description.to_lowercase().contains(&search_lower)
                        || t.model_id.to_lowercase().contains(&search_lower);
                    if !matches {
                        return false;
                    }
                }
                true
            })
            .collect();

        results.sort_by(|a, b| {
            b.downloads.cmp(&a.downloads)
                .then_with(|| b.avg_rating.partial_cmp(&a.avg_rating).unwrap_or(std::cmp::Ordering::Equal))
        });
        results
    }

    pub fn update(&self, id: &str, updates: &std::collections::HashMap<String, serde_json::Value>) -> Result<DeploymentTemplateSnapshot, String> {
        let tmpl = self.templates.get(id).ok_or("Template not found")?;
        let mut clone_arc = Arc::try_unwrap(tmpl.clone()).unwrap_or_else(|arc| {
            let snap = arc.snapshot();
            DeploymentTemplate {
                id: snap.id,
                name: snap.name,
                description: snap.description,
                category: snap.category,
                model_id: snap.model_id,
                model_version: snap.model_version,
                resource_spec: snap.resource_spec,
                config: snap.config,
                backend: snap.backend,
                tags: snap.tags,
                created_by: snap.created_by,
                created_at: snap.created_at,
                is_official: snap.is_official,
                downloads: AtomicU64::new(snap.downloads),
                rating_sum: AtomicU64::new((snap.avg_rating * snap.rating_count as f64).round() as u64),
                rating_count: AtomicU64::new(snap.rating_count),
            }
        });

        if let Some(v) = updates.get("name") {
            if let Some(s) = v.as_str() { clone_arc.name = s.to_string(); }
        }
        if let Some(v) = updates.get("description") {
            if let Some(s) = v.as_str() { clone_arc.description = s.to_string(); }
        }
        if let Some(v) = updates.get("backend") {
            if let Some(s) = v.as_str() { clone_arc.backend = s.to_string(); }
        }
        if let Some(v) = updates.get("tags") {
            if let Some(arr) = v.as_array() {
                clone_arc.tags = arr.iter().filter_map(|t| t.as_str().map(String::from)).collect();
            }
        }

        let snapshot = clone_arc.snapshot();
        drop(tmpl);
        self.templates.insert(id.to_string(), Arc::new(clone_arc));
        Ok(snapshot)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        if self.templates.remove(id).is_none() {
            return Err("Template not found".to_string());
        }
        for mut cat in self.categories.iter_mut() {
            cat.value_mut().retain(|tid| tid != id);
        }
        Ok(())
    }

    pub fn increment_downloads(&self, id: &str) -> Result<u64, String> {
        let tmpl = self.templates.get(id).ok_or("Template not found")?;
        let new_count = tmpl.downloads.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(new_count)
    }

    pub fn rate(&self, id: &str, score: u32) -> Result<f64, String> {
        if score < 1 || score > 5 {
            return Err("Rating must be between 1 and 5".to_string());
        }
        let tmpl = self.templates.get(id).ok_or("Template not found")?;
        tmpl.rating_sum.fetch_add(score as u64, Ordering::Relaxed);
        tmpl.rating_count.fetch_add(1, Ordering::Relaxed);
        let count = tmpl.rating_count.load(Ordering::Relaxed);
        let sum = tmpl.rating_sum.load(Ordering::Relaxed);
        Ok(sum as f64 / count as f64)
    }

    pub fn list_categories(&self) -> Vec<String> {
        self.categories.iter().map(|c| c.key().clone()).collect()
    }

    pub fn get_by_category(&self, category: &str) -> Vec<DeploymentTemplateSnapshot> {
        let ids = match self.categories.get(category) {
            Some(c) => c.value().clone(),
            None => return vec![],
        };
        ids.iter()
            .filter_map(|id| self.templates.get(id).map(|t| t.snapshot()))
            .collect()
    }

    pub fn get_official(&self) -> Vec<DeploymentTemplateSnapshot> {
        self.templates
            .iter()
            .filter(|t| t.is_official)
            .map(|t| t.snapshot())
            .collect()
    }

    pub fn get_metrics(&self) -> TemplateMetrics {
        let total = self.templates.len() as u64;
        let total_downloads: u64 = self.templates.iter().map(|t| t.downloads()).sum();
        let rating_sum: u64 = self.templates.iter().map(|t| t.rating_sum.load(Ordering::Relaxed)).sum();
        let rating_count: u64 = self.templates.iter().map(|t| t.rating_count()).sum();
        let official: u64 = self.templates.iter().filter(|t| t.is_official).count() as u64;
        let categories = self.categories.len() as u32;
        let avg_rating = if rating_count > 0 { rating_sum as f64 / rating_count as f64 } else { 0.0 };

        TemplateMetrics {
            total_templates: total,
            total_categories: categories,
            total_downloads,
            avg_rating,
            official_templates: official,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_template(name: &str, cat: TemplateCategory) -> DeploymentTemplate {
        DeploymentTemplate {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: format!("{} template", name),
            category: cat,
            model_id: name.to_lowercase().replace(' ', "-"),
            model_version: "1.0.0".to_string(),
            resource_spec: ResourceSpec::default(),
            config: ModelConfig::default(),
            backend: "ollama".to_string(),
            tags: vec!["test".into()],
            created_by: "test-user".to_string(),
            created_at: Utc::now(),
            is_official: false,
            downloads: AtomicU64::new(0),
            rating_sum: AtomicU64::new(0),
            rating_count: AtomicU64::new(0),
        }
    }

    #[test]
    fn test_new_registry_has_defaults() {
        let reg = TemplateRegistry::new();
        let metrics = reg.get_metrics();
        assert_eq!(metrics.total_templates, 8);
        assert!(metrics.official_templates >= 8);
    }

    #[test]
    fn test_register_template() {
        let mut reg = TemplateRegistry::new();
        let initial = reg.get_metrics().total_templates;
        let tmpl = make_template("Custom Model", TemplateCategory::Custom);
        let result = reg.register(tmpl);
        assert!(result.is_ok());
        assert_eq!(reg.get_metrics().total_templates, initial + 1);
    }

    #[test]
    fn test_get_template() {
        let reg = TemplateRegistry::new();
        let first_id = reg.templates.iter().next().unwrap().id.clone();
        let result = reg.get(&first_id);
        assert!(result.is_some());
    }

    #[test]
    fn test_get_nonexistent() {
        let reg = TemplateRegistry::new();
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn test_search_by_category() {
        let reg = TemplateRegistry::new();
        let filter = TemplateFilter {
            category: Some(TemplateCategory::Llm),
            ..Default::default()
        };
        let results = reg.search(&filter);
        assert!(!results.is_empty());
        assert!(results.iter().all(|t| t.category == TemplateCategory::Llm));
    }

    #[test]
    fn test_search_by_backend() {
        let reg = TemplateRegistry::new();
        let filter = TemplateFilter {
            backend: Some("ollama".to_string()),
            ..Default::default()
        };
        let results = reg.search(&filter);
        assert!(!results.is_empty());
        assert!(results.iter().all(|t| t.backend == "ollama"));
    }

    #[test]
    fn test_search_by_official() {
        let reg = TemplateRegistry::new();
        let filter = TemplateFilter {
            is_official: Some(true),
            ..Default::default()
        };
        let results = reg.search(&filter);
        assert!(!results.is_empty());
        assert!(results.iter().all(|t| t.is_official));
    }

    #[test]
    fn test_search_by_min_rating() {
        let reg = TemplateRegistry::new();
        // Rate something high
        let first_id = reg.templates.iter().next().unwrap().id.clone();
        for _ in 0..5 {
            reg.rate(&first_id, 5).unwrap();
        }
        let filter = TemplateFilter {
            min_rating: 4.0,
            ..Default::default()
        };
        let results = reg.search(&filter);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_search_by_tags() {
        let reg = TemplateRegistry::new();
        let filter = TemplateFilter {
            tags: vec!["code".into()],
            ..Default::default()
        };
        let results = reg.search(&filter);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_search_by_text() {
        let reg = TemplateRegistry::new();
        let filter = TemplateFilter {
            search: "Qwen".to_string(),
            ..Default::default()
        };
        let results = reg.search(&filter);
        assert!(!results.is_empty());
        assert!(results.iter().any(|t| t.name.to_lowercase().contains("qwen")));
    }

    #[test]
    fn test_search_empty_results() {
        let reg = TemplateRegistry::new();
        let filter = TemplateFilter {
            search: "xyznonexistent123".to_string(),
            ..Default::default()
        };
        let results = reg.search(&filter);
        assert!(results.is_empty());
    }

    #[test]
    fn test_update_template() {
        let reg = TemplateRegistry::new();
        let first_id = reg.templates.iter().next().unwrap().id.clone();
        let mut updates = HashMap::new();
        updates.insert("name".to_string(), serde_json::json!("Updated Name"));
        let result = reg.update(&first_id, &updates);
        assert!(result.is_ok());
        let updated = reg.get(&first_id).unwrap();
        assert_eq!(updated.name, "Updated Name");
    }

    #[test]
    fn test_update_nonexistent() {
        let reg = TemplateRegistry::new();
        let updates = HashMap::new();
        assert!(reg.update("nonexistent", &updates).is_err());
    }

    #[test]
    fn test_remove_template() {
        let reg = TemplateRegistry::new();
        let first_id = reg.templates.iter().next().unwrap().id.clone();
        assert!(reg.remove(&first_id).is_ok());
        assert!(reg.get(&first_id).is_none());
    }

    #[test]
    fn test_remove_nonexistent() {
        let reg = TemplateRegistry::new();
        assert!(reg.remove("nonexistent").is_err());
    }

    #[test]
    fn test_increment_downloads() {
        let reg = TemplateRegistry::new();
        let first_id = reg.templates.iter().next().unwrap().id.clone();
        assert_eq!(reg.increment_downloads(&first_id).unwrap(), 1);
        assert_eq!(reg.increment_downloads(&first_id).unwrap(), 2);
        let tmpl = reg.get(&first_id).unwrap();
        assert_eq!(tmpl.downloads, 2);
    }

    #[test]
    fn test_rate_template() {
        let reg = TemplateRegistry::new();
        let first_id = reg.templates.iter().next().unwrap().id.clone();
        assert_eq!(reg.rate(&first_id, 4).unwrap(), 4.0);
        assert_eq!(reg.rate(&first_id, 5).unwrap(), 4.5);
    }

    #[test]
    fn test_rate_invalid() {
        let reg = TemplateRegistry::new();
        let first_id = reg.templates.iter().next().unwrap().id.clone();
        assert!(reg.rate(&first_id, 0).is_err());
        assert!(reg.rate(&first_id, 6).is_err());
    }

    #[test]
    fn test_list_categories() {
        let reg = TemplateRegistry::new();
        let cats = reg.list_categories();
        assert!(cats.len() >= 4);
        assert!(cats.contains(&"LLM".to_string()));
    }

    #[test]
    fn test_get_by_category() {
        let reg = TemplateRegistry::new();
        let llms = reg.get_by_category("LLM");
        assert!(llms.len() >= 3);
    }

    #[test]
    fn test_get_official() {
        let reg = TemplateRegistry::new();
        let official = reg.get_official();
        assert!(official.len() >= 8);
        assert!(official.iter().all(|t| t.is_official));
    }

    #[test]
    fn test_metrics() {
        let reg = TemplateRegistry::new();
        let metrics = reg.get_metrics();
        assert_eq!(metrics.total_templates, 8);
        assert!(metrics.total_downloads == 0);
    }

    #[test]
    fn test_template_category_display() {
        assert_eq!(TemplateCategory::Llm.to_string(), "LLM");
        assert_eq!(TemplateCategory::ImageGen.to_string(), "ImageGen");
        assert_eq!(TemplateCategory::Custom.to_string(), "Custom");
    }

    #[test]
    fn test_resource_spec_default() {
        let spec = ResourceSpec::default();
        assert_eq!(spec.min_gpu_count, 0);
        assert_eq!(spec.cpu_cores, 0);
        assert_eq!(spec.disk_gb, 0.0);
    }

    #[test]
    fn test_model_config_default() {
        let config = ModelConfig::default();
        assert_eq!(config.batch_size, 0);
        assert_eq!(config.max_tokens, 0);
        assert_eq!(config.quantization, "");
        assert_eq!(config.rope_freq_base, 0.0);
        assert_eq!(config.rope_scale, 0.0);
    }

    #[test]
    fn test_snapshot_preserves_data() {
        let tmpl = make_template("Snapshot Test", TemplateCategory::Multimodal);
        tmpl.downloads.store(42, Ordering::Relaxed);
        tmpl.rating_sum.store(15, Ordering::Relaxed);
        tmpl.rating_count.store(3, Ordering::Relaxed);
        let snap = tmpl.snapshot();
        assert_eq!(snap.downloads, 42);
        assert!((snap.avg_rating - 5.0).abs() < 0.01);
        assert_eq!(snap.rating_count, 3);
    }
}
