//! Adapter for Ollama's native chat API.

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::generation::ai::{CapturedRequest, RationaleProvider};

use super::{
    client, post_json, response_json, validate_endpoint, validate_model, ProviderConfigError,
    ProviderOptions,
};

/// Synchronous adapter for Ollama's local `/api/chat` endpoint.
pub struct OllamaProvider {
    client: Client,
    endpoint: String,
    model: String,
    api_key: Option<String>,
    options: ProviderOptions,
}

impl OllamaProvider {
    /// Creates an adapter for the local Ollama service and selected model.
    pub fn new(model: impl Into<String>) -> Result<Self, ProviderConfigError> {
        let endpoint = "http://localhost:11434/api/chat".to_string();
        let model = model.into();
        let options = ProviderOptions::default();
        validate_model(&model)?;
        Ok(Self {
            client: client(options)?,
            endpoint,
            model,
            api_key: None,
            options,
        })
    }

    /// Adds an optional bearer token for hosted Ollama endpoints.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Result<Self, ProviderConfigError> {
        let api_key = api_key.into();
        super::validate_api_key(&api_key)?;
        self.api_key = Some(api_key);
        Ok(self)
    }

    /// Uses a custom Ollama-compatible chat endpoint.
    pub fn with_endpoint(
        mut self,
        endpoint: impl Into<String>,
    ) -> Result<Self, ProviderConfigError> {
        let endpoint = endpoint.into();
        validate_endpoint(&endpoint)?;
        self.endpoint = endpoint;
        Ok(self)
    }

    /// Uses a different request timeout and retry policy.
    pub fn with_options(mut self, options: ProviderOptions) -> Result<Self, ProviderConfigError> {
        self.client = client(options)?;
        self.options = options;
        Ok(self)
    }
}

impl RationaleProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn complete(&self, request: &CapturedRequest) -> Result<String, String> {
        let body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": request.prompt() }],
            "format": "json",
            "stream": false,
        });
        let response = post_json(self.options, self.name(), || {
            let builder = self.client.post(&self.endpoint);
            let builder = match &self.api_key {
                Some(api_key) => builder.bearer_auth(api_key),
                None => builder,
            };
            builder.json(&body)
        })?;
        let value = response_json(response, self.name())?;
        let content = value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| "ollama response is missing message.content".to_string())?;
        Ok(content.to_string())
    }
}
