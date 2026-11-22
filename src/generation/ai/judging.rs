//! Provider-backed candidate judging, comparison, and structured tablature critique.
//!
//! This module evaluates only caller-supplied assessments and validates all
//! provider output against the captured candidates or rendered tab.

use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "ascii")]
use serde_json::json;
use serde_json::{Map, Value};

use super::{
    capture_request, classify_provider_failure, validate_prompt_input, AiError, CapturedRequest,
    PromptInput, PromptTask, RationaleFinding, RationaleProvider,
};
#[cfg(feature = "ascii")]
use super::{stable_fingerprint, PromptCandidate};
#[cfg(feature = "ascii")]
use crate::format::{ascii::Ascii, WriteFormat};
#[cfg(feature = "ascii")]
use crate::generation::assessment::assess;
use crate::generation::assessment::{Measurement, Metric, Region, Severity};
use crate::generation::preference::{Choice, PreferenceRecord, RECORD_VERSION};
#[cfg(feature = "ascii")]
use crate::model::Document;

/// A versioned best-to-worst candidate ranking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateRanking {
    /// Ranking response schema version.
    pub version: u32,
    /// Candidate labels ordered best to worst.
    pub ranking: Vec<String>,
}

/// Asks a provider to rank assessment reports and validates its complete order.
pub fn judge_candidates(
    provider: &impl RationaleProvider,
    input: PromptInput,
) -> Result<CandidateRanking, AiError> {
    if input.task != PromptTask::CandidateJudging {
        return Err(AiError::InvalidContext(
            "candidate judging requires a candidate-judging prompt".into(),
        ));
    }
    let request = capture_request(provider.name(), input)?;
    let response = provider
        .complete(&request)
        .map_err(|message| AiError::ProviderFailure {
            reason: classify_provider_failure(&message),
            message,
        })?;
    parse_candidate_ranking(&request, &response)
}

