//! Provider-independent cached decisions with deterministic fallbacks.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// A structured request whose canonical JSON identifies a decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionRequest {
    /// Namespace such as `section-labels` or `transition-plan`.
    pub namespace: String,
    /// All inputs that can affect the answer.
    pub input: Value,
}

/// Where a resolved answer came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionSource {
    /// The answer was served from the cache.
    Cache,
    /// The answer came from the provider.
    Provider,
    /// The answer came from the fallback after the provider failed.
    Fallback,
}

/// A decision and its provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionResult {
    /// Stable request key.
    pub key: String,
    /// Structured answer.
    pub value: Value,
    /// Resolution source.
    pub source: DecisionSource,
}

/// A caller-owned interactive decision provider.
pub struct InteractiveDecisionProvider<F> {
    callback: F,
}

impl<F> InteractiveDecisionProvider<F> {
    /// Creates a provider around a caller-supplied choice callback.
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

impl<F> InteractiveDecisionProvider<F>
where
    F: FnMut(&DecisionRequest) -> Result<Value, String>,
{
    /// Requests one choice from the callback.
    ///
    /// # Errors
    ///
    /// Returns the callback's error string unchanged if it cannot produce a choice.
    pub fn decide(&mut self, request: &DecisionRequest) -> Result<Value, String> {
        (self.callback)(request)
    }
}

/// Errors raised while reading or writing a decision store.
#[derive(Debug, Error)]
pub enum DecisionStoreError {
    /// Filesystem failure.
    #[error("decision cache I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// Invalid cached JSON.
    #[error("decision cache JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    /// Provider failure.
    #[error("decision provider failed: {0}")]
    Provider(String),
}

/// A storage backend for cached decisions.
pub trait DecisionStore {
    /// Reads a cached value by key.
    ///
    /// # Errors
    ///
    /// Returns [`DecisionStoreError`] if the backend cannot read the cached value; for
    /// [`FileDecisionStore`] that is an I/O failure other than a missing file, or invalid cached
    /// JSON.
    fn get(&mut self, key: &str) -> Result<Option<Value>, DecisionStoreError>;
    /// Stores a value by key.
    ///
    /// # Errors
    ///
    /// Returns [`DecisionStoreError`] if the backend cannot write the value; for
    /// [`FileDecisionStore`] that is a failure creating the directory or writing the file, or a
    /// JSON serialization failure.
    fn put(&mut self, key: &str, value: &Value) -> Result<(), DecisionStoreError>;
}

/// In-memory decision store for tests and short-lived runs.
#[derive(Debug, Default, Clone)]
pub struct MemoryDecisionStore(std::collections::BTreeMap<String, Value>);

impl DecisionStore for MemoryDecisionStore {
    fn get(&mut self, key: &str) -> Result<Option<Value>, DecisionStoreError> {
        Ok(self.0.get(key).cloned())
    }
    fn put(&mut self, key: &str, value: &Value) -> Result<(), DecisionStoreError> {
        self.0.insert(key.to_string(), value.clone());
        Ok(())
    }
}

/// A filesystem-backed JSON decision store.
#[derive(Debug, Clone)]
pub struct FileDecisionStore {
    directory: PathBuf,
}

impl FileDecisionStore {
    /// Creates a store rooted at `directory`.
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
    fn path(&self, key: &str) -> PathBuf {
        self.directory.join(format!("{key}.json"))
    }
}

impl DecisionStore for FileDecisionStore {
    fn get(&mut self, key: &str) -> Result<Option<Value>, DecisionStoreError> {
        let path = self.path(key);
        match fs::read_to_string(path) {
            Ok(contents) => Ok(Some(serde_json::from_str(&contents)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn put(&mut self, key: &str, value: &Value) -> Result<(), DecisionStoreError> {
        fs::create_dir_all(Path::new(&self.directory))?;
        fs::write(self.path(key), serde_json::to_string_pretty(value)?)?;
        Ok(())
    }
}

/// Produces a stable key from the full decision input.
pub fn decision_key(request: &DecisionRequest) -> String {
    let encoded = format!("{}:{}", request.namespace, request.input);
    crate::evaluation::content_hash(encoded.as_bytes())
}

/// Resolves a decision from cache, provider, or a deterministic fallback.
///
/// # Errors
///
/// Returns [`DecisionStoreError`] if reading the cache or storing the resolved value fails. A
/// provider error is not returned: it selects the fallback value instead.
pub fn resolve_decision<S, P, F>(
    store: &mut S,
    request: &DecisionRequest,
    provider: P,
    fallback: F,
) -> Result<DecisionResult, DecisionStoreError>
where
    S: DecisionStore,
    P: FnOnce(&DecisionRequest) -> Result<Value, String>,
    F: FnOnce(&DecisionRequest) -> Value,
{
    let key = decision_key(request);
    if let Some(value) = store.get(&key)? {
        return Ok(DecisionResult {
            key,
            value,
            source: DecisionSource::Cache,
        });
    }
    let (value, source) = match provider(request) {
        Ok(value) => (value, DecisionSource::Provider),
        Err(_) => (fallback(request), DecisionSource::Fallback),
    };
    store.put(&key, &value)?;
    Ok(DecisionResult { key, value, source })
}

/// Stable action choices for section decisions.
pub fn section_decisions_action_space(section_count: usize) -> Vec<Value> {
    (0..section_count)
        .map(|index| serde_json::json!({ "section": index, "action": "keep" }))
        .collect()
}

/// Stable action choices for boundaries between adjacent sections.
pub fn boundary_decisions_action_space(section_count: usize) -> Vec<Value> {
    (0..section_count.saturating_sub(1))
        .map(|index| serde_json::json!({ "boundary": index, "action": "keep" }))
        .collect()
}

/// Returns the allowed rationale action space for a decision request.
pub fn rationale_prose_action_space() -> Vec<Value> {
    vec![serde_json::json!({ "prose": "" })]
}

/// Resolves a section decision through the shared cache and fallback path.
///
/// # Errors
///
/// Returns any [`DecisionStoreError`] from [`resolve_decision`]: a failed cache read or write. A
/// provider error is not returned: it selects the fallback value instead.
pub fn resolve_section_decisions<S, P>(
    store: &mut S,
    input: Value,
    provider: P,
) -> Result<DecisionResult, DecisionStoreError>
where
    S: DecisionStore,
    P: FnOnce(&DecisionRequest) -> Result<Value, String>,
{
    resolve_decision(
        store,
        &DecisionRequest {
            namespace: "section-decisions".into(),
            input,
        },
        provider,
        |_| serde_json::json!({ "decisions": [] }),
    )
}

/// Resolves boundary decisions through the shared cache and fallback path.
///
/// # Errors
///
/// Returns any [`DecisionStoreError`] from [`resolve_decision`]: a failed cache read or write. A
/// provider error is not returned: it selects the fallback value instead.
pub fn resolve_boundary_decisions<S, P>(
    store: &mut S,
    input: Value,
    provider: P,
) -> Result<DecisionResult, DecisionStoreError>
where
    S: DecisionStore,
    P: FnOnce(&DecisionRequest) -> Result<Value, String>,
{
    resolve_decision(
        store,
        &DecisionRequest {
            namespace: "boundary-decisions".into(),
            input,
        },
        provider,
        |_| serde_json::json!({ "boundaries": [] }),
    )
}

/// Resolves rationale prose through the shared cache and fallback path.
///
/// # Errors
///
/// Returns any [`DecisionStoreError`] from [`resolve_decision`]: a failed cache read or write. A
/// provider error is not returned: it selects the fallback value instead.
pub fn resolve_rationale_prose<S, P>(
    store: &mut S,
    input: Value,
    provider: P,
) -> Result<DecisionResult, DecisionStoreError>
where
    S: DecisionStore,
    P: FnOnce(&DecisionRequest) -> Result<Value, String>,
{
    resolve_decision(
        store,
        &DecisionRequest {
            namespace: "rationale-prose".into(),
            input,
        },
        provider,
        |_| serde_json::json!({ "prose": "" }),
    )
}

#[cfg(test)]
#[path = "decision_test.rs"]
mod tests;
