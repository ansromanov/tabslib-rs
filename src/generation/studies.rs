//! Caller-owned evaluation and study summaries for generated documents.
//!
//! These harnesses provide repeatable reporting without shipping score files or
//! style policy. Applications supply the known cases, reference measurements,
//! and gate thresholds; the library records exactly what was evaluated.

#[cfg(all(feature = "ai", feature = "ascii"))]
use std::collections::BTreeMap;

use crate::model::Document;

use super::assessment::{
    assess, calibrate, AssessmentReport, CalibrationReport, CalibrationSample,
};
#[cfg(feature = "ai")]
use super::preference::PreferenceRecord;
use super::preference::{GatePolicy, GateReport};

/// One caller-supplied known regression case.
#[derive(Debug, Clone)]
pub struct KnownRegressionCase {
    /// Stable case label used in reports.
    pub label: String,
    /// Generated document under evaluation.
    pub document: Document,
    /// External reference measurements for this case.
    pub references: Vec<CalibrationSample>,
    /// Caller-owned tolerance and quarantine policy.
    pub policy: GatePolicy,
}

/// Results for one known regression case.
#[derive(Debug, Clone)]
pub struct KnownRegressionResult {
    /// Case label.
    pub label: String,
    /// Deterministic assessment of the generated document.
    pub assessment: AssessmentReport,
    /// Comparison with the supplied references.
    pub calibration: CalibrationReport,
    /// Gate verdict under the supplied policy.
    pub gate: GateReport,
}

/// Runs all supplied known regression cases in caller order.
pub fn evaluate_known_regression_cases(
    cases: &[KnownRegressionCase],
) -> Vec<KnownRegressionResult> {
    cases
        .iter()
        .map(|case| {
            let assessment = assess(&case.document);
            let calibration = calibrate(&assessment, &case.references);
            let gate = super::preference::gate(&assessment, &case.references, &case.policy);
            KnownRegressionResult {
                label: case.label.clone(),
                assessment,
                calibration,
                gate,
            }
        })
        .collect()
}

/// Aggregate result for a known-regression study.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelJudgeStudySummary {
    /// Number of evaluated cases.
    pub cases: usize,
    /// Cases whose caller-owned gates passed.
    pub passed: usize,
    /// Cases whose caller-owned gates failed.
    pub failed: usize,
}

impl ModelJudgeStudySummary {
    /// Whether every evaluated case passed.
    pub const fn passed(self) -> bool {
        self.failed == 0
    }
}

/// Summarizes known-regression verdicts without discarding per-case reports.
pub fn summarize_model_judge_study(results: &[KnownRegressionResult]) -> ModelJudgeStudySummary {
    let passed = results.iter().filter(|result| result.gate.passed()).count();
    ModelJudgeStudySummary {
        cases: results.len(),
        passed,
        failed: results.len().saturating_sub(passed),
    }
}

/// A compact, stable summary of structured tab critiques.
#[cfg(all(feature = "ai", feature = "ascii"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabCritiqueStudySummary {
    /// Number of critique responses evaluated.
    pub cases: usize,
    /// Cases containing at least one structured finding.
    pub cases_with_findings: usize,
    /// Finding counts grouped by stable defect class name.
    pub findings_by_class: BTreeMap<String, usize>,
}

/// Summarizes structured tab critiques, including empty critiques.
#[cfg(all(feature = "ai", feature = "ascii"))]
pub fn summarize_tab_critique_study(
    critiques: &[super::ai::TabCritique],
) -> TabCritiqueStudySummary {
    let mut findings_by_class = BTreeMap::new();
    let cases_with_findings = critiques
        .iter()
        .filter(|critique| !critique.findings.is_empty())
        .count();
    for finding in critiques.iter().flat_map(|critique| &critique.findings) {
        let class = format!("{:?}", finding.class);
        *findings_by_class.entry(class).or_insert(0) += 1;
    }
    TabCritiqueStudySummary {
        cases: critiques.len(),
        cases_with_findings,
        findings_by_class,
    }
}

/// Compares a provider ranking with blind preference records.
#[cfg(feature = "ai")]
pub fn evaluate_judge_against_preferences(
    ranking: &super::ai::CandidateRanking,
    records: &[PreferenceRecord],
) -> Result<super::ai::JudgeAgreement, super::ai::AiError> {
    super::ai::judge_preference_agreement(ranking, records)
}
