use crate::config::ProviderConfig;
use crate::types::ChatCompletionRequest;
use reqwest::Client;
use std::sync::Arc;

#[derive(Clone)]
pub struct Provider {
    pub config: ProviderConfig,
    pub client: Client,
}

impl Provider {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    pub async fn chat_completions(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/v1/chat/completions", self.config.base_url);

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );

        if let Some(ref api_key) = self.config.api_key {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", api_key).parse().unwrap(),
            );
        }

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(&request)
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }
}

pub type ProviderMap = std::sync::Arc<std::collections::HashMap<String, Provider>>;

pub fn build_providers(config: &[ProviderConfig]) -> ProviderMap {
    let mut providers = std::collections::HashMap::new();
    for provider_config in config {
        let provider = Provider::new(provider_config.clone());
        providers.insert(provider.config.id.clone(), provider);
    }
    Arc::new(providers)
}
