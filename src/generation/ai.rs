//! Optional provider-facing rationale and candidate-evaluation workflows.
//!
//! Prompt inputs contain only structured assessment facts. Provider calls are
//! made through [`RationaleProvider`], so core score APIs remain offline and
//! provider-independent.

use std::collections::BTreeSet;

use serde_json::{json, Value};

#[path = "ai/failure.rs"]
mod failure;
pub use failure::{classify_provider_failure, AiFailureReason};
#[path = "ai/hash.rs"]
mod hash;
use hash::{mix_seed, stable_fingerprint};

#[path = "ai/decision.rs"]
pub mod decision;
pub use decision::{
    boundary_decisions_action_space, decision_key, rationale_prose_action_space,
    resolve_boundary_decisions, resolve_decision, resolve_rationale_prose,
    resolve_section_decisions, section_decisions_action_space, DecisionRequest, DecisionResult,
    DecisionSource, DecisionStore, DecisionStoreError, FileDecisionStore,
    InteractiveDecisionProvider, MemoryDecisionStore,
};

#[path = "ai/advanced.rs"]
mod advanced;
pub use advanced::{
    assess_sections_with_ai, build_section_assessment_payload, capture_drum_critique_request,
    critique_drums, parse_drum_critique_response, DrumCritique, DrumCritiqueFinding,
    SectionAssessment, SectionAssessmentResponse,
};

#[path = "ai/comparison.rs"]
mod comparison;
#[path = "ai/judging.rs"]
mod judging;
pub use comparison::{
    compare_candidates, CandidateEvaluation, DiagnosticComparison, MetricComparison,
    MetricDirection,
};
use judging::{parse_finding, require_keys, validate_finding};
#[path = "ai/sections.rs"]
mod sections;
pub use judging::{
    blend_rankings_with_deterministic, judge_candidates, judge_preference_agreement,
    offline_heuristic_judge, parse_candidate_ranking, verify_bar_claims_in_tab, BarClaim,
    CandidateRanking, JudgeAgreement, TabCritique, TabCritiqueFinding, TabDefectClass,
    VerifiedBarClaim,
};
#[cfg(feature = "ascii")]
pub use judging::{
    capture_tab_critique_request, critique_tab, parse_tab_critique_response, verify_bar_claims,
};
pub use sections::{
    generate_section_boundaries, generate_section_labels, parse_section_boundary_response,
    parse_section_labels_response, SectionBoundaryResponse, SectionFingerprint, SectionLabel,
    SectionLabelsResponse, SectionMergeSuggestion,
};
use thiserror::Error;

use super::assessment::{AssessmentReport, Region};
#[cfg(feature = "ai-http")]
#[path = "ai/providers/mod.rs"]
mod providers;

#[cfg(feature = "ai-gemini")]
pub use providers::GeminiProvider;
#[cfg(feature = "ai-ollama")]
pub use providers::OllamaProvider;
#[cfg(feature = "ai-openai-compatible")]
pub use providers::OpenAiCompatibleProvider;
#[cfg(feature = "ai-http")]
pub use providers::{AiConfig, AiConnector, ProviderConfigError, ProviderOptions};
#[cfg(feature = "ai-anthropic")]
pub use providers::{AnthropicProvider, ANTHROPIC_DEFAULT_MODEL};

/// Task requested from a rationale provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTask {
    /// Explain facts from one assessment report.
    Rationale,
    /// Compare facts from two assessment reports.
    CandidateComparison,
    /// Rank two or more candidates using their assessment facts.
    CandidateJudging,
    /// Identify countable defects in a rendered ASCII tablature excerpt.
    TabCritique,
    /// Assign caller-defined labels to ordered section fingerprints.
    SectionLabels,
    /// Suggest adjacent section merges and split points.
    SectionBoundaries,
    /// Evaluate a drum part against structured score facts.
    DrumCritique,
    /// Assess section-level score facts without score text.
    SectionAssessment,
}

impl PromptTask {
    fn as_str(self) -> &'static str {
        match self {
            Self::Rationale => "rationale",
            Self::CandidateComparison => "candidate_comparison",
            Self::CandidateJudging => "candidate_judging",
            Self::TabCritique => "tab_critique",
            Self::SectionLabels => "section_labels",
            Self::SectionBoundaries => "section_boundaries",
            Self::DrumCritique => "drum_critique",
            Self::SectionAssessment => "section_assessment",
        }
    }
}

