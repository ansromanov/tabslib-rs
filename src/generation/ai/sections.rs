//! Versioned AI prompts for caller-defined section labels and boundaries.
//!
//! This module captures structured caller fingerprints, calls the shared provider
//! interface, and validates every returned label or boundary against the request.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::{
    capture_request, classify_provider_failure, require_keys, AiError, CapturedRequest,
    PromptInput, PromptTask, RationaleProvider,
};

/// Compact, caller-owned facts describing one song section.
#[derive(Debug, Clone)]
pub struct SectionFingerprint {
    /// Stable caller identity used to validate the response.
    pub id: String,
    /// Named numeric measurements; the library assigns no style meaning.
    pub features: BTreeMap<String, f64>,
}

impl PartialEq for SectionFingerprint {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.features.len() == other.features.len()
            && self.features.iter().all(|(key, value)| {
                other
                    .features
                    .get(key)
                    .is_some_and(|other_value| value.to_bits() == other_value.to_bits())
            })
    }
}

impl Eq for SectionFingerprint {}

/// One validated label attached to a requested section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionLabel {
    /// Zero-based section index in the captured request.
    pub section_index: usize,
    /// Nonempty label returned by the provider.
    pub label: String,
}

/// Validated response to a section-label prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionLabelsResponse {
    /// Exact versioned request used for this response.
    pub request: CapturedRequest,
    /// One label for every section, in request order.
    pub labels: Vec<SectionLabel>,
}

/// One provider-suggested merge of adjacent sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionMergeSuggestion {
    /// Index of the left section.
    pub left: usize,
    /// Index of the right section; it must equal `left + 1`.
    pub right: usize,
    /// Caller-facing explanation for the suggestion.
    pub reason: String,
}

/// Validated response containing merge and split suggestions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionBoundaryResponse {
    /// Exact versioned request used for this response.
    pub request: CapturedRequest,
    /// Adjacent section pairs suggested for merging.
    pub merges: Vec<SectionMergeSuggestion>,
    /// Zero-based indices after which a split is suggested.
    pub splits_after: Vec<usize>,
}

impl PromptInput {
    /// Builds a section-label prompt from caller-provided fingerprints.
    pub fn section_labels(sections: &[SectionFingerprint]) -> Self {
        Self {
            task: PromptTask::SectionLabels,
            candidates: Vec::new(),
            section_fingerprints: sections.to_vec(),
        }
    }

    /// Builds a section-boundary prompt from caller-provided fingerprints.
    pub fn section_boundaries(sections: &[SectionFingerprint]) -> Self {
        Self {
            task: PromptTask::SectionBoundaries,
            candidates: Vec::new(),
            section_fingerprints: sections.to_vec(),
        }
    }
}

/// Requests and validates labels for every supplied section.
///
/// # Errors
///
/// Returns any error from [`capture_request`], including [`AiError::InvalidContext`] if `sections`
/// is empty or has blank or duplicate ids or non-finite features, and [`AiError::ProviderFailure`]
/// if the provider's `complete` call fails. Returns any error from
/// [`parse_section_labels_response`] for an invalid response.
pub fn generate_section_labels(
    provider: &impl RationaleProvider,
    sections: &[SectionFingerprint],
) -> Result<SectionLabelsResponse, AiError> {
    let request = capture_request(provider.name(), PromptInput::section_labels(sections))?;
    let response = provider
        .complete(&request)
        .map_err(|message| AiError::ProviderFailure {
            reason: classify_provider_failure(&message),
            message,
        })?;
    parse_section_labels_response(&request, &response)
}

/// Requests and validates merge/split suggestions for supplied sections.
///
/// # Errors
///
/// Returns any error from [`capture_request`], including [`AiError::InvalidContext`] if `sections`
/// has fewer than two entries or has blank or duplicate ids or non-finite features, and
/// [`AiError::ProviderFailure`] if the provider's `complete` call fails. Returns any error from
/// [`parse_section_boundary_response`] for an invalid response.
pub fn generate_section_boundaries(
    provider: &impl RationaleProvider,
    sections: &[SectionFingerprint],
) -> Result<SectionBoundaryResponse, AiError> {
    let request = capture_request(provider.name(), PromptInput::section_boundaries(sections))?;
    let response = provider
        .complete(&request)
        .map_err(|message| AiError::ProviderFailure {
            reason: classify_provider_failure(&message),
            message,
        })?;
    parse_section_boundary_response(&request, &response)
}

