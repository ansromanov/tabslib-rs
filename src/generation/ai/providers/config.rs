//! One configuration type that builds any enabled connector.

use crate::generation::ai::RationaleProvider;

use super::{ProviderConfigError, ProviderOptions};

/// The service an [`AiConfig`] connects to. Each variant exists only when its
/// feature is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AiConnector {
    /// A local or remote Ollama server (`ai-ollama`).
    #[cfg(feature = "ai-ollama")]
    Ollama,
    /// Any OpenAI-compatible Chat Completions endpoint (`ai-openai-compatible`).
    #[cfg(feature = "ai-openai-compatible")]
    OpenAiCompatible,
    /// Google Gemini (`ai-gemini`).
    #[cfg(feature = "ai-gemini")]
    Gemini,
    /// The Anthropic Messages API (`ai-anthropic`).
    #[cfg(feature = "ai-anthropic")]
    Anthropic,
}

impl AiConnector {
    /// Environment variable read for the API key when none is configured.
    pub fn api_key_env(self) -> Option<&'static str> {
        match self {
            #[cfg(feature = "ai-ollama")]
            Self::Ollama => None,
            #[cfg(feature = "ai-openai-compatible")]
            Self::OpenAiCompatible => Some("OPENAI_API_KEY"),
            #[cfg(feature = "ai-gemini")]
            Self::Gemini => Some("GEMINI_API_KEY"),
            #[cfg(feature = "ai-anthropic")]
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
        }
    }
}

/// Settings for a rationale connector, built into a provider with
/// [`AiConfig::build`].
///
/// ```no_run
/// # #[cfg(feature = "ai-anthropic")] {
/// use tabslib::generation::ai::{AiConfig, AiConnector, ProviderOptions};
///
/// let provider = AiConfig::new(AiConnector::Anthropic)
///     .with_options(ProviderOptions::default().with_max_retries(2))
///     .build()?;
/// # }
/// # Ok::<(), tabslib::generation::ai::ProviderConfigError>(())
/// ```
///
/// Unset fields fall back to the connector's defaults: its public endpoint,
/// its default model where it has one, and the API key from
/// [`AiConnector::api_key_env`].
#[derive(Clone)]
pub struct AiConfig {
    /// Service to connect to.
    pub connector: AiConnector,
    /// Model name; required for every connector except Anthropic.
    pub model: Option<String>,
    /// Endpoint URL; required for OpenAI-compatible services.
    pub endpoint: Option<String>,
    /// API key; read from the connector's environment variable when unset.
    pub api_key: Option<String>,
    /// Response length cap in tokens, where the service accepts one.
    pub max_tokens: Option<u32>,
    /// Timeout and retry policy.
    pub options: ProviderOptions,
}

impl std::fmt::Debug for AiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiConfig")
            .field("connector", &self.connector)
            .field("model", &self.model)
            .field("endpoint", &self.endpoint)
            .field("api_key", &self.api_key.as_ref().map(|_| "<set>"))
            .field("max_tokens", &self.max_tokens)
            .field("options", &self.options)
            .finish()
    }
}

impl AiConfig {
    /// Starts a configuration for one connector with every field at its default.
    pub fn new(connector: AiConnector) -> Self {
        Self {
            connector,
            model: None,
            endpoint: None,
            api_key: None,
            max_tokens: None,
            options: ProviderOptions::default(),
        }
    }

    /// Names the model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Uses a specific endpoint instead of the connector's default.
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Supplies the API key directly instead of reading the environment.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Caps the length of each response, where the service accepts a cap.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Uses a different timeout and retry policy.
    pub fn with_options(mut self, options: ProviderOptions) -> Self {
        self.options = options;
        self
    }

    fn resolved_api_key(&self) -> Option<String> {
        self.api_key.clone().or_else(|| {
            self.connector
                .api_key_env()
                .and_then(|name| std::env::var(name).ok())
                .filter(|key| !key.trim().is_empty())
        })
    }

    #[allow(dead_code)]
    fn required_api_key(&self) -> Result<String, ProviderConfigError> {
        self.resolved_api_key()
            .ok_or(ProviderConfigError::MissingApiKey(
                self.connector.api_key_env().unwrap_or("the config"),
            ))
    }

    #[allow(dead_code)]
    fn required_model(&self) -> Result<String, ProviderConfigError> {
        self.model.clone().ok_or(ProviderConfigError::EmptyModel)
    }

    /// Builds the configured connector.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderConfigError::MissingApiKey`] if a connector that needs a key has none
    /// configured or in its environment variable, [`ProviderConfigError::EmptyModel`] if a required
    /// model is missing or blank, and [`ProviderConfigError::MissingEndpoint`] if an
    /// OpenAI-compatible config has no endpoint. Also returns any error from the chosen provider's
    /// constructor and `with_*` methods, such as an empty endpoint or invalid [`ProviderOptions`].
    pub fn build(&self) -> Result<Box<dyn RationaleProvider>, ProviderConfigError> {
        match self.connector {
            #[cfg(feature = "ai-ollama")]
            AiConnector::Ollama => {
                let mut provider = super::OllamaProvider::new(self.required_model()?)?;
                if let Some(endpoint) = &self.endpoint {
                    provider = provider.with_endpoint(endpoint.clone())?;
                }
                if let Some(api_key) = self.resolved_api_key() {
                    provider = provider.with_api_key(api_key)?;
                }
                Ok(Box::new(provider.with_options(self.options)?))
            }
            #[cfg(feature = "ai-openai-compatible")]
            AiConnector::OpenAiCompatible => {
                let endpoint = self
                    .endpoint
                    .clone()
                    .ok_or(ProviderConfigError::MissingEndpoint)?;
                let mut provider =
                    super::OpenAiCompatibleProvider::new(endpoint, self.required_model()?)?;
                if let Some(api_key) = self.resolved_api_key() {
                    provider = provider.with_api_key(api_key)?;
                }
                Ok(Box::new(provider.with_options(self.options)?))
            }
            #[cfg(feature = "ai-gemini")]
            AiConnector::Gemini => {
                let mut provider =
                    super::GeminiProvider::new(self.required_api_key()?, self.required_model()?)?;
                if let Some(endpoint) = &self.endpoint {
                    provider = provider.with_endpoint(endpoint.clone())?;
                }
                Ok(Box::new(provider.with_options(self.options)?))
            }
            #[cfg(feature = "ai-anthropic")]
            AiConnector::Anthropic => {
                let model = self
                    .model
                    .clone()
                    .unwrap_or_else(|| super::ANTHROPIC_DEFAULT_MODEL.to_string());
                let mut provider = super::AnthropicProvider::new(self.required_api_key()?, model)?;
                if let Some(endpoint) = &self.endpoint {
                    provider = provider.with_endpoint(endpoint.clone())?;
                }
                if let Some(max_tokens) = self.max_tokens {
                    provider = provider.with_max_tokens(max_tokens);
                }
                Ok(Box::new(provider.with_options(self.options)?))
            }
        }
    }
}