/// One named assessment included in a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptCandidate {
    /// Caller-supplied candidate label.
    pub label: String,
    /// Deterministic score facts for the candidate.
    pub assessment: AssessmentReport,
}

/// Structured and provider-neutral facts supplied to a rationale workflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptInput {
    /// Requested workflow.
    pub task: PromptTask,
    /// Assessments included in the request, in caller order.
    pub candidates: Vec<PromptCandidate>,
    /// Caller-supplied section fingerprints for section prompt tasks.
    pub section_fingerprints: Vec<SectionFingerprint>,
}

/// Provider-ready anonymous pair plus its caller-only identity mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlindPair {
    /// Comparison input labeled only as `Candidate A` and `Candidate B`.
    pub input: PromptInput,
    /// Original identity shown to the provider as `Candidate A`.
    pub candidate_a_identity: String,
    /// Original identity shown to the provider as `Candidate B`.
    pub candidate_b_identity: String,
}

impl PromptInput {
    /// Builds a rationale prompt from one report without including score text.
    pub fn rationale(label: impl Into<String>, report: &AssessmentReport) -> Self {
        Self {
            task: PromptTask::Rationale,
            candidates: vec![PromptCandidate {
                label: label.into(),
                assessment: report.clone(),
            }],
            section_fingerprints: Vec::new(),
        }
    }

    /// Builds a comparison prompt from two named assessment reports.
    pub fn comparison(
        left_label: impl Into<String>,
        left: &AssessmentReport,
        right_label: impl Into<String>,
        right: &AssessmentReport,
    ) -> Self {
        Self {
            task: PromptTask::CandidateComparison,
            candidates: vec![
                PromptCandidate {
                    label: left_label.into(),
                    assessment: left.clone(),
                },
                PromptCandidate {
                    label: right_label.into(),
                    assessment: right.clone(),
                },
            ],
            section_fingerprints: Vec::new(),
        }
    }

    /// Builds a ranking prompt from two or more caller-named assessment reports.
    pub fn judging(candidates: Vec<PromptCandidate>) -> Self {
        Self {
            task: PromptTask::CandidateJudging,
            candidates,
            section_fingerprints: Vec::new(),
        }
    }

    /// Serializes the structured context with stable field and metric order.
    ///
    /// # Errors
    ///
    /// Returns [`AiError::InvalidContext`] if the prompt context is invalid for its task: wrong
    /// candidate or section count, blank or duplicate labels or ids, or non-finite section
    /// features.
    pub fn to_json(&self) -> Result<String, AiError> {
        validate_prompt_input(self)?;
        let mut candidates = Vec::with_capacity(self.candidates.len());
        for candidate in &self.candidates {
            let assessment: Value = serde_json::from_str(&candidate.assessment.to_json())
                .map_err(|error| AiError::InvalidContext(error.to_string()))?;
            candidates.push(json!({
                "label": candidate.label,
                "assessment": assessment,
            }));
        }
        let value = match self.task {
            PromptTask::Rationale
            | PromptTask::CandidateComparison
            | PromptTask::CandidateJudging
            | PromptTask::TabCritique
            | PromptTask::DrumCritique => json!({
                "schema_version": 1,
                "task": self.task.as_str(),
                "candidates": candidates,
            }),
            PromptTask::SectionLabels
            | PromptTask::SectionBoundaries
            | PromptTask::SectionAssessment => json!({
                "schema_version": 1,
                "task": self.task.as_str(),
                "prompt_version": sections::section_prompt_version(self.task),
                "sections": self.section_fingerprints.iter().map(|section| json!({
                    "id": section.id,
                    "features": section.features,
                })).collect::<Vec<_>>(),
            }),
        };
        serde_json::to_string(&value).map_err(|error| AiError::InvalidContext(error.to_string()))
    }
}