/// Parses labels and verifies that each section index appears exactly once.
///
/// # Errors
///
/// Returns [`AiError::InvalidContext`] if `request` is not a section-label request, and
/// [`AiError::MalformedResponse`] if the response is not a JSON object with a `labels` array, if an
/// entry is malformed or has an out-of-range `section_index` or a label outside 1..=120 characters,
/// or if the array does not label each section exactly once.
pub fn parse_section_labels_response(
    request: &CapturedRequest,
    response: &str,
) -> Result<SectionLabelsResponse, AiError> {
    if request.input.task != PromptTask::SectionLabels {
        return Err(AiError::InvalidContext(
            "request is not a section-label task".into(),
        ));
    }
    let object = response_object(response)?;
    require_keys(&object, &["labels"])?;
    let values = object
        .get("labels")
        .and_then(Value::as_array)
        .filter(|labels| labels.len() == request.input.section_fingerprints.len())
        .ok_or_else(|| {
            AiError::MalformedResponse("labels must contain one entry per section".into())
        })?;
    let mut labels = Vec::with_capacity(values.len());
    let mut indices = BTreeSet::new();
    for value in values {
        let item = value
            .as_object()
            .ok_or_else(|| AiError::MalformedResponse("each label must be an object".into()))?;
        require_keys(item, &["section_index", "label"])?;
        let section_index = item
            .get("section_index")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
            .filter(|index| *index < request.input.section_fingerprints.len())
            .ok_or_else(|| {
                AiError::MalformedResponse("section_index is outside the requested sections".into())
            })?;
        let label = item
            .get("label")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty() && s.chars().count() <= 120)
            .ok_or_else(|| AiError::MalformedResponse("label must be 1..=120 characters".into()))?
            .to_string();
        if !indices.insert(section_index) {
            return Err(AiError::MalformedResponse(
                "each section must be labeled exactly once".into(),
            ));
        }
        labels.push(SectionLabel {
            section_index,
            label,
        });
    }
    labels.sort_by_key(|label| label.section_index);
    Ok(SectionLabelsResponse {
        request: request.clone(),
        labels,
    })
}

/// Parses typed merge and split suggestions and checks all indices against the request.
///
/// # Errors
///
/// Returns [`AiError::InvalidContext`] if `request` is not a section-boundary request, and
/// [`AiError::MalformedResponse`] if the response is not a JSON object with `merges` and
/// `splits_after` arrays, if an index is outside the requested sections, if a merge does not name
/// unique adjacent sections, or if a boundary is both merged and split.
pub fn parse_section_boundary_response(
    request: &CapturedRequest,
    response: &str,
) -> Result<SectionBoundaryResponse, AiError> {
    if request.input.task != PromptTask::SectionBoundaries {
        return Err(AiError::InvalidContext(
            "request is not a section-boundary task".into(),
        ));
    }
    let count = request.input.section_fingerprints.len();
    let object = response_object(response)?;
    require_keys(&object, &["merges", "splits_after"])?;
    let merge_values = object
        .get("merges")
        .and_then(Value::as_array)
        .ok_or_else(|| AiError::MalformedResponse("merges must be an array".into()))?;
    let split_values = object
        .get("splits_after")
        .and_then(Value::as_array)
        .ok_or_else(|| AiError::MalformedResponse("splits_after must be an array".into()))?;
    let mut merges = Vec::with_capacity(merge_values.len());
    let mut boundaries = BTreeSet::new();
    for value in merge_values {
        let item = value
            .as_object()
            .ok_or_else(|| AiError::MalformedResponse("each merge must be an object".into()))?;
        require_keys(item, &["left", "right", "reason"])?;
        let left = index_field(item, "left", count)?;
        let right = index_field(item, "right", count)?;
        if right != left + 1 || !boundaries.insert(left) {
            return Err(AiError::MalformedResponse(
                "merges must name unique adjacent sections".into(),
            ));
        }
        let reason = item
            .get("reason")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty() && s.chars().count() <= 500)
            .ok_or_else(|| {
                AiError::MalformedResponse("merge reason must be 1..=500 characters".into())
            })?
            .to_string();
        merges.push(SectionMergeSuggestion {
            left,
            right,
            reason,
        });
    }
    let mut splits_after = Vec::with_capacity(split_values.len());
    for value in split_values {
        let index = value
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .filter(|index| index + 1 < count)
            .ok_or_else(|| {
                AiError::MalformedResponse(
                    "split index must identify a boundary within the section count".into(),
                )
            })?;
        if !boundaries.insert(index) {
            return Err(AiError::MalformedResponse(
                "a boundary cannot be both merged and split".into(),
            ));
        }
        splits_after.push(index);
    }
    merges.sort_by_key(|merge| merge.left);
    splits_after.sort_unstable();
    Ok(SectionBoundaryResponse {
        request: request.clone(),
        merges,
        splits_after,
    })
}

fn response_object(response: &str) -> Result<Map<String, Value>, AiError> {
    let value: Value = serde_json::from_str(response)
        .map_err(|error| AiError::MalformedResponse(error.to_string()))?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| AiError::MalformedResponse("response must be an object".into()))
}

pub(super) fn section_prompt_version(task: PromptTask) -> &'static str {
    match task {
        PromptTask::SectionLabels => "tabslib.section-labels.v1",
        PromptTask::SectionBoundaries => "tabslib.section-boundaries.v1",
        PromptTask::Rationale
        | PromptTask::CandidateComparison
        | PromptTask::CandidateJudging
        | PromptTask::TabCritique
        | PromptTask::DrumCritique
        | PromptTask::SectionAssessment => "tabslib.ai.v1",
    }
}

fn index_field(
    object: &Map<String, Value>,
    key: &str,
    section_count: usize,
) -> Result<usize, AiError> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|index| *index < section_count)
        .ok_or_else(|| {
            AiError::MalformedResponse(format!("{key} is outside the requested sections"))
        })
}
