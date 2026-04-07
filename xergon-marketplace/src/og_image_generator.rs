use std::collections::HashMap;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// OgImageTemplate
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum OgImageTemplate {
    ModelCard,
    ProviderProfile,
    MarketplaceListing,
    TestResult,
    GovernanceProposal,
}

impl OgImageTemplate {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "model_card" => Some(Self::ModelCard),
            "provider_profile" => Some(Self::ProviderProfile),
            "marketplace_listing" => Some(Self::MarketplaceListing),
            "test_result" => Some(Self::TestResult),
            "governance_proposal" => Some(Self::GovernanceProposal),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::ModelCard => "model_card",
            Self::ProviderProfile => "provider_profile",
            Self::MarketplaceListing => "marketplace_listing",
            Self::TestResult => "test_result",
            Self::GovernanceProposal => "governance_proposal",
        }
    }
}

impl std::fmt::Display for OgImageTemplate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// OgImageConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OgImageConfig {
    pub width: u32,
    pub height: u32,
    pub background_color: String,
    pub font_size: u32,
    pub title: String,
    pub subtitle: String,
    pub provider_name: String,
    pub model_name: String,
    pub stats: HashMap<String, String>,
}

impl Default for OgImageConfig {
    fn default() -> Self {
        Self {
            width: 1200,
            height: 630,
            background_color: "#1a1a2e".to_string(),
            font_size: 48,
            title: String::new(),
            subtitle: String::new(),
            provider_name: String::new(),
            model_name: String::new(),
            stats: HashMap::new(),
        }
    }
}

impl OgImageConfig {
    pub fn new(title: &str, subtitle: &str) -> Self {
        Self {
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            ..Default::default()
        }
    }

    pub fn with_provider(mut self, provider: &str) -> Self {
        self.provider_name = provider.to_string();
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model_name = model.to_string();
        self
    }

    pub fn with_stat(mut self, key: &str, value: &str) -> Self {
        self.stats.insert(key.to_string(), value.to_string());
        self
    }
}

// ---------------------------------------------------------------------------
// OgMetadata
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OgMetadata {
    pub og_title: String,
    pub og_description: String,
    pub og_image_url: String,
    pub og_type: String,
    pub og_url: String,
    pub twitter_card: String,
    pub twitter_title: String,
    pub twitter_description: String,
    pub twitter_image: String,
}

// ---------------------------------------------------------------------------
// CachedImage
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CachedImage {
    pub key: String,
    pub page_type: String,
    pub id: String,
    pub svg_markup: String,
    pub metadata: OgMetadata,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u64,
}