/// Produces a stable anonymous candidate order from caller-supplied identities.
///
/// The mapping stays separate from [`BlindPair::input`], so passing only the
/// input to a provider does not reveal the original candidate labels.
///
/// # Errors
///
/// Returns [`AiError::InvalidContext`] if either label is blank or both labels are equal.
pub fn blind_pair(
    left_label: impl Into<String>,
    left: &AssessmentReport,
    right_label: impl Into<String>,
    right: &AssessmentReport,
    seed: u64,
) -> Result<BlindPair, AiError> {
    let left_label = left_label.into();
    let right_label = right_label.into();
    if left_label.trim().is_empty() || right_label.trim().is_empty() || left_label == right_label {
        return Err(AiError::InvalidContext(
            "blind candidate identities must be nonempty and distinct".into(),
        ));
    }
    let flip = mix_seed(seed) & 1 == 1;
    let (candidate_a_identity, candidate_a, candidate_b_identity, candidate_b) = if flip {
        (right_label, right, left_label, left)
    } else {
        (left_label, left, right_label, right)
    };
    Ok(BlindPair {
        input: PromptInput::comparison("Candidate A", candidate_a, "Candidate B", candidate_b),
        candidate_a_identity,
        candidate_b_identity,
    })
}

/// Immutable request context captured before a provider is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedRequest {
    /// Provider identifier supplied by the caller.
    provider: String,
    /// Structured facts behind the request.
    input: PromptInput,
    /// Canonical JSON context, suitable for persistence and replay.
    context_json: String,
    /// Stable 64-bit fingerprint of `context_json`.
    context_fingerprint: String,
    /// Complete prompt sent to the provider.
    prompt: String,
    /// Rendered ASCII tab for a critique request, when present.
    tab_text: Option<String>,
    /// Number of zero-based bars available to a tab critique.
    #[cfg(feature = "ascii")]
    bar_count: Option<usize>,
}

impl CapturedRequest {
    /// Provider identifier recorded for this request.
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Structured facts included in this request.
    pub fn input(&self) -> &PromptInput {
        &self.input
    }

    /// Canonical JSON context suitable for persistence and replay.
    pub fn context_json(&self) -> &str {
        &self.context_json
    }

    /// Stable fingerprint of the canonical context JSON.
    pub fn context_fingerprint(&self) -> &str {
        &self.context_fingerprint
    }

    /// Complete prompt sent to the provider.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// Rendered ASCII tab included in a critique request, when present.
    pub fn tab_text(&self) -> Option<&str> {
        self.tab_text.as_deref()
    }
}

/// Structured finding returned by a provider and checked against the context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RationaleFinding {
    /// Candidate label from the request.
    pub candidate: String,
    /// Assessment diagnostic code supporting the finding.
    pub code: String,
    /// Score region supporting the finding.
    pub region: Region,
    /// Provider explanation of the supported finding.
    pub detail: String,
}

/// Validated provider rationale linked to its captured context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rationale {
    /// Exact request and context supplied to the provider.
    pub request: CapturedRequest,
    /// Provider rationale text.
    pub text: String,
    /// Supported findings sorted into stable order.
    pub findings: Vec<RationaleFinding>,
}

/// Error returned by prompt, provider, parsing, or validation operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AiError {
    /// Prompt context is empty or inconsistent.
    #[error("invalid AI prompt context: {0}")]
    InvalidContext(String),
    /// The provider returned an error.
    #[error("provider failed ({reason:?}): {message}")]
    ProviderFailure {
        /// Classified cause of the provider failure.
        reason: AiFailureReason,
        /// Sanitized provider error text.
        message: String,
    },
    /// The provider response is not valid JSON with the expected schema.
    #[error("malformed provider response: {0}")]
    MalformedResponse(String),
    /// A provider output field does not agree with the captured context.
    #[error("unsupported provider finding: {0}")]
    UnsupportedFinding(String),
    /// A ranking response does not contain every candidate exactly once.
    #[error("invalid candidate ranking: {0}")]
    InvalidRanking(String),
    /// A requested ranking weight is outside its valid range.
    #[error("model weight must be finite and between zero and one")]
    InvalidModelWeight,
}

/// Provider interface for rationale text; implementations may call a service.
///
/// The library supplies no network implementation. Tests and offline callers
/// can implement this trait without credentials or external dependencies.
pub trait RationaleProvider {
    /// Stable provider identifier recorded with the result.
    fn name(&self) -> &str;

    /// Returns a JSON response conforming to [`parse_response`]'s schema.
    ///
    /// # Errors
    ///
    /// Returns the provider's error message as a `String` if the request cannot be completed; the
    /// text should start with a category prefix that [`classify_provider_failure`] recognizes.
    fn complete(&self, request: &CapturedRequest) -> Result<String, String>;
}