/// Parses a versioned ranking and requires every prompt candidate exactly once.
pub fn parse_candidate_ranking(
    request: &CapturedRequest,
    response: &str,
) -> Result<CandidateRanking, AiError> {
    if request.input.task != PromptTask::CandidateJudging {
        return Err(AiError::InvalidContext(
            "ranking response requires a candidate-judging request".into(),
        ));
    }
    let value: Value = serde_json::from_str(response)
        .map_err(|error| AiError::MalformedResponse(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| AiError::MalformedResponse("ranking must be an object".into()))?;
    require_keys(object, &["schema_version", "ranking"])?;
    let version = object
        .get("schema_version")
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .ok_or_else(|| AiError::MalformedResponse("schema_version must be an integer".into()))?;
    if version != 1 {
        return Err(AiError::MalformedResponse(format!(
            "unsupported ranking schema version {version}"
        )));
    }
    let ranking = object
        .get("ranking")
        .and_then(Value::as_array)
        .ok_or_else(|| AiError::MalformedResponse("ranking must be an array".into()))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|label| !label.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    AiError::MalformedResponse("ranking labels must be nonempty strings".into())
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let expected = request
        .input
        .candidates
        .iter()
        .map(|candidate| candidate.label.as_str())
        .collect::<BTreeSet<_>>();
    let actual = ranking.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if ranking.len() != expected.len() || actual != expected {
        return Err(AiError::InvalidRanking(
            "ranking must contain each prompt candidate exactly once".into(),
        ));
    }
    Ok(CandidateRanking { version, ranking })
}

/// Blends provider and deterministic rank orders with a caller-selected weight.
///
/// Rank positions are normalized to 0..=1. A weight of one uses the provider
/// order alone; zero uses the deterministic order alone. Ties sort by label.
pub fn blend_rankings_with_deterministic(
    model_ranking: &CandidateRanking,
    deterministic_ranking: &[String],
    model_weight: f64,
) -> Result<CandidateRanking, AiError> {
    if !model_weight.is_finite() || !(0.0..=1.0).contains(&model_weight) {
        return Err(AiError::InvalidModelWeight);
    }
    let model_labels = model_ranking
        .ranking
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let deterministic_labels = deterministic_ranking
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if model_ranking.ranking.len() != model_labels.len()
        || deterministic_ranking.len() != deterministic_labels.len()
        || model_labels != deterministic_labels
    {
        return Err(AiError::InvalidRanking(
            "rankings must contain the same unique candidate labels".into(),
        ));
    }
    let count = model_ranking.ranking.len();
    let normalized = |index: usize| {
        if count <= 1 {
            1.0
        } else {
            1.0 - index as f64 / (count - 1) as f64
        }
    };
    let model_scores = model_ranking
        .ranking
        .iter()
        .enumerate()
        .map(|(index, label)| (label.as_str(), normalized(index)))
        .collect::<BTreeMap<_, _>>();
    let deterministic_scores = deterministic_ranking
        .iter()
        .enumerate()
        .map(|(index, label)| (label.as_str(), normalized(index)))
        .collect::<BTreeMap<_, _>>();
    let mut labels = model_labels.into_iter().collect::<Vec<_>>();
    labels.sort_by(|left, right| {
        let left_score =
            model_weight * model_scores[left] + (1.0 - model_weight) * deterministic_scores[left];
        let right_score =
            model_weight * model_scores[right] + (1.0 - model_weight) * deterministic_scores[right];
        right_score.total_cmp(&left_score).then(left.cmp(right))
    });
    Ok(CandidateRanking {
        version: 1,
        ranking: labels.into_iter().map(str::to_string).collect(),
    })
}

/// Ranks candidates offline using caller-owned signed metric weights.
///
/// Positive weights prefer larger measurements and negative weights prefer
/// smaller ones. The function makes no default quality judgments.
pub fn offline_heuristic_judge(
    input: &PromptInput,
    metric_weights: &BTreeMap<Metric, f64>,
) -> Result<CandidateRanking, AiError> {
    if input.task != PromptTask::CandidateJudging || metric_weights.is_empty() {
        return Err(AiError::InvalidContext(
            "offline judging needs a candidate-judging prompt and metric weights".into(),
        ));
    }
    validate_prompt_input(input)?;
    if metric_weights.values().any(|weight| !weight.is_finite()) {
        return Err(AiError::InvalidContext(
            "offline judge weights must be finite".into(),
        ));
    }
    let mut scores = Vec::with_capacity(input.candidates.len());
    for candidate in &input.candidates {
        let score = metric_weights
            .iter()
            .filter_map(|(metric, weight)| {
                candidate
                    .assessment
                    .metrics
                    .get(metric)
                    .map(|value| *weight * measurement_value(*value))
            })
            .sum::<f64>();
        scores.push((candidate.label.as_str(), score));
    }
    scores.sort_by(|left, right| right.1.total_cmp(&left.1).then(left.0.cmp(right.0)));
    Ok(CandidateRanking {
        version: 1,
        ranking: scores
            .into_iter()
            .map(|(label, _)| label.to_string())
            .collect(),
    })
}

/// Agreement between a ranking and decisive recorded preferences.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JudgeAgreement {
    /// Correct pairwise predictions.
    pub correct: usize,
    /// Decisive records whose labels were present in the ranking.
    pub decisive: usize,
}

/// A countable defect category for rendered tablature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TabDefectClass {
    /// A repeated pattern continues without a score-level change.
    LoopWithoutDevelopment,
    /// Repeated notes lack an articulation or dynamic change.
    MissingArticulation,
    /// A lead line stays within a narrow fret-position span.
    ConfinedSoloPosition,
    /// Adjacent sections join with an unusually large written gap or jump.
    WeakSectionJoin,
    /// A track contributes no notes in the rendered arrangement.
    LayerDoesNotEarnPlace,
}

impl TabDefectClass {
    #[cfg(feature = "ascii")]
    fn as_str(self) -> &'static str {
        match self {
            Self::LoopWithoutDevelopment => "loop-without-development",
            Self::MissingArticulation => "missing-articulation",
            Self::ConfinedSoloPosition => "confined-solo-position",
            Self::WeakSectionJoin => "weak-section-join",
            Self::LayerDoesNotEarnPlace => "layer-does-not-earn-place",
        }
    }

    #[cfg(feature = "ascii")]
    fn parse(value: &str) -> Option<Self> {
        match value {
            "loop-without-development" => Some(Self::LoopWithoutDevelopment),
            "missing-articulation" => Some(Self::MissingArticulation),
            "confined-solo-position" => Some(Self::ConfinedSoloPosition),
            "weak-section-join" => Some(Self::WeakSectionJoin),
            "layer-does-not-earn-place" => Some(Self::LayerDoesNotEarnPlace),
            _ => None,
        }
    }
}

