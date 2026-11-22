//! Typed classification for provider failures surfaced by AI workflows.

use super::AiError;

/// Stable category for a failed provider request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiFailureReason {
    /// The provider endpoint could not be reached.
    Unreachable,
    /// The request timed out or was rate limited.
    Timeout,
    /// Authentication or authorization failed.
    Auth,
    /// The requested endpoint or model was not found.
    NotFound,
    /// The provider returned a server-side failure.
    ServerError,
    /// The failure could not be classified.
    Unknown,
}

impl AiError {
    /// Returns the typed reason when this is a provider failure.
    pub fn failure_reason(&self) -> Option<AiFailureReason> {
        match self {
            Self::ProviderFailure { reason, .. } => Some(*reason),
            _ => None,
        }
    }
}

/// Classifies sanitized adapter or mock-provider failure text.
pub fn classify_provider_failure(message: &str) -> AiFailureReason {
    let lower = message.to_ascii_lowercase();
    let lower = lower.trim();
    if lower.starts_with("unreachable:") || lower.contains("connection refused") {
        AiFailureReason::Unreachable
    } else if lower.starts_with("timeout:")
        || lower.contains("timed out")
        || lower.contains("http 408")
        || lower.contains("http 429")
    {
        AiFailureReason::Timeout
    } else if lower.starts_with("auth:") || lower.contains("http 401") || lower.contains("http 403")
    {
        AiFailureReason::Auth
    } else if lower.starts_with("not_found:") || lower.contains("http 404") {
        AiFailureReason::NotFound
    } else if lower.starts_with("server_error:") || lower.contains("http 5") {
        AiFailureReason::ServerError
    } else {
        AiFailureReason::Unknown
    }
}
