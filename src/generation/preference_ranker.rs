//! Learns a deterministic candidate ranker from blind preference records.
//!
//! Callers pair each record with the two candidates' assessment reports. The
//! module owns [`train_preference_ranker`], scoring, ranking, held-out
//! agreement, and versioned JSON persistence; it ships no trained model.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::assessment::{AssessmentReport, Measurement, Metric};
use super::preference::{Choice, PreferenceRecord, RECORD_VERSION};

/// Schema written by [`PreferenceRanker::to_json`].
pub const PREFERENCE_RANKER_VERSION: u32 = 1;

/// One candidate paired with its deterministic assessment metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferenceCandidate {
    /// Caller-owned candidate identifier.
    pub label: String,
    /// Assessment values for the candidate.
    pub metrics: BTreeMap<Metric, Measurement>,
}

impl PreferenceCandidate {
    /// Creates a candidate from an assessment report.
    pub fn from_report(label: impl Into<String>, report: &AssessmentReport) -> Self {
        Self {
            label: label.into(),
            metrics: report.metrics.clone(),
        }
    }
}

/// A preference record and the assessment report for each recorded side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferenceTrainingPair {
    /// The listener's blind comparison.
    pub record: PreferenceRecord,
    /// Assessment values for the first side.
    pub first: AssessmentReport,
    /// Assessment values for the second side.
    pub second: AssessmentReport,
}

/// A standardized model coefficient for one assessment metric.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PreferenceWeight {
    /// Mean observed while training.
    pub mean: f64,
    /// Standard deviation used to normalize candidate values.
    pub scale: f64,
    /// Learned preference coefficient; positive means larger values scored higher.
    pub weight: f64,
}

/// A versioned, caller-trained preference model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreferenceRanker {
    /// JSON schema version. Unsupported versions are rejected when loading.
    pub version: u32,
    /// Number of decisive comparisons used for training.
    pub training_records: usize,
    /// Metric normalization and learned coefficients in stable metric order.
    pub weights: BTreeMap<Metric, PreferenceWeight>,
}

impl PreferenceRanker {
    /// Serializes this model as pretty JSON.
    pub fn to_json(&self) -> Result<String, PreferenceRankerError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| PreferenceRankerError::Serialization(error.to_string()))
    }

    /// Parses a model and rejects unknown or invalid schema data.
    pub fn from_json(json: &str) -> Result<Self, PreferenceRankerError> {
        let model: Self = serde_json::from_str(json)
            .map_err(|error| PreferenceRankerError::Serialization(error.to_string()))?;
        if model.version != PREFERENCE_RANKER_VERSION {
            return Err(PreferenceRankerError::UnsupportedVersion(model.version));
        }
        if model.training_records == 0
            || model.weights.is_empty()
            || model.weights.values().any(|value| {
                !value.mean.is_finite()
                    || !value.scale.is_finite()
                    || value.scale <= 0.0
                    || !value.weight.is_finite()
            })
        {
            return Err(PreferenceRankerError::InvalidModel);
        }
        Ok(model)
    }
}

/// A candidate and its learned score.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredPreferenceCandidate {
    /// Candidate whose assessment was scored.
    pub candidate: PreferenceCandidate,
    /// Deterministic weighted score; higher ranks first.
    pub score: f64,
}

/// Held-out agreement on decisive preference records.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreferenceAgreement {
    /// Number of held-out choices the ranker predicted correctly.
    pub correct: usize,
    /// Number of decisive held-out choices evaluated.
    pub decisive: usize,
}

impl PreferenceAgreement {
    /// Correct predictions divided by the number evaluated.
    pub fn rate(self) -> f64 {
        self.correct as f64 / self.decisive.max(1) as f64
    }
}

/// Why preference ranking could not train, score, or load a model.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PreferenceRankerError {
    /// There were fewer decisive records than the caller requires.
    #[error("need at least {minimum} decisive records, received {actual}")]
    TooFewDecisive {
        /// Minimum number of decisive records required.
        minimum: usize,
        /// Number of decisive records received.
        actual: usize,
    },
    /// Decisive examples do not carry the same metric keys on both sides.
    #[error("each pair must carry matching, nonempty assessment metric sets")]
    InconsistentMetrics,
    /// The ranker JSON could not be serialized or parsed.
    #[error("preference ranker serialization failed: {0}")]
    Serialization(String),
    /// The model was written by an unsupported schema version.
    #[error("unsupported preference ranker version {0}")]
    UnsupportedVersion(u32),
    /// The model contains no useful metrics or invalid numeric values.
    #[error("preference ranker model is empty or contains invalid values")]
    InvalidModel,
    /// No decisive held-out choice was available for evaluation.
    #[error("held-out evaluation needs at least one decisive choice")]
    EmptyEvaluation,
}

