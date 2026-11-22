//! Connector for the Anthropic Messages API.

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::generation::ai::{CapturedRequest, RationaleProvider};

use super::{
    client, post_json, response_json, validate_api_key, validate_endpoint, validate_model,
    ProviderConfigError, ProviderOptions,
};

/// Model used when the caller does not name one.
pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-sonnet-5-5";

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2026-10-21";
const DEFAULT_MAX_TOKENS: u32 = 16_000;
const SYSTEM: &str = "Reply with one JSON object and nothing else: no prose, no code fence.";

/// Synchronous connector for the Anthropic Messages API.
///
/// The API key is supplied by the caller and sent in the `x-api-key` header.
/// It is never added to the prompt or request URL.
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
    max_tokens: u32,
    options: ProviderOptions,
}

impl AnthropicProvider {
    /// Creates a connector for the selected model and API key.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConfigError::EmptyApiKey`] or [`ProviderConfigError::EmptyModel`] if the
    /// key or model is blank, and [`ProviderConfigError::ClientInitialization`] if the HTTP client
    /// cannot be built.
    pub fn new(
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderConfigError> {
        let api_key = api_key.into();
        let model = model.into();
        let options = ProviderOptions::default();
        validate_api_key(&api_key)?;
        validate_model(&model)?;
        Ok(Self {
            client: client(options)?,
            api_key,
            model,
            endpoint: ENDPOINT.to_string(),
            max_tokens: DEFAULT_MAX_TOKENS,
            options,
        })
    }

    /// Uses a custom Messages endpoint, such as a compatible gateway.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConfigError::EmptyEndpoint`] if `endpoint` is blank.
    pub fn with_endpoint(
        mut self,
        endpoint: impl Into<String>,
    ) -> Result<Self, ProviderConfigError> {
        let endpoint = endpoint.into();
        validate_endpoint(&endpoint)?;
        self.endpoint = endpoint;
        Ok(self)
    }

    /// Caps the length of each response, in tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens.max(1);
        self
    }

    /// Uses a different request timeout and retry policy.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConfigError::InvalidTimeout`] if the timeout is zero,
    /// [`ProviderConfigError::TooManyRetries`] if `max_retries` exceeds five, and
    /// [`ProviderConfigError::ClientInitialization`] if the HTTP client cannot be built.
    pub fn with_options(mut self, options: ProviderOptions) -> Result<Self, ProviderConfigError> {
        self.client = client(options)?;
        self.options = options;
        Ok(self)
    }
}

impl RationaleProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn complete(&self, request: &CapturedRequest) -> Result<String, String> {
        let body = json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "system": SYSTEM,
            "messages": [{ "role": "user", "content": request.prompt() }],
        });
        let response = post_json(self.options, self.name(), || {
            self.client
                .post(&self.endpoint)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", API_VERSION)
                .json(&body)
        })?;
        let value = response_json(response, self.name())?;
        if value.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
            return Err("unknown: anthropic model declined the request".to_string());
        }
        let text: String = value
            .get("content")
            .and_then(Value::as_array)
            .ok_or_else(|| "anthropic response is missing content".to_string())?
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect();
        if text.is_empty() {
            return Err("anthropic response has no text content".to_string());
        }
        Ok(text)
    }
}