/// Lets a boxed provider, such as the one [`AiConfig::build`] returns, be passed
/// wherever a provider is expected.
#[cfg(feature = "ai-http")]
impl<P: RationaleProvider + ?Sized> RationaleProvider for Box<P> {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn complete(&self, request: &CapturedRequest) -> Result<String, String> {
        (**self).complete(request)
    }
}

/// Captures canonical prompt context before any provider call.
///
/// # Errors
///
/// Returns [`AiError::InvalidContext`] if the task is a tab critique, the provider name is blank,
/// or the prompt context is invalid for its task.
pub fn capture_request(
    provider: impl Into<String>,
    input: PromptInput,
) -> Result<CapturedRequest, AiError> {
    if input.task == PromptTask::TabCritique {
        return Err(AiError::InvalidContext(
            "tab critique requests must include ASCII output".into(),
        ));
    }
    let provider = provider.into();
    if provider.trim().is_empty() {
        return Err(AiError::InvalidContext("provider name is empty".into()));
    }
    let context_json = input.to_json()?;
    let context_fingerprint = stable_fingerprint(context_json.as_bytes());
    let instruction = match input.task {
        PromptTask::CandidateJudging => "Use only the assessment facts below. Return one JSON object with exactly schema_version (integer 1) and ranking (array of candidate labels), ordered best to worst. Include every candidate exactly once and do not add labels.",
        PromptTask::TabCritique => "Use only the rendered ASCII tab and its assessment facts. Return one JSON object with exactly findings (array). Each finding must have class, severity, bar (zero-based integer), and detail. Return no praise or general prose; use an empty findings array when no supported defect is visible.",
        PromptTask::SectionLabels => "",
        PromptTask::SectionBoundaries => "",
        PromptTask::DrumCritique | PromptTask::SectionAssessment => "",
        PromptTask::Rationale | PromptTask::CandidateComparison => "Use only the structured facts below. Do not infer unlisted score facts. Return one JSON object with exactly these fields: rationale (nonempty string) and findings (array). Each finding must have exactly candidate (string), code (string), bar (integer), track (integer or null), voice (integer or null), beat (integer or null), and detail (nonempty string). Cite only candidates, diagnostic codes, and exact regions present in the context. Use an empty findings array when none are supported.",
    };
    let prompt = match input.task {
        PromptTask::SectionLabels => format!(
            "Prompt version: {}. Use only the supplied section fingerprints. Return JSON with exactly one labels array entry per section; each entry has section_index (zero-based) and label (1-120 characters). Do not invent section indices.\n{}",
            sections::section_prompt_version(input.task), context_json
        ),
        PromptTask::SectionBoundaries => format!(
            "Prompt version: {}. Use only the supplied ordered section fingerprints. Return JSON with exactly merges and splits_after. Each merge has adjacent left/right zero-based indices and a concise reason; each split_after is a boundary index. Do not return indices outside the supplied sections or suggest the same boundary as both merge and split.\n{}",
            sections::section_prompt_version(input.task), context_json
        ),
        PromptTask::DrumCritique => format!(
            "Use only the structured drum facts below. Return JSON with exactly schema_version 1 and findings. Each finding has category, severity, section, and detail. Use an empty findings array when unsupported.\n{context_json}"
        ),
        PromptTask::SectionAssessment => format!(
            "Use only the supplied section facts. Return JSON with exactly schema_version 1 and assessments. Each assessment has section_index, score (0..=1), and detail. Do not invent indices.\n{context_json}"
        ),
        _ => format!("{instruction}\n{context_json}"),
    };
    Ok(CapturedRequest {
        provider,
        input,
        context_json,
        context_fingerprint,
        prompt,
        tab_text: None,
        #[cfg(feature = "ascii")]
        bar_count: None,
    })
}

/// Calls a provider, validates its response, and captures the rationale context.
///
/// # Errors
///
/// Returns any error from [`capture_request`], [`AiError::ProviderFailure`] if the provider's
/// `complete` call fails, and any error from [`parse_response`].
pub fn generate_rationale(
    provider: &impl RationaleProvider,
    input: PromptInput,
) -> Result<Rationale, AiError> {
    let request = capture_request(provider.name(), input)?;
    let response = provider
        .complete(&request)
        .map_err(|message| AiError::ProviderFailure {
            reason: classify_provider_failure(&message),
            message,
        })?;
    parse_response(&request, &response)
}