// ---------------------------------------------------------------------------
// TemplateDefinition
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TemplateDefinition {
    pub template_type: OgImageTemplate,
    pub name: String,
    pub description: String,
    pub svg_pattern: String,
    pub variables: Vec<String>,
    pub default_config: OgImageConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// OgImageGenerator
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct OgImageGenerator {
    templates: DashMap<String, TemplateDefinition>,
    cache: DashMap<String, CachedImage>,
    base_url: String,
}

impl Default for OgImageGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl OgImageGenerator {
    pub fn new() -> Self {
        let gen = Self {
            templates: DashMap::new(),
            cache: DashMap::new(),
            base_url: "https://marketplace.xergon.network".to_string(),
        };
        gen.register_default_templates();
        gen
    }

    pub fn with_base_url(base_url: &str) -> Self {
        let gen = Self {
            templates: DashMap::new(),
            cache: DashMap::new(),
            base_url: base_url.to_string(),
        };
        gen.register_default_templates();
        gen
    }

    fn register_default_templates(&self) {
        let svg_model_card = concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{{width}}" height="{{height}}">"##,
            "\n  ",
            r##"<rect width="100%" height="100%" fill="{{background_color}}"/>"##,
            "\n  ",
            r##"<text x="60" y="120" fill="#e94560" font-size="24" font-family="Arial">{{provider_name}}</text>"##,
            "\n  ",
            r##"<text x="60" y="200" fill="white" font-size="{{font_size}}" font-family="Arial" font-weight="bold">{{title}}</text>"##,
            "\n  ",
            r##"<text x="60" y="280" fill="#a8a8b3" font-size="28" font-family="Arial">{{subtitle}}</text>"##,
            "\n  {{#stats}}\n  ",
            r##"<text x="60" y="{{y}}" fill="#a8a8b3" font-size="20" font-family="Arial">{{key}}: {{value}}</text>"##,
            "\n  {{/stats}}\n  ",
            r##"<text x="60" y="580" fill="#555" font-size="16" font-family="Arial">Xergon Network Marketplace</text>"##,
            "\n</svg>",
        );

        let model_card = TemplateDefinition {
            template_type: OgImageTemplate::ModelCard,
            name: "Model Card".to_string(),
            description: "OG image for model cards".to_string(),
            svg_pattern: svg_model_card.to_string(),
            variables: vec![
                "width".to_string(),
                "height".to_string(),
                "background_color".to_string(),
                "font_size".to_string(),
                "title".to_string(),
                "subtitle".to_string(),
                "provider_name".to_string(),
            ],
            default_config: OgImageConfig {
                background_color: "#1a1a2e".to_string(),
                font_size: 48,
                ..Default::default()
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.templates.insert("model_card".to_string(), model_card);

        let svg_provider = concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{{width}}" height="{{height}}">"##,
            "\n  ",
            r##"<rect width="100%" height="100%" fill="{{background_color}}"/>"##,
            "\n  ",
            r##"<text x="60" y="200" fill="white" font-size="{{font_size}}" font-family="Arial" font-weight="bold">{{title}}</text>"##,
            "\n  ",
            r##"<text x="60" y="280" fill="#a8a8b3" font-size="28" font-family="Arial">{{subtitle}}</text>"##,
            "\n  ",
            r##"<text x="60" y="580" fill="#555" font-size="16" font-family="Arial">Xergon Network - Provider</text>"##,
            "\n</svg>",
        );

        let provider_profile = TemplateDefinition {
            template_type: OgImageTemplate::ProviderProfile,
            name: "Provider Profile".to_string(),
            description: "OG image for provider profiles".to_string(),
            svg_pattern: svg_provider.to_string(),
            variables: vec![
                "width".to_string(),
                "height".to_string(),
                "background_color".to_string(),
                "font_size".to_string(),
                "title".to_string(),
                "subtitle".to_string(),
            ],
            default_config: OgImageConfig {
                background_color: "#16213e".to_string(),
                font_size: 48,
                ..Default::default()
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.templates.insert("provider_profile".to_string(), provider_profile);

        let svg_listing = concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{{width}}" height="{{height}}">"##,
            "\n  ",
            r##"<rect width="100%" height="100%" fill="{{background_color}}"/>"##,
            "\n  ",
            r##"<text x="60" y="200" fill="white" font-size="{{font_size}}" font-family="Arial" font-weight="bold">{{title}}</text>"##,
            "\n  ",
            r##"<text x="60" y="280" fill="#a8a8b3" font-size="28" font-family="Arial">{{subtitle}}</text>"##,
            "\n  ",
            r##"<text x="60" y="580" fill="#555" font-size="16" font-family="Arial">Xergon Network Marketplace</text>"##,
            "\n</svg>",
        );

        let marketplace_listing = TemplateDefinition {
            template_type: OgImageTemplate::MarketplaceListing,
            name: "Marketplace Listing".to_string(),
            description: "OG image for marketplace listings".to_string(),
            svg_pattern: svg_listing.to_string(),
            variables: vec![
                "width".to_string(),
                "height".to_string(),
                "background_color".to_string(),
                "font_size".to_string(),
                "title".to_string(),
                "subtitle".to_string(),
            ],
            default_config: OgImageConfig {
                background_color: "#0f3460".to_string(),
                font_size: 48,
                ..Default::default()
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.templates.insert("marketplace_listing".to_string(), marketplace_listing);

        let svg_test = concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{{width}}" height="{{height}}">"##,
            "\n  ",
            r##"<rect width="100%" height="100%" fill="{{background_color}}"/>"##,
            "\n  ",
            r##"<text x="60" y="200" fill="#4ecca3" font-size="{{font_size}}" font-family="Arial" font-weight="bold">{{title}}</text>"##,
            "\n  ",
            r##"<text x="60" y="280" fill="#a8a8b3" font-size="28" font-family="Arial">{{subtitle}}</text>"##,
            "\n  ",
            r##"<text x="60" y="580" fill="#555" font-size="16" font-family="Arial">Xergon Network - Test Results</text>"##,
            "\n</svg>",
        );

        let test_result = TemplateDefinition {
            template_type: OgImageTemplate::TestResult,
            name: "Test Result".to_string(),
            description: "OG image for test results".to_string(),
            svg_pattern: svg_test.to_string(),
            variables: vec![
                "width".to_string(),
                "height".to_string(),
                "background_color".to_string(),
                "font_size".to_string(),
                "title".to_string(),
                "subtitle".to_string(),
            ],
            default_config: OgImageConfig {
                background_color: "#1a1a2e".to_string(),
                font_size: 48,
                ..Default::default()
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.templates.insert("test_result".to_string(), test_result);

        let svg_governance = concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{{width}}" height="{{height}}">"##,
            "\n  ",
            r##"<rect width="100%" height="100%" fill="{{background_color}}"/>"##,
            "\n  ",
            r##"<text x="60" y="200" fill="#e94560" font-size="{{font_size}}" font-family="Arial" font-weight="bold">{{title}}</text>"##,
            "\n  ",
            r##"<text x="60" y="280" fill="#a8a8b3" font-size="28" font-family="Arial">{{subtitle}}</text>"##,
            "\n  ",
            r##"<text x="60" y="580" fill="#555" font-size="16" font-family="Arial">Xergon Network - Governance</text>"##,
            "\n</svg>",
        );

        let governance = TemplateDefinition {
            template_type: OgImageTemplate::GovernanceProposal,
            name: "Governance Proposal".to_string(),
            description: "OG image for governance proposals".to_string(),
            svg_pattern: svg_governance.to_string(),
            variables: vec![
                "width".to_string(),
                "height".to_string(),
                "background_color".to_string(),
                "font_size".to_string(),
                "title".to_string(),
                "subtitle".to_string(),
            ],
            default_config: OgImageConfig {
                background_color: "#533483".to_string(),
                font_size: 48,
                ..Default::default()
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.templates.insert("governance_proposal".to_string(), governance);
    }

    /// Generate OG metadata for a given page type and id.
    pub fn generate_metadata(&self, page_type: &str, id: &str, config: &OgImageConfig) -> OgMetadata {
        let image_url = self.get_image_url(page_type, id);
        let title = if config.title.is_empty() {
            format!("Xergon Marketplace - {}", page_type)
        } else {
            config.title.clone()
        };
        let description = if config.subtitle.is_empty() {
            format!("Explore {} on Xergon Network", page_type)
        } else {
            config.subtitle.clone()
        };
        let page_url = format!("{}/{}", self.base_url, page_type.replace("_", "-"));

        OgMetadata {
            og_title: title.clone(),
            og_description: description.clone(),
            og_image_url: image_url.clone(),
            og_type: "website".to_string(),
            og_url: page_url.clone(),
            twitter_card: "summary_large_image".to_string(),
            twitter_title: title,
            twitter_description: description,
            twitter_image: image_url,
        }
    }

    /// Get the image URL for a given page type and id.
    pub fn get_image_url(&self, page_type: &str, id: &str) -> String {
        let cache_key = self.cache_key(page_type, id);
        if self.cache.contains_key(&cache_key) {
            if let Some(mut entry) = self.cache.get_mut(&cache_key) {
                entry.last_accessed = Utc::now();
                entry.access_count += 1;
            }
        }
        format!(
            "{}/v1/og/image/{}/{}",
            self.base_url,
            page_type.replace("_", "-"),
            id
        )
    }

    /// Register a custom template.
    pub fn register_template(&self, template: TemplateDefinition) {
        let key = template.template_type.as_str().to_string();
        self.templates.insert(key, template);
    }

    /// Cache an image with the given key and config.
    pub fn cache_image(&self, page_type: &str, id: &str, config: &OgImageConfig) -> CachedImage {
        let cache_key = self.cache_key(page_type, id);
        let svg_markup = self.generate_svg(config);
        let metadata = self.generate_metadata(page_type, id, config);

        let cached = CachedImage {
            key: cache_key.clone(),
            page_type: page_type.to_string(),
            id: id.to_string(),
            svg_markup,
            metadata,
            created_at: Utc::now(),
            last_accessed: Utc::now(),
            access_count: 1,
        };

        self.cache.insert(cache_key, cached.clone());
        cached
    }

    /// Invalidate a cached image by key.
    pub fn invalidate_cache(&self, page_type: &str, id: &str) -> bool {
        let key = self.cache_key(page_type, id);
        self.cache.remove(&key).is_some()
    }

    /// List all cached images.
    pub fn list_cached(&self) -> Vec<CachedImage> {
        self.cache
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get a cached image.
    pub fn get_cached(&self, page_type: &str, id: &str) -> Option<CachedImage> {
        let key = self.cache_key(page_type, id);
        self.cache.get(&key).map(|entry| {
            let mut cached = entry.value().clone();
            cached.last_accessed = Utc::now();
            cached.access_count += 1;
            cached
        })
    }

    /// Get a template by type name.
    pub fn get_template(&self, template_type: &str) -> Option<TemplateDefinition> {
        self.templates.get(template_type).map(|e| e.value().clone())
    }

    /// List all registered templates.
    pub fn list_templates(&self) -> Vec<TemplateDefinition> {
        self.templates
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Generate SVG markup from a config.
    pub fn generate_svg(&self, config: &OgImageConfig) -> String {
        let stats_lines: String = config
            .stats
            .iter()
            .enumerate()
            .map(|(i, (k, v))| {
                let y = 360 + (i as u32) * 30;
                format!(
                    concat!(
                        r##"  <text x="60" y="{}" fill="#a8a8b3" font-size="20" font-family="Arial">"##,
                        "{}: {}",
                        "</text>",
                    ),
                    y, k, v
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            concat!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}">"##, "\n",
                r##"  <rect width="100%" height="100%" fill="{}"/>"##, "\n",
                r##"  <text x="60" y="120" fill="#e94560" font-size="24" font-family="Arial">{}</text>"##, "\n",
                r##"  <text x="60" y="200" fill="white" font-size="{}" font-family="Arial" font-weight="bold">{}</text>"##, "\n",
                r##"  <text x="60" y="280" fill="#a8a8b3" font-size="28" font-family="Arial">{}</text>"##, "\n",
                "{}", "\n",
                r##"  <text x="60" y="580" fill="#555" font-size="16" font-family="Arial">Xergon Network Marketplace</text>"##, "\n",
                "</svg>",
            ),
            config.width,
            config.height,
            config.background_color,
            config.provider_name,
            config.font_size,
            config.title,
            config.subtitle,
            stats_lines
        )
    }

    /// Interpolate template variables into the SVG pattern.
    pub fn interpolate_template(
        &self,
        template_type: &str,
        variables: &HashMap<String, String>,
    ) -> Result<String, String> {
        let template = self
            .templates
            .get(template_type)
            .ok_or_else(|| format!("Template '{}' not found", template_type))?;

        let mut result = template.svg_pattern.clone();
        for (key, value) in variables {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }

        // Remove any unreplaced {{#stats}} blocks
        result = result.replace("{{#stats}}", "").replace("{{/stats}}", "");

        Ok(result)
    }

    /// Get cache statistics.
    pub fn cache_stats(&self) -> CacheStats {
        let total = self.cache.len();
        let total_accesses: u64 = self.cache.iter().map(|e| e.value().access_count).sum();
        CacheStats {
            total_cached_images: total,
            total_accesses,
            total_templates: self.templates.len(),
        }
    }

    fn cache_key(&self, page_type: &str, id: &str) -> String {
        format!("{}:{}", page_type, id)
    }
}

// ---------------------------------------------------------------------------
// CacheStats
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CacheStats {
    pub total_cached_images: usize,
    pub total_accesses: u64,
    pub total_templates: usize,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_generator() -> OgImageGenerator {
        OgImageGenerator::new()
    }

    fn make_config() -> OgImageConfig {
        OgImageConfig::new("Test Model", "A great AI model")
            .with_provider("TestProvider")
            .with_model("test-model-v1")
            .with_stat("Downloads", "1,234")
            .with_stat("Rating", "4.8")
    }

    #[test]
    fn test_generate_metadata_basic() {
        let gen = make_generator();
        let config = make_config();
        let meta = gen.generate_metadata("model_card", "model-123", &config);

        assert_eq!(meta.og_title, "Test Model");
        assert_eq!(meta.og_description, "A great AI model");
        assert!(meta.og_image_url.contains("model-card"));
        assert!(meta.og_image_url.contains("model-123"));
        assert_eq!(meta.og_type, "website");
        assert_eq!(meta.twitter_card, "summary_large_image");
    }

    #[test]
    fn test_generate_metadata_defaults() {
        let gen = make_generator();
        let config = OgImageConfig::default();
        let meta = gen.generate_metadata("marketplace_listing", "list-456", &config);

        assert_eq!(meta.og_title, "Xergon Marketplace - marketplace_listing");
        assert!(meta.og_description.contains("marketplace_listing"));
    }

    #[test]
    fn test_template_interpolation() {
        let gen = make_generator();
        let mut vars = HashMap::new();
        vars.insert("width".to_string(), "1200".to_string());
        vars.insert("height".to_string(), "630".to_string());
        vars.insert("background_color".to_string(), "#1a1a2e".to_string());
        vars.insert("font_size".to_string(), "48".to_string());
        vars.insert("title".to_string(), "My Model".to_string());
        vars.insert("subtitle".to_string(), "Great model".to_string());
        vars.insert("provider_name".to_string(), "ProviderX".to_string());

        let svg = gen.interpolate_template("model_card", &vars).unwrap();
        assert!(svg.contains("My Model"));
        assert!(svg.contains("Great model"));
        assert!(svg.contains("ProviderX"));
        assert!(svg.contains("width=\"1200\""));
    }

    #[test]
    fn test_template_interpolation_missing_template() {
        let gen = make_generator();
        let vars = HashMap::new();
        let result = gen.interpolate_template("nonexistent", &vars);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_cache_hit() {
        let gen = make_generator();
        let config = make_config();
        gen.cache_image("model_card", "model-1", &config);

        let cached = gen.get_cached("model_card", "model-1");
        assert!(cached.is_some());
        let cached = cached.unwrap();
        assert_eq!(cached.page_type, "model_card");
        assert_eq!(cached.id, "model-1");
        assert!(cached.svg_markup.contains("Test Model"));
    }

    #[test]
    fn test_cache_miss() {
        let gen = make_generator();
        let cached = gen.get_cached("model_card", "nonexistent");
        assert!(cached.is_none());
    }

    #[test]
    fn test_cache_invalidation() {
        let gen = make_generator();
        let config = make_config();
        gen.cache_image("model_card", "model-1", &config);

        assert!(gen.get_cached("model_card", "model-1").is_some());
        assert!(gen.invalidate_cache("model_card", "model-1"));
        assert!(gen.get_cached("model_card", "model-1").is_none());
    }

    #[test]
    fn test_cache_list() {
        let gen = make_generator();
        let config = make_config();

        gen.cache_image("model_card", "m1", &config);
        gen.cache_image("provider_profile", "p1", &config);
        gen.cache_image("test_result", "t1", &config);

        let list = gen.list_cached();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_svg_generation() {
        let gen = make_generator();
        let config = make_config();
        let svg = gen.generate_svg(&config);

        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("width=\"1200\""));
        assert!(svg.contains("height=\"630\""));
        assert!(svg.contains("Test Model"));
        assert!(svg.contains("A great AI model"));
        assert!(svg.contains("TestProvider"));
        assert!(svg.contains("Downloads: 1,234"));
        assert!(svg.contains("Rating: 4.8"));
        assert!(svg.contains("Xergon Network Marketplace"));
    }

    #[test]
    fn test_get_image_url() {
        let gen = make_generator();
        let url = gen.get_image_url("model_card", "model-xyz");

        assert!(url.contains("model-card"));
        assert!(url.contains("model-xyz"));
        assert!(url.contains("v1/og/image"));
    }

    #[test]
    fn test_list_templates() {
        let gen = make_generator();
        let templates = gen.list_templates();

        assert!(templates.len() >= 5);
        let names: Vec<&str> = templates.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"Model Card"));
        assert!(names.contains(&"Provider Profile"));
    }

    #[test]
    fn test_cache_stats() {
        let gen = make_generator();
        let config = make_config();
        gen.cache_image("model_card", "m1", &config);
        gen.cache_image("model_card", "m2", &config);

        let stats = gen.cache_stats();
        assert_eq!(stats.total_cached_images, 2);
        assert!(stats.total_templates >= 5);
    }
}
