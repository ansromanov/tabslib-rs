//! Arrangement plan sidecars, provenance hashing, and versioned serialization.
//!
//! A sidecar captures the arrangement plan together with its authoring history,
//! software provenance, and a SHA-256 fingerprint of the source score.
//! When a plan is reloaded against a modified score, the mismatch is detected
//! and rejected so stale edits do not corrupt the arrangement.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::arrangement::ArrangementPlan;

/// Current schema version for plan sidecars.
pub const PLAN_VERSION: u32 = 1;

/// A recorded edit in the plan's history sidecar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEditRecord {
    /// Action verb identifying the edit.
    pub verb: String,
    /// Human-readable slot address or target.
    pub target: String,
    /// Value or payload associated with the edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Stable timestamp for reproducible sidecars.
    pub timestamp: String,
    /// Optional natural-language intent that motivated this edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// Optional index of the decision in a batch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_index: Option<usize>,
}

/// A serialized plan sidecar containing metadata, provenance, and edit history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanSidecar {
    /// Sidecar schema version.
    pub version: u32,
    /// Application version that produced the sidecar.
    pub app_version: String,
    /// Optional source file path or identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Leading 16 hex characters of the source document's SHA-256 digest.
    pub source_hash: String,
    /// Timestamp when the sidecar was generated.
    pub created_at: String,
    /// Full chronological history of applied edits.
    #[serde(default)]
    pub edit_history: Vec<PlanEditRecord>,
    /// The arrangement plan itself.
    pub plan: ArrangementPlan,
}

/// Errors occurring during plan sidecar operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanSidecarError {
    /// The sidecar was written by a newer version than supported.
    #[error("unsupported plan version {version}; max supported is {max_supported}")]
    UnsupportedVersion {
        /// Version found in sidecar.
        version: u32,
        /// Maximum supported version.
        max_supported: u32,
    },
    /// The source score has changed since the plan was created.
    #[error("source hash mismatch: plan expected {expected}, current source is {actual}")]
    SourceHashMismatch {
        /// Hash recorded in the sidecar.
        expected: String,
        /// Current source hash.
        actual: String,
    },
    /// JSON serialization or deserialization failure.
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Computes the leading 16 hexadecimal characters of the SHA-256 digest of source bytes.
pub fn compute_source_hash(source_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_bytes);
    let digest = format!("{:x}", hasher.finalize());
    digest.chars().take(16).collect()
}

/// Packages an arrangement plan and its edit history into a sidecar structure.
pub fn plan_to_sidecar(
    plan: &ArrangementPlan,
    source_bytes: &[u8],
    app_version: &str,
    edit_history: Vec<PlanEditRecord>,
) -> PlanSidecar {
    PlanSidecar {
        version: PLAN_VERSION,
        app_version: app_version.to_string(),
        source_path: None,
        source_hash: compute_source_hash(source_bytes),
        created_at: "1970-01-01T00:00:00Z".to_string(),
        edit_history,
        plan: plan.clone(),
    }
}

/// Validates a sidecar against the current source score and extracts the plan.
///
/// If `current_source_bytes` is provided and the sidecar has a known source hash,
/// the source hash is verified. Stale plans whose source hash does not match
/// are rejected with [`PlanSidecarError::SourceHashMismatch`].
///
/// # Errors
///
/// Returns [`PlanSidecarError::UnsupportedVersion`] if the sidecar is newer than this build
/// supports, and [`PlanSidecarError::SourceHashMismatch`] if the source hash does not match
/// `current_source_bytes`.
pub fn plan_from_sidecar(
    sidecar: &PlanSidecar,
    current_source_bytes: Option<&[u8]>,
) -> Result<ArrangementPlan, PlanSidecarError> {
    if sidecar.version > PLAN_VERSION {
        return Err(PlanSidecarError::UnsupportedVersion {
            version: sidecar.version,
            max_supported: PLAN_VERSION,
        });
    }

    if let Some(bytes) = current_source_bytes {
        let current_hash = compute_source_hash(bytes);
        if sidecar.source_hash != "unknown" && sidecar.source_hash != current_hash {
            return Err(PlanSidecarError::SourceHashMismatch {
                expected: sidecar.source_hash.clone(),
                actual: current_hash,
            });
        }
    }

    Ok(sidecar.plan.clone())
}

/// Serializes an arrangement plan and history to a JSON sidecar string.
///
/// # Errors
///
/// Returns [`PlanSidecarError::Serialization`] if the sidecar cannot be encoded as JSON.
pub fn save_plan(
    plan: &ArrangementPlan,
    source_bytes: &[u8],
    app_version: &str,
    edit_history: Vec<PlanEditRecord>,
) -> Result<String, PlanSidecarError> {
    let sidecar = plan_to_sidecar(plan, source_bytes, app_version, edit_history);
    serde_json::to_string_pretty(&sidecar)
        .map_err(|e| PlanSidecarError::Serialization(e.to_string()))
}

/// Deserializes a plan sidecar JSON string and validates it against current source bytes.
///
/// # Errors
///
/// Returns [`PlanSidecarError::Serialization`] if `sidecar_json` is not a valid sidecar. Also
/// returns any error from [`plan_from_sidecar`] for an unsupported version or a stale source hash.
pub fn load_plan(
    sidecar_json: &str,
    current_source_bytes: Option<&[u8]>,
) -> Result<(ArrangementPlan, PlanSidecar), PlanSidecarError> {
    let sidecar: PlanSidecar = serde_json::from_str(sidecar_json)
        .map_err(|e| PlanSidecarError::Serialization(e.to_string()))?;
    let plan = plan_from_sidecar(&sidecar, current_source_bytes)?;
    Ok((plan, sidecar))
}

#[cfg(test)]
#[path = "plan_sidecar_test.rs"]
mod tests;