/// Trains a pairwise logistic ranker from decisive blind comparisons.
///
/// `minimum_decisive_records` is caller policy: the library does not decide
/// how much listening data is enough to trust a model. No-preference and
/// both-bad records are skipped.
pub fn train_preference_ranker(
    examples: &[PreferenceTrainingPair],
    minimum_decisive_records: usize,
) -> Result<PreferenceRanker, PreferenceRankerError> {
    let decisive = examples
        .iter()
        .filter(|example| example.record.choice.is_decisive())
        .collect::<Vec<_>>();
    if decisive.len() < minimum_decisive_records.max(1) {
        return Err(PreferenceRankerError::TooFewDecisive {
            minimum: minimum_decisive_records.max(1),
            actual: decisive.len(),
        });
    }
    let mut metrics = BTreeSet::new();
    for example in &decisive {
        if example.record.version != RECORD_VERSION
            || example.first.metrics.is_empty()
            || example
                .first
                .metrics
                .keys()
                .ne(example.second.metrics.keys())
        {
            return Err(PreferenceRankerError::InconsistentMetrics);
        }
        metrics.extend(example.first.metrics.keys().copied());
    }

    let mut means = BTreeMap::new();
    let mut scales = BTreeMap::new();
    for metric in &metrics {
        let values = decisive
            .iter()
            .flat_map(|example| {
                [
                    example.first.metrics[metric].as_f64(),
                    example.second.metrics[metric].as_f64(),
                ]
            })
            .collect::<Vec<_>>();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / values.len() as f64;
        means.insert(*metric, mean);
        scales.insert(*metric, variance.sqrt().max(1e-9));
    }

    let mut training_vectors = Vec::with_capacity(decisive.len());
    for example in &decisive {
        let (preferred, other) = match example.record.choice {
            Choice::First => (&example.first, &example.second),
            Choice::Second => (&example.second, &example.first),
            Choice::NoPreference | Choice::BothBad | Choice::Third => continue,
        };
        training_vectors.push(
            metrics
                .iter()
                .map(|metric| {
                    let scale = scales[metric];
                    let difference = (preferred.metrics[metric].as_f64()
                        - other.metrics[metric].as_f64())
                        / scale;
                    (*metric, difference)
                })
                .collect::<BTreeMap<_, _>>(),
        );
    }
    let mut coefficients = metrics
        .iter()
        .map(|metric| (*metric, 0.0))
        .collect::<BTreeMap<_, _>>();
    for _ in 0..256 {
        for example in &training_vectors {
            let margin = example
                .iter()
                .map(|(metric, value)| coefficients[metric] * value)
                .sum::<f64>();
            let gradient = 1.0 / (1.0 + margin.exp());
            for (metric, value) in example {
                *coefficients.entry(*metric).or_default() +=
                    0.08 * gradient * value / training_vectors.len() as f64;
            }
        }
    }

    let weights = metrics
        .into_iter()
        .map(|metric| {
            (
                metric,
                PreferenceWeight {
                    mean: means[&metric],
                    scale: scales[&metric],
                    weight: coefficients[&metric],
                },
            )
        })
        .collect();
    Ok(PreferenceRanker {
        version: PREFERENCE_RANKER_VERSION,
        training_records: decisive.len(),
        weights,
    })
}

/// Scores a candidate using only metrics represented by the trained model.
pub fn score_preference_candidate(
    model: &PreferenceRanker,
    candidate: &PreferenceCandidate,
) -> f64 {
    model
        .weights
        .iter()
        .filter_map(|(metric, weight)| {
            candidate
                .metrics
                .get(metric)
                .map(|value| weight.weight * (value.as_f64() - weight.mean) / weight.scale)
        })
        .sum()
}

/// Scores and deterministically sorts candidates, breaking ties by label.
pub fn rank_preference_candidates(
    model: &PreferenceRanker,
    candidates: &[PreferenceCandidate],
) -> Vec<ScoredPreferenceCandidate> {
    let mut ranked = candidates
        .iter()
        .cloned()
        .map(|candidate| ScoredPreferenceCandidate {
            score: score_preference_candidate(model, &candidate),
            candidate,
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then(left.candidate.label.cmp(&right.candidate.label))
    });
    ranked
}

/// Measures agreement with held-out decisive comparisons.
pub fn preference_ranker_agreement(
    model: &PreferenceRanker,
    examples: &[PreferenceTrainingPair],
) -> Result<PreferenceAgreement, PreferenceRankerError> {
    let decisive = examples
        .iter()
        .filter(|example| example.record.choice.is_decisive())
        .collect::<Vec<_>>();
    if decisive.is_empty() {
        return Err(PreferenceRankerError::EmptyEvaluation);
    }
    let correct = decisive
        .iter()
        .filter(|example| {
            let first =
                PreferenceCandidate::from_report(&example.record.first.label, &example.first);
            let second =
                PreferenceCandidate::from_report(&example.record.second.label, &example.second);
            let predicted_first = score_preference_candidate(model, &first)
                .total_cmp(&score_preference_candidate(model, &second))
                .is_gt()
                || (score_preference_candidate(model, &first)
                    .total_cmp(&score_preference_candidate(model, &second))
                    .is_eq()
                    && first.label <= second.label);
            matches!(
                (example.record.choice, predicted_first),
                (Choice::First, true) | (Choice::Second, false)
            )
        })
        .count();
    Ok(PreferenceAgreement {
        correct,
        decisive: decisive.len(),
    })
}

impl Measurement {
    fn as_f64(self) -> f64 {
        self.numerator as f64 / self.denominator.max(1) as f64
    }
}

#[cfg(test)]
#[path = "preference_ranker_test.rs"]
mod tests;
