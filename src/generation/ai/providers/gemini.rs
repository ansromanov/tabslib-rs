//! Adapter for Google's Gemini `generateContent` API.

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::generation::ai::{CapturedRequest, RationaleProvider};

use super::{
    client, post_json, response_json, validate_api_key, validate_endpoint, validate_model,
    ProviderConfigError, ProviderOptions,
};

/// Synchronous adapter for Gemini's `generateContent` API.
///
/// The API key is supplied by the caller and sent in the `x-goog-api-key`
/// header. It is never added to the prompt or request URL.
pub struct GeminiProvider {
    client: Client,
    api_key: String,
    endpoint: String,
    options: ProviderOptions,
}

impl GeminiProvider {
    /// Creates an adapter for the selected Gemini model and API key.
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
            endpoint: format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
            ),
            options,
        })
    }

    /// Uses a custom `generateContent` endpoint for a compatible gateway.
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

impl RationaleProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn complete(&self, request: &CapturedRequest) -> Result<String, String> {
        let body = json!({
            "contents": [{ "parts": [{ "text": request.prompt() }] }],
            "generationConfig": { "responseMimeType": "application/json" },
        });
        let response = post_json(self.options, self.name(), || {
            self.client
                .post(&self.endpoint)
                .header("x-goog-api-key", &self.api_key)
                .json(&body)
        })?;
        let value = response_json(response, self.name())?;
        let parts = value
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|candidates| candidates.first())
            .and_then(|candidate| candidate.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
            .ok_or_else(|| "gemini response is missing candidates[0].content.parts".to_string())?;
        let mut text = String::new();
        for part in parts {
            if let Some(part_text) = part.get("text").and_then(Value::as_str) {
                text.push_str(part_text);
            }
        }
        if text.is_empty() {
            return Err("gemini response contains no text parts".into());
        }
        Ok(text)
    }
}