/// Parses and validates a provider response against its captured request.
///
/// # Errors
///
/// Returns [`AiError::MalformedResponse`] if the response is not a JSON object with exactly
/// `rationale` and `findings`, if `rationale` is empty or over 4000 characters, if `findings` has
/// over 100 entries, or if a finding is malformed. Returns [`AiError::UnsupportedFinding`] if a
/// finding names a candidate, code, or region absent from the request context.
pub fn parse_response(request: &CapturedRequest, response: &str) -> Result<Rationale, AiError> {
    let value: Value = serde_json::from_str(response)
        .map_err(|error| AiError::MalformedResponse(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| AiError::MalformedResponse("response must be an object".into()))?;
    require_keys(object, &["rationale", "findings"])?;
    let text = object
        .get("rationale")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty() && text.chars().count() <= 4000)
        .ok_or_else(|| AiError::MalformedResponse("rationale must be 1..=4000 chars".into()))?
        .to_string();
    let findings_value = object
        .get("findings")
        .and_then(Value::as_array)
        .filter(|items| items.len() <= 100)
        .ok_or_else(|| {
            AiError::MalformedResponse("findings must be an array of at most 100".into())
        })?;

    let mut findings = Vec::with_capacity(findings_value.len());
    for value in findings_value {
        let finding = parse_finding(value)?;
        validate_finding(request, &finding)?;
        findings.push(finding);
    }
    findings.sort_by(|left, right| {
        (&left.candidate, left.region, &left.code, &left.detail).cmp(&(
            &right.candidate,
            right.region,
            &right.code,
            &right.detail,
        ))
    });
    findings.dedup();

    Ok(Rationale {
        request: request.clone(),
        text,
        findings,
    })
}

/// Formats a validated rationale deterministically for logs or review.
pub fn format_rationale(rationale: &Rationale) -> String {
    let mut output = format!(
        "Provider: {}\nContext: {}\nRationale: {}",
        rationale.request.provider, rationale.request.context_fingerprint, rationale.text
    );
    for finding in &rationale.findings {
        output.push_str(&format!(
            "\n- {} [{}] at bar {} track {:?} voice {:?} beat {:?}: {}",
            finding.candidate,
            finding.code,
            finding.region.bar,
            finding.region.track,
            finding.region.voice,
            finding.region.beat,
            finding.detail
        ));
    }
    output
}

fn validate_prompt_input(input: &PromptInput) -> Result<(), AiError> {
    let expected = match input.task {
        PromptTask::Rationale | PromptTask::TabCritique | PromptTask::DrumCritique => Some(1),
        PromptTask::CandidateComparison => Some(2),
        PromptTask::CandidateJudging => Some(2),
        PromptTask::SectionLabels
        | PromptTask::SectionBoundaries
        | PromptTask::SectionAssessment => None,
    };
    if let Some(expected) = expected {
        let valid = if input.task == PromptTask::CandidateJudging {
            input.candidates.len() >= expected
        } else {
            input.candidates.len() == expected
        };
        if !valid || !input.section_fingerprints.is_empty() {
            return Err(AiError::InvalidContext(format!(
                "{} task has an invalid candidate or section count",
                input.task.as_str()
            )));
        }
    } else {
        let minimum = if matches!(
            input.task,
            PromptTask::SectionLabels | PromptTask::SectionAssessment
        ) {
            1
        } else {
            2
        };
        if !input.candidates.is_empty() || input.section_fingerprints.len() < minimum {
            return Err(AiError::InvalidContext(format!(
                "{} task requires at least {minimum} section fingerprint(s) and no candidates",
                input.task.as_str()
            )));
        }
        let mut ids = BTreeSet::new();
        for section in &input.section_fingerprints {
            if section.id.trim().is_empty() || !ids.insert(section.id.as_str()) {
                return Err(AiError::InvalidContext(
                    "section fingerprint ids must be nonempty and unique".into(),
                ));
            }
            if section
                .features
                .iter()
                .any(|(key, value)| key.trim().is_empty() || !value.is_finite())
            {
                return Err(AiError::InvalidContext(
                    "section feature names must be nonempty and values finite".into(),
                ));
            }
        }
        return Ok(());
    }
    let mut labels = BTreeSet::new();
    for candidate in &input.candidates {
        if candidate.label.trim().is_empty() || !labels.insert(candidate.label.as_str()) {
            return Err(AiError::InvalidContext(
                "candidate labels must be nonempty and unique".into(),
            ));
        }
    }
    Ok(())
}