/// A validated finding tied to one rendered tablature bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabCritiqueFinding {
    /// Defect class from the supported vocabulary.
    pub class: TabDefectClass,
    /// Finding severity.
    pub severity: Severity,
    /// Zero-based bar index in the supplied document.
    pub bar: usize,
    /// Countable observation supported by the rendered tab.
    pub detail: String,
}

/// Validated structured critique of an ASCII tab rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabCritique {
    /// Exact request and rendered tab sent to the provider.
    pub request: CapturedRequest,
    /// Findings sorted by bar, class, and detail.
    pub findings: Vec<TabCritiqueFinding>,
}

/// A cited bar claim made by a judge or evaluation model.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BarClaim {
    /// Zero-based bar index being cited.
    pub bar: usize,
    /// What the model claims about the bar (e.g. density, chord, technique).
    pub assertion: String,
    /// Optional asserted value or description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Verification outcome of a cited bar claim.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VerifiedBarClaim {
    /// The original bar claim.
    pub claim: BarClaim,
    /// Whether the claimed bar is verified as present in the material.
    pub verified: bool,
    /// Reason explaining why the claim could not be verified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Verifies cited bar numbers in claims against the bars present in rendered tab text.
pub fn verify_bar_claims_in_tab(tab_text: &str, claims: &[BarClaim]) -> Vec<VerifiedBarClaim> {
    let mut bars = BTreeSet::new();
    for line in tab_text.lines() {
        if let Some(open) = line.find('[') {
            if let Some(close) = line[open..].find(']') {
                let inner = &line[open + 1..open + close];
                if let Some((start_str, end_str)) = inner.split_once("..=") {
                    if let (Ok(start), Ok(end)) = (
                        start_str.trim().parse::<usize>(),
                        end_str.trim().parse::<usize>(),
                    ) {
                        let s = start.saturating_sub(1);
                        let e = end.saturating_sub(1);
                        for b in s..=e {
                            bars.insert(b);
                        }
                    }
                }
            }
        }
        for segment in line.split('|') {
            let trimmed = segment.trim();
            if let Ok(num) = trimmed.parse::<usize>() {
                bars.insert(num);
                if num > 0 {
                    bars.insert(num - 1);
                }
            }
        }
    }

    claims
        .iter()
        .map(|claim| {
            if bars.contains(&claim.bar) {
                VerifiedBarClaim {
                    claim: claim.clone(),
                    verified: true,
                    reason: None,
                }
            } else {
                VerifiedBarClaim {
                    claim: claim.clone(),
                    verified: false,
                    reason: Some(format!(
                        "bar {} is not present in the rendered candidate tab",
                        claim.bar
                    )),
                }
            }
        })
        .collect()
}

/// Verifies cited bar claims against an authored document's master bars.
#[cfg(feature = "ascii")]
pub fn verify_bar_claims(doc: &Document, claims: &[BarClaim]) -> Vec<VerifiedBarClaim> {
    let bar_count = doc.master_bars.len();
    claims
        .iter()
        .map(|claim| {
            if claim.bar < bar_count {
                VerifiedBarClaim {
                    claim: claim.clone(),
                    verified: true,
                    reason: None,
                }
            } else {
                VerifiedBarClaim {
                    claim: claim.clone(),
                    verified: false,
                    reason: Some(format!(
                        "bar {} is outside the document (bar count: {bar_count})",
                        claim.bar
                    )),
                }
            }
        })
        .collect()
}

