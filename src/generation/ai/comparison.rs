//! Deterministic metric and diagnostic comparison for provider-facing assessment reports.

use std::collections::{BTreeMap, BTreeSet};

use crate::generation::assessment::{Diagnostic, Measurement, Metric, Region};

use super::{validate_prompt_input, AiError, PromptInput, PromptTask};

/// Direction of the right candidate's metric relative to the left candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricDirection {
    /// Right metric is less than left.
    Lower,
    /// Both metrics are equal.
    Equal,
    /// Right metric is greater than left.
    Higher,
}

/// A metric comparison that does not declare either value better.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricComparison {
    /// Metric being compared.
    pub metric: Metric,
    /// Left candidate's exact value.
    pub left: Measurement,
    /// Right candidate's exact value.
    pub right: Measurement,
    /// Numeric direction from left to right.
    pub direction: MetricDirection,
}

/// Changes in assessment diagnostics between two candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticComparison {
    /// Diagnostic code.
    pub code: String,
    /// Region associated with the diagnostic.
    pub region: Region,
    /// Values reported for the left candidate.
    pub left_values: Vec<i64>,
    /// Values reported for the right candidate.
    pub right_values: Vec<i64>,
}

/// Structured, non-mutating comparison of two candidate assessment reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateEvaluation {
    /// Left candidate label.
    pub left_label: String,
    /// Right candidate label.
    pub right_label: String,
    /// Exact metric comparisons in stable metric order.
    pub metrics: Vec<MetricComparison>,
    /// Diagnostic values grouped by code and score region.
    pub diagnostics: Vec<DiagnosticComparison>,
}

/// Compares two assessments without selecting a winner or changing documents.
pub fn compare_candidates(input: &PromptInput) -> Result<CandidateEvaluation, AiError> {
    validate_prompt_input(input)?;
    if input.task != PromptTask::CandidateComparison || input.candidates.len() != 2 {
        return Err(AiError::InvalidContext(
            "candidate comparison requires exactly two comparison candidates".into(),
        ));
    }
    let left = &input.candidates[0];
    let right = &input.candidates[1];
    if left.assessment.metrics.keys().collect::<Vec<_>>()
        != right.assessment.metrics.keys().collect::<Vec<_>>()
    {
        return Err(AiError::InvalidContext(
            "candidate assessments must contain the same metrics".into(),
        ));
    }
    let mut metrics = Vec::new();
    for (metric, left_value) in &left.assessment.metrics {
        if let Some(right_value) = right.assessment.metrics.get(metric) {
            metrics.push(MetricComparison {
                metric: *metric,
                left: *left_value,
                right: *right_value,
                direction: compare_measurements(*left_value, *right_value),
            });
        }
    }
    let left_diagnostics = group_diagnostics(&left.assessment.diagnostics);
    let right_diagnostics = group_diagnostics(&right.assessment.diagnostics);
    let keys = left_diagnostics
        .keys()
        .chain(right_diagnostics.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let diagnostics = keys
        .into_iter()
        .map(|(code, region)| DiagnosticComparison {
            left_values: left_diagnostics
                .get(&(code.clone(), region))
                .cloned()
                .unwrap_or_default(),
            right_values: right_diagnostics
                .get(&(code.clone(), region))
                .cloned()
                .unwrap_or_default(),
            code,
            region,
        })
        .collect();
    Ok(CandidateEvaluation {
        left_label: left.label.clone(),
        right_label: right.label.clone(),
        metrics,
        diagnostics,
    })
}

fn compare_measurements(left: Measurement, right: Measurement) -> MetricDirection {
    let left_numerator = u128::from(left.numerator) * u128::from(right.denominator.max(1));
    let right_numerator = u128::from(right.numerator) * u128::from(left.denominator.max(1));
    match right_numerator.cmp(&left_numerator) {
        std::cmp::Ordering::Less => MetricDirection::Lower,
        std::cmp::Ordering::Equal => MetricDirection::Equal,
        std::cmp::Ordering::Greater => MetricDirection::Higher,
    }
}

fn group_diagnostics(diagnostics: &[Diagnostic]) -> BTreeMap<(String, Region), Vec<i64>> {
    let mut grouped = BTreeMap::new();
    for diagnostic in diagnostics {
        grouped
            .entry((diagnostic.code.to_string(), diagnostic.region))
            .or_insert_with(Vec::new)
            .push(diagnostic.value);
    }
    for values in grouped.values_mut() {
        values.sort_unstable();
    }
    grouped
}
