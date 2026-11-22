//! Structured drum critique and section assessment workflows.

use serde_json::{json, Value};

use super::{
    capture_request, classify_provider_failure, AiError, CapturedRequest, PromptCandidate,
    PromptInput, PromptTask, RationaleProvider, SectionFingerprint,
};
use crate::generation::assessment::assess;
use crate::model::Document;

/// One validated drum critique finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrumCritiqueFinding {
    /// One of the seven supported, style-neutral categories.
    pub category: String,
    /// Provider severity (`info`, `warning`, or `error`).
    pub severity: String,
    /// Zero-based section index.
    pub section: usize,
    /// Countable explanation.
    pub detail: String,
}

/// A validated structured critique of a drum track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrumCritique {
    /// Captured provider request.
    pub request: CapturedRequest,
    /// Sorted validated findings.
    pub findings: Vec<DrumCritiqueFinding>,
}

/// One score attached to a section assessment.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionAssessment {
    /// Zero-based section index.
    pub section_index: usize,
    /// Provider score between zero and one, inclusive.
    pub score: f64,
    /// Supported explanation.
    pub detail: String,
}

/// A validated section assessment response.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionAssessmentResponse {
    /// Captured provider request.
    pub request: CapturedRequest,
    /// Assessments sorted by section index.
    pub assessments: Vec<SectionAssessment>,
}

/// Builds deterministic section timeline JSON from a document.
pub fn build_section_assessment_payload(document: &Document) -> Value {
    json!({"schema_version": 1, "sections": document.master_bars.iter().enumerate()
        .filter_map(|(index, bar)| bar.section.as_ref().map(|label| json!({
            "index": index, "label": label, "bar": index,
        }))).collect::<Vec<_>>()})
}

/// Captures structured drum facts and section timeline for a provider.
pub fn capture_drum_critique_request(
    provider: impl Into<String>,
    label: impl Into<String>,
    document: &Document,
    drum_track: usize,
) -> Result<CapturedRequest, AiError> {
    let report = assess(document);
    let input = PromptInput {
        task: PromptTask::DrumCritique,
        candidates: vec![PromptCandidate {
            label: label.into(),
            assessment: report.clone(),
        }],
        section_fingerprints: Vec::new(),
    };
    let report_json = report.to_json();
    let assessment: Value = serde_json::from_str(&report_json)
        .map_err(|error| AiError::InvalidContext(error.to_string()))?;
    let context_json = serde_json::to_string(&json!({
        "schema_version": 1, "task": "drum_critique", "drum_track": drum_track,
        "candidate": assessment,
        "section_timeline": build_section_assessment_payload(document),
    }))
    .map_err(|error| AiError::InvalidContext(error.to_string()))?;
    let provider = provider.into();
    if provider.trim().is_empty() {
        return Err(AiError::InvalidContext("provider name is empty".into()));
    }
    let prompt = format!("Use only these structured drum facts. Return JSON with schema_version 1 and findings.\n{context_json}");
    Ok(CapturedRequest {
        provider,
        input,
        context_fingerprint: super::stable_fingerprint(context_json.as_bytes()),
        context_json,
        prompt,
        tab_text: None,
        #[cfg(feature = "ascii")]
        bar_count: None,
    })
}

/// Calls a provider and validates a drum critique response.
pub fn critique_drums(
    provider: &impl RationaleProvider,
    label: impl Into<String>,
    document: &Document,
    drum_track: usize,
) -> Result<DrumCritique, AiError> {
    let request = capture_drum_critique_request(provider.name(), label, document, drum_track)?;
    let response = provider
        .complete(&request)
        .map_err(|message| AiError::ProviderFailure {
            reason: classify_provider_failure(&message),
            message,
        })?;
    parse_drum_critique_response(&request, &response)
}