/// Captures a critique prompt containing format::ascii output and assessment facts.
#[cfg(feature = "ascii")]
pub fn capture_tab_critique_request(
    provider: impl Into<String>,
    label: impl Into<String>,
    document: &Document,
) -> Result<CapturedRequest, AiError> {
    let label = label.into();
    if label.trim().is_empty() {
        return Err(AiError::InvalidContext("tab label is empty".into()));
    }
    let tab_text = String::from_utf8(
        Ascii::write(document).map_err(|error| AiError::InvalidContext(error.to_string()))?,
    )
    .map_err(|error| AiError::InvalidContext(error.to_string()))?;
    let report = assess(document);
    let input = PromptInput {
        task: PromptTask::TabCritique,
        candidates: vec![PromptCandidate {
            label: label.clone(),
            assessment: report.clone(),
        }],
        section_fingerprints: Vec::new(),
    };
    let provider = provider.into();
    if provider.trim().is_empty() {
        return Err(AiError::InvalidContext("provider name is empty".into()));
    }
    let assessment: Value = serde_json::from_str(&report.to_json())
        .map_err(|error| AiError::InvalidContext(error.to_string()))?;
    let context_json = serde_json::to_string(&json!({
        "schema_version": 1,
        "task": "tab_critique",
        "candidate": {"label": label, "assessment": assessment},
        "ascii_tab": tab_text,
    }))
    .map_err(|error| AiError::InvalidContext(error.to_string()))?;
    let supported_classes = [
        TabDefectClass::LoopWithoutDevelopment,
        TabDefectClass::MissingArticulation,
        TabDefectClass::ConfinedSoloPosition,
        TabDefectClass::WeakSectionJoin,
        TabDefectClass::LayerDoesNotEarnPlace,
    ]
    .into_iter()
    .map(TabDefectClass::as_str)
    .collect::<Vec<_>>()
    .join(", ");
    let prompt = format!(
        "Inspect only the ASCII tablature below and its assessment. Return one JSON object with exactly schema_version (integer 1) and findings (array). Each finding must have exactly class (one of: {supported_classes}), severity (info, warning, or error), bar (zero-based integer), and detail (nonempty countable observation). Do not return praise or free-text commentary; use an empty findings array when no supported defect is visible.\n{context_json}"
    );
    let context_fingerprint = stable_fingerprint(context_json.as_bytes());
    Ok(CapturedRequest {
        provider,
        input,
        context_json,
        context_fingerprint,
        prompt,
        tab_text: Some(tab_text),
        bar_count: Some(document.master_bars.len()),
    })
}

/// Asks a provider to critique the rendered ASCII tab with structured findings.
#[cfg(feature = "ascii")]
pub fn critique_tab(
    provider: &impl RationaleProvider,
    label: impl Into<String>,
    document: &Document,
) -> Result<TabCritique, AiError> {
    let request = capture_tab_critique_request(provider.name(), label, document)?;
    let response = provider
        .complete(&request)
        .map_err(|message| AiError::ProviderFailure {
            reason: classify_provider_failure(&message),
            message,
        })?;
    parse_tab_critique_response(&request, &response)
}

/// Validates a provider critique, rejecting prose-only praise and unknown bars.
#[cfg(feature = "ascii")]
pub fn parse_tab_critique_response(
    request: &CapturedRequest,
    response: &str,
) -> Result<TabCritique, AiError> {
    if request.input.task != PromptTask::TabCritique || request.tab_text.is_none() {
        return Err(AiError::InvalidContext(
            "tab critique response requires an ASCII critique request".into(),
        ));
    }
    let value: Value = serde_json::from_str(response)
        .map_err(|error| AiError::MalformedResponse(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| AiError::MalformedResponse("critique must be an object".into()))?;
    require_keys(object, &["schema_version", "findings"])?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(AiError::MalformedResponse(
            "unsupported critique schema version".into(),
        ));
    }
    let values = object
        .get("findings")
        .and_then(Value::as_array)
        .filter(|values| values.len() <= 100)
        .ok_or_else(|| {
            AiError::MalformedResponse("findings must be an array of at most 100".into())
        })?;
    let bar_count = request.bar_count.unwrap_or(0);
    let mut findings = Vec::with_capacity(values.len());
    for value in values {
        let finding = value
            .as_object()
            .ok_or_else(|| AiError::MalformedResponse("finding must be an object".into()))?;
        require_keys(finding, &["class", "severity", "bar", "detail"])?;
        let class = finding
            .get("class")
            .and_then(Value::as_str)
            .and_then(TabDefectClass::parse)
            .ok_or_else(|| AiError::UnsupportedFinding("unknown tab defect class".into()))?;
        let severity = match finding.get("severity").and_then(Value::as_str) {
            Some("info") => Severity::Info,
            Some("warning") => Severity::Warning,
            Some("error") => Severity::Error,
            _ => return Err(AiError::UnsupportedFinding("unknown severity".into())),
        };
        let bar = parse_index(finding.get("bar"), "bar")?
            .ok_or_else(|| AiError::MalformedResponse("bar must be an integer".into()))?;
        if bar >= bar_count {
            return Err(AiError::UnsupportedFinding(format!(
                "bar {bar} is outside the rendered tab"
            )));
        }
        let detail = finding
            .get("detail")
            .and_then(Value::as_str)
            .filter(|detail| !detail.trim().is_empty() && detail.chars().count() <= 1000)
            .ok_or_else(|| AiError::MalformedResponse("detail must be 1..=1000 chars".into()))?
            .to_string();
        findings.push(TabCritiqueFinding {
            class,
            severity,
            bar,
            detail,
        });
    }
    findings.sort_by(|left, right| {
        (left.bar, left.class, &left.detail).cmp(&(right.bar, right.class, &right.detail))
    });
    findings.dedup();
    Ok(TabCritique {
        request: request.clone(),
        findings,
    })
}

