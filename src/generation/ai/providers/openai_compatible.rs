//! Adapter for OpenAI-compatible Chat Completions endpoints.

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::generation::ai::{CapturedRequest, RationaleProvider};

use super::{
    client, post_json, response_json, validate_api_key, validate_endpoint, validate_model,
    ProviderConfigError, ProviderOptions,
};

/// Synchronous adapter for an OpenAI-compatible Chat Completions endpoint.
///
/// The endpoint and model are supplied by the caller. A bearer token is
/// optional for local services and can be set with [`Self::with_api_key`].
pub struct OpenAiCompatibleProvider {
    client: Client,
    endpoint: String,
    model: String,
    api_key: Option<String>,
    options: ProviderOptions,
}

impl OpenAiCompatibleProvider {
    /// Creates an adapter for a complete Chat Completions URL and model name.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConfigError::EmptyEndpoint`] or [`ProviderConfigError::EmptyModel`] if
    /// `endpoint` or `model` is blank, and [`ProviderConfigError::ClientInitialization`] if the
    /// HTTP client cannot be built.
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderConfigError> {
        let endpoint = endpoint.into();
        let model = model.into();
        let options = ProviderOptions::default();
        validate_endpoint(&endpoint)?;
        validate_model(&model)?;
        Ok(Self {
            client: client(options)?,
            endpoint,
            model,
            api_key: None,
            options,
        })
    }

    /// Adds a bearer token for endpoints that require authentication.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConfigError::EmptyApiKey`] if `api_key` is blank.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Result<Self, ProviderConfigError> {
        let api_key = api_key.into();
        validate_api_key(&api_key)?;
        self.api_key = Some(api_key);
        Ok(self)
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

impl RationaleProvider for OpenAiCompatibleProvider {
    fn name(&self) -> &str {
        "openai-compatible"
    }

    fn complete(&self, request: &CapturedRequest) -> Result<String, String> {
        let body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": request.prompt() }],
            "response_format": { "type": "json_object" },
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
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "openai-compatible response is missing choices[0].message.content".to_string()
            })?;
        Ok(content.to_string())
    }
}
