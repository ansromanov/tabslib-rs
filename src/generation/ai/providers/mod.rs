//! Optional synchronous HTTP adapters for rationale services.

use std::io::Read;
use std::thread;
use std::time::Duration;

use reqwest::blocking::{Client, RequestBuilder, Response};
use serde_json::Value;
use thiserror::Error;

#[cfg(feature = "ai-anthropic")]
mod anthropic;
mod config;
#[cfg(feature = "ai-gemini")]
mod gemini;
#[cfg(feature = "ai-ollama")]
mod ollama;
#[cfg(feature = "ai-openai-compatible")]
mod openai_compatible;

#[cfg(feature = "ai-anthropic")]
pub use anthropic::{AnthropicProvider, ANTHROPIC_DEFAULT_MODEL};
pub use config::{AiConfig, AiConnector};
#[cfg(feature = "ai-gemini")]
pub use gemini::GeminiProvider;
#[cfg(feature = "ai-ollama")]
pub use ollama::OllamaProvider;
#[cfg(feature = "ai-openai-compatible")]
pub use openai_compatible::OpenAiCompatibleProvider;

const RESPONSE_LIMIT: u64 = 1_048_576;
const RESPONSE_LIMIT_USIZE: usize = 1_048_576;
const MAX_RETRIES: usize = 5;

/// Timeout and retry settings shared by concrete providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderOptions {
    /// Maximum time for each individual HTTP request.
    pub timeout: Duration,
    /// Number of retries for connection errors, timeouts, HTTP 408/429, and 5xx responses.
    pub max_retries: usize,
    /// Delay between retry attempts. Zero disables the delay, not retries.
    pub retry_delay: Duration,
}

impl ProviderOptions {
    /// Uses a different per-request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the maximum retry count, capped at five retries.
    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Sets the fixed delay between retry attempts.
    pub fn with_retry_delay(mut self, retry_delay: Duration) -> Self {
        self.retry_delay = retry_delay;
        self
    }
}

impl Default for ProviderOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
            max_retries: 0,
            retry_delay: Duration::from_millis(200),
        }
    }
}

/// Error returned when a concrete provider cannot be configured.
#[derive(Debug, Error)]
pub enum ProviderConfigError {
    /// The API key contains no non-whitespace characters.
    #[error("provider API key is empty")]
    EmptyApiKey,
    /// The model name contains no non-whitespace characters.
    #[error("provider model is empty")]
    EmptyModel,
    /// The endpoint contains no non-whitespace characters.
    #[error("provider endpoint is empty")]
    EmptyEndpoint,
    /// The request timeout must be positive.
    #[error("provider timeout must be greater than zero")]
    InvalidTimeout,
    /// The configured retry count exceeds the supported maximum.
    #[error("provider retry count cannot exceed 5")]
    TooManyRetries,
    /// The connector needs an API key and none was configured or found in the
    /// environment variable named here.
    #[error("provider API key is missing; set it on the config or in {0}")]
    MissingApiKey(&'static str),
    /// The connector needs an endpoint and none was configured.
    #[error("provider endpoint is missing")]
    MissingEndpoint,
    /// The HTTP client could not be initialized.
    #[error("provider HTTP client could not be initialized: {0}")]
    ClientInitialization(String),
}

/// Builds a reusable client with the selected finite request timeout.
fn client(options: ProviderOptions) -> Result<Client, ProviderConfigError> {
    validate_options(options)?;
    Client::builder()
        .timeout(options.timeout)
        .build()
        .map_err(|error| ProviderConfigError::ClientInitialization(error.without_url().to_string()))
}

fn validate_options(options: ProviderOptions) -> Result<(), ProviderConfigError> {
    if options.timeout.is_zero() {
        return Err(ProviderConfigError::InvalidTimeout);
    }
    if options.max_retries > MAX_RETRIES {
        return Err(ProviderConfigError::TooManyRetries);
    }
    Ok(())
}

fn validate_model(model: &str) -> Result<(), ProviderConfigError> {
    if model.trim().is_empty() {
        return Err(ProviderConfigError::EmptyModel);
    }
    Ok(())
}

fn validate_api_key(api_key: &str) -> Result<(), ProviderConfigError> {
    if api_key.trim().is_empty() {
        return Err(ProviderConfigError::EmptyApiKey);
    }
    Ok(())
}

fn validate_endpoint(endpoint: &str) -> Result<(), ProviderConfigError> {
    if endpoint.trim().is_empty() {
        return Err(ProviderConfigError::EmptyEndpoint);
    }
    Ok(())
}

/// Sends a JSON POST request and retries only transient connection or server failures.
fn post_json<F>(
    options: ProviderOptions,
    provider: &str,
    mut request: F,
) -> Result<Response, String>
where
    F: FnMut() -> RequestBuilder,
{
    for attempt in 0..=options.max_retries {
        match request().send() {
            Ok(response)
                if retryable_status(response.status()) && attempt < options.max_retries =>
            {
                drop(response);
                if !options.retry_delay.is_zero() {
                    thread::sleep(options.retry_delay);
                }
            }
            Ok(response) => return Ok(response),
            Err(error)
                if (error.is_timeout() || error.is_connect()) && attempt < options.max_retries =>
            {
                if !options.retry_delay.is_zero() {
                    thread::sleep(options.retry_delay);
                }
            }
            Err(error) => {
                let reason = if error.is_timeout() {
                    "timeout"
                } else if error.is_connect() {
                    "unreachable"
                } else {
                    "unknown"
                };
                return Err(format!(
                    "{reason}: {provider} request failed: {}",
                    error.without_url()
                ));
            }
        }
    }
    Err(format!("timeout: {provider} request failed after retries"))
}

fn retryable_status(status: reqwest::StatusCode) -> bool {
    status.as_u16() == 408 || status.as_u16() == 429 || status.is_server_error()
}

/// Reads a successful, bounded JSON response from a provider endpoint.
fn response_json(response: Response, provider: &str) -> Result<Value, String> {
    let status = response.status();
    if !status.is_success() {
        let code = status.as_u16();
        let reason = if code == 408 || code == 429 {
            "timeout"
        } else if code == 401 || code == 403 {
            "auth"
        } else if code == 404 {
            "not_found"
        } else if status.is_server_error() {
            "server_error"
        } else {
            "unknown"
        };
        return Err(format!("{reason}: {provider} API returned HTTP {code}"));
    }

    let mut bytes = Vec::new();
    response
        .take(RESPONSE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| format!("unknown: {provider} response body could not be read"))?;
    if bytes.len() > RESPONSE_LIMIT_USIZE {
        return Err(format!(
            "unknown: {provider} response exceeded the 1 MiB limit"
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| format!("unknown: {provider} response was not valid JSON"))
}