/// Parses and validates a seven-category drum critique.
pub fn parse_drum_critique_response(
    request: &CapturedRequest,
    response: &str,
) -> Result<DrumCritique, AiError> {
    let value: Value = serde_json::from_str(response)
        .map_err(|error| AiError::MalformedResponse(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| AiError::MalformedResponse("critique must be an object".into()))?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(AiError::MalformedResponse(
            "unsupported critique schema version".into(),
        ));
    }
    let allowed = [
        "riffLock",
        "fillPlacement",
        "variety",
        "energyMatch",
        "grooveConsistency",
        "dynamicsUse",
        "sectionVariation",
    ];
    let values = object
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| AiError::MalformedResponse("findings must be an array".into()))?;
    let mut findings = Vec::new();
    for item in values {
        let item = item
            .as_object()
            .ok_or_else(|| AiError::MalformedResponse("finding must be an object".into()))?;
        let category = item
            .get("category")
            .and_then(Value::as_str)
            .filter(|value| allowed.contains(value))
            .ok_or_else(|| AiError::UnsupportedFinding("unknown drum category".into()))?;
        let severity = item
            .get("severity")
            .and_then(Value::as_str)
            .filter(|value| ["info", "warning", "error"].contains(value))
            .ok_or_else(|| AiError::UnsupportedFinding("unknown severity".into()))?;
        let section = item
            .get("section")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| AiError::MalformedResponse("section must be an integer".into()))?;
        let detail = item
            .get("detail")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty() && value.chars().count() <= 1000)
            .ok_or_else(|| AiError::MalformedResponse("detail must be 1..=1000 chars".into()))?;
        findings.push(DrumCritiqueFinding {
            category: category.into(),
            severity: severity.into(),
            section,
            detail: detail.into(),
        });
    }
    findings.sort_by(|left, right| {
        (left.section, &left.category, &left.detail).cmp(&(
            right.section,
            &right.category,
            &right.detail,
        ))
    });
    Ok(DrumCritique {
        request: request.clone(),
        findings,
    })
}

/// Calls a provider with caller-supplied section fingerprints.
pub fn assess_sections_with_ai(
    provider: &impl RationaleProvider,
    sections: Vec<SectionFingerprint>,
) -> Result<SectionAssessmentResponse, AiError> {
    let input = PromptInput {
        task: PromptTask::SectionAssessment,
        candidates: Vec::new(),
        section_fingerprints: sections,
    };
    let request = capture_request(provider.name(), input)?;
    let response = provider
        .complete(&request)
        .map_err(|message| AiError::ProviderFailure {
            reason: classify_provider_failure(&message),
            message,
        })?;
    let value: Value = serde_json::from_str(&response)
        .map_err(|error| AiError::MalformedResponse(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| AiError::MalformedResponse("assessment must be an object".into()))?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(AiError::MalformedResponse(
            "unsupported assessment schema version".into(),
        ));
    }
    let values = object
        .get("assessments")
        .and_then(Value::as_array)
        .ok_or_else(|| AiError::MalformedResponse("assessments must be an array".into()))?;
    let mut assessments = Vec::new();
    for item in values {
        let item = item
            .as_object()
            .ok_or_else(|| AiError::MalformedResponse("assessment must be an object".into()))?;
        let index = item
            .get("section_index")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| AiError::MalformedResponse("section_index must be an integer".into()))?;
        if index >= request.input.section_fingerprints.len() {
            return Err(AiError::UnsupportedFinding(
                "section index outside request".into(),
            ));
        }
        let score = item
            .get("score")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
            .ok_or_else(|| {
                AiError::MalformedResponse("score must be between zero and one".into())
            })?;
        let detail = item
            .get("detail")
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| AiError::MalformedResponse("detail is required".into()))?;
        assessments.push(SectionAssessment {
            section_index: index,
            score,
            detail: detail.into(),
        });
    }
    assessments.sort_by_key(|assessment| assessment.section_index);
    Ok(SectionAssessmentResponse {
        request,
        assessments,
    })
}

#[cfg(test)]
#[path = "advanced_test.rs"]
mod tests;