impl JudgeAgreement {
    /// Correct predictions divided by evaluated comparisons.
    pub fn rate(self) -> f64 {
        self.correct as f64 / self.decisive.max(1) as f64
    }
}

/// Measures the ranking's pairwise agreement with available preference records.
pub fn judge_preference_agreement(
    ranking: &CandidateRanking,
    records: &[PreferenceRecord],
) -> Result<JudgeAgreement, AiError> {
    let positions = ranking
        .ranking
        .iter()
        .enumerate()
        .map(|(index, label)| (label.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let mut decisive = 0;
    let mut correct = 0;
    for record in records {
        if record.version != RECORD_VERSION {
            return Err(AiError::InvalidContext(format!(
                "unsupported preference record version {}",
                record.version
            )));
        }
        if !record.choice.is_decisive() {
            continue;
        }
        let (preferred, other) = match record.choice {
            Choice::First => (&record.first, &record.second),
            Choice::Second => (&record.second, &record.first),
            Choice::NoPreference | Choice::BothBad | Choice::Third => continue,
        };
        let (Some(preferred_rank), Some(other_rank)) = (
            positions.get(preferred.label.as_str()),
            positions.get(other.label.as_str()),
        ) else {
            continue;
        };
        decisive += 1;
        correct += usize::from(preferred_rank < other_rank);
    }
    if decisive == 0 {
        return Err(AiError::InvalidContext(
            "no decisive preference pairs overlap the ranking".into(),
        ));
    }
    Ok(JudgeAgreement { correct, decisive })
}

pub(super) fn measurement_value(value: Measurement) -> f64 {
    value.numerator as f64 / value.denominator.max(1) as f64
}

pub(super) fn require_keys(object: &Map<String, Value>, keys: &[&str]) -> Result<(), AiError> {
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = keys.iter().copied().collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(AiError::MalformedResponse(format!(
            "expected keys {expected:?}, got {actual:?}"
        )));
    }
    Ok(())
}

pub(super) fn parse_finding(value: &Value) -> Result<RationaleFinding, AiError> {
    let object = value
        .as_object()
        .ok_or_else(|| AiError::MalformedResponse("each finding must be an object".into()))?;
    require_keys(
        object,
        &[
            "candidate",
            "code",
            "bar",
            "track",
            "voice",
            "beat",
            "detail",
        ],
    )?;
    let string = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| AiError::MalformedResponse(format!("{key} must be a nonempty string")))
    };
    let bar = parse_index(object.get("bar"), "bar")?
        .ok_or_else(|| AiError::MalformedResponse("bar must be a nonnegative integer".into()))?;
    let detail = string("detail")?;
    if detail.chars().count() > 1000 {
        return Err(AiError::MalformedResponse(
            "finding detail must be at most 1000 chars".into(),
        ));
    }
    Ok(RationaleFinding {
        candidate: string("candidate")?,
        code: string("code")?,
        region: Region {
            bar,
            track: parse_index(object.get("track"), "track")?,
            voice: parse_index(object.get("voice"), "voice")?,
            beat: parse_index(object.get("beat"), "beat")?,
        },
        detail,
    })
}

fn parse_index(value: Option<&Value>, name: &str) -> Result<Option<usize>, AiError> {
    match value {
        Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|number| usize::try_from(number).ok())
            .map(Some)
            .ok_or_else(|| {
                AiError::MalformedResponse(format!("{name} must be null or a nonnegative integer"))
            }),
        None => Err(AiError::MalformedResponse(format!("missing {name}"))),
    }
}

pub(super) fn validate_finding(
    request: &CapturedRequest,
    finding: &RationaleFinding,
) -> Result<(), AiError> {
    let supported = request.input.candidates.iter().any(|candidate| {
        candidate.label == finding.candidate
            && candidate.assessment.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == finding.code && diagnostic.region == finding.region
            })
    });
    if supported {
        Ok(())
    } else {
        Err(AiError::UnsupportedFinding(format!(
            "{} [{}] at {:?}",
            finding.candidate, finding.code, finding.region
        )))
    }
}
