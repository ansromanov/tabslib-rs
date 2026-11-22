//! Exact time budgeting and seeded arrangement search.
//!
//! This module owns exact bar-duration calculations, repeat resolution, and
//! candidate-plan search. Its scoring policy comes entirely from the caller.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use thiserror::Error;

use crate::generation::assessment::{self, AssessmentReport, Metric};
use crate::generation::requirements::{self, Violation, DEFAULT_BPM};
use crate::inspect;
use crate::model::{Document, TempoChange};

use super::arrangement::{self, ArrangementError, ArrangementPlan, Motif};

/// An exact non-negative duration, expressed as a rational number of seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactDuration {
    /// Numerator of the duration in seconds.
    pub numerator: u128,
    /// Positive denominator of the duration in seconds.
    pub denominator: u128,
}

impl ExactDuration {
    fn new(numerator: u128, denominator: u128) -> Self {
        let divisor = gcd(numerator, denominator);
        Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        }
    }

    fn checked_add(self, other: Self) -> Result<Self, ArrangementSearchError> {
        let shared = gcd(self.denominator, other.denominator);
        let left_scale = other.denominator / shared;
        let right_scale = self.denominator / shared;
        let numerator = self
            .numerator
            .checked_mul(left_scale)
            .and_then(|left| {
                other
                    .numerator
                    .checked_mul(right_scale)
                    .and_then(|right| left.checked_add(right))
            })
            .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
        let denominator = self
            .denominator
            .checked_mul(left_scale)
            .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
        Ok(Self::new(numerator, denominator))
    }

    fn checked_mul(self, multiplier: u128) -> Result<Self, ArrangementSearchError> {
        let numerator = self
            .numerator
            .checked_mul(multiplier)
            .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
        Ok(Self::new(numerator, self.denominator))
    }

    fn checked_cmp(self, other: Self) -> Result<Ordering, ArrangementSearchError> {
        let left = self
            .numerator
            .checked_mul(other.denominator)
            .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
        let right = other
            .numerator
            .checked_mul(self.denominator)
            .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
        Ok(left.cmp(&right))
    }

    fn whole_units_ratio(self, other: Self) -> Result<u128, ArrangementSearchError> {
        let numerator = self
            .numerator
            .checked_mul(other.denominator)
            .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
        let denominator = self
            .denominator
            .checked_mul(other.numerator)
            .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
        if denominator == 0 {
            return Err(ArrangementSearchError::ArithmeticOverflow);
        }
        Ok(numerator / denominator)
    }

    /// Converts this exact duration to seconds for display or weighted scoring.
    pub fn as_seconds(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

/// Exact number of whole bars that fit in a time budget when the score's
/// playback pattern repeats. The achieved duration never exceeds the budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarsForMinutes {
    /// Requested whole minutes.
    pub requested_minutes: u64,
    /// Number of whole bars that fit.
    pub bars: usize,
    /// Exact playback duration of those bars.
    pub achieved: ExactDuration,
}

/// Closest whole-number repeat count for a requested duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepeatResolution {
    /// Requested whole minutes.
    pub requested_minutes: u64,
    /// Number of times to play the score's expanded playback order.
    pub repeats: u64,
    /// Exact duration achieved by that repeat count.
    pub achieved: ExactDuration,
    /// Whether the requested duration was reached exactly.
    pub exact: bool,
}

/// Caller-selected weights for arrangement metrics and time-budget fit.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrangementSearchWeights {
    /// Signed weight for each exact assessment ratio. Negative values minimize
    /// a metric; positive values maximize it.
    pub metrics: BTreeMap<Metric, f64>,
    /// Non-negative penalty per hard-requirement violation.
    pub hard_violation_penalty: f64,
    /// Non-negative penalty per second of distance from the time budget.
    pub duration_penalty_per_second: f64,
}

impl Default for ArrangementSearchWeights {
    fn default() -> Self {
        Self {
            metrics: BTreeMap::new(),
            hard_violation_penalty: 1.0,
            duration_penalty_per_second: 0.0,
        }
    }
}

/// Controls seeded candidate generation and optional duration targeting.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrangementSearchRequest {
    /// Number of distinct arrangements to return.
    pub candidate_count: usize,
    /// Seed that makes sampling and tie ordering repeatable.
    pub seed: u64,
    /// Optional target song length in whole minutes.
    pub target_minutes: Option<u64>,
    /// Caller-owned ranking weights.
    pub weights: ArrangementSearchWeights,
}

/// One composed plan with the measurements used to rank it.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrangementCandidate {
    /// Editable plan that produced this arrangement.
    pub plan: ArrangementPlan,
    /// Composed score used for measurement and hard-requirement checks.
    pub document: Document,
    /// Exact deterministic arrangement metrics.
    pub metrics: AssessmentReport,
    /// Hard-requirement failures, including their score locations.
    pub violations: Vec<Violation>,
    /// Exact duration of the composed score's playback order.
    pub duration: ExactDuration,
    /// Weighted score; higher-ranked candidates have larger values.
    pub score: f64,
}

/// Errors returned by time-budget calculations and arrangement search.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArrangementSearchError {
    /// A document or request does not contain enough valid data to search.
    #[error("invalid arrangement search input: {0}")]
    InvalidInput(String),
    /// The requested number of unique arrangements cannot be composed.
    #[error("requested {requested} unique arrangements, but only {available} were found")]
    InsufficientCandidates {
        /// Requested distinct candidate count.
        requested: usize,
        /// Number of distinct candidates that could be composed.
        available: usize,
    },
    /// Exact rational duration arithmetic exceeded its supported range.
    #[error("exact duration arithmetic overflowed its supported range")]
    ArithmeticOverflow,
    /// Arrangement composition rejected a candidate plan.
    #[error(transparent)]
    Arrangement(#[from] ArrangementError),
}

/// Returns one bar's duration as an exact rational number of seconds.
///
/// The meter contributes an exact quarter-note fraction; the finite `f64`
/// tempo is treated as its exact IEEE-754 rational value. Unsupported extreme
/// tempos return [`ArrangementSearchError::ArithmeticOverflow`].
///
/// # Errors
///
/// Returns [`ArrangementSearchError::InvalidInput`] if the meter has a zero part or `bpm` is
/// not positive and finite, and [`ArrangementSearchError::ArithmeticOverflow`] if the
/// exact arithmetic exceeds its range.
pub fn bar_duration(meter: (u32, u32), bpm: f64) -> Result<ExactDuration, ArrangementSearchError> {
    if meter.0 == 0 || meter.1 == 0 || !bpm.is_finite() || bpm <= 0.0 {
        return Err(ArrangementSearchError::InvalidInput(
            "meter and tempo must be positive and finite".into(),
        ));
    }
    let (tempo_numerator, tempo_denominator) = exact_positive_float(bpm)?;
    let numerator = u128::from(meter.0)
        .checked_mul(240)
        .and_then(|value| value.checked_mul(tempo_denominator))
        .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
    let denominator = u128::from(meter.1)
        .checked_mul(tempo_numerator)
        .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
    Ok(ExactDuration::new(numerator, denominator))
}

/// Returns the duration of a bar at its score position using the effective
/// tempo map entry, or the requirements module's documented default tempo.
///
/// # Errors
///
/// Returns [`ArrangementSearchError::InvalidInput`] if `bar_index` is outside the score, and
/// any error from [`bar_duration`] for the bar's meter and tempo.
pub fn document_bar_duration(
    document: &Document,
    bar_index: usize,
) -> Result<ExactDuration, ArrangementSearchError> {
    let bar = document.master_bars.get(bar_index).ok_or_else(|| {
        ArrangementSearchError::InvalidInput(format!("bar index {bar_index} is outside the score"))
    })?;
    let bpm = tempo_at(&document.tempo_map, bar_index).unwrap_or(DEFAULT_BPM);
    bar_duration(bar.time, bpm)
}

/// Counts how many whole bars from repeated playback fit a time budget.
///
/// # Errors
///
/// Returns [`ArrangementSearchError::InvalidInput`] if the document has no playable master
/// bar, and [`ArrangementSearchError::ArithmeticOverflow`] if the exact duration
/// arithmetic exceeds its range. Also returns any error from [`document_bar_duration`].
pub fn bars_for_minutes(
    document: &Document,
    minutes: u64,
) -> Result<BarsForMinutes, ArrangementSearchError> {
    let order = inspect::playback_order(document);
    if order.is_empty() {
        return Err(ArrangementSearchError::InvalidInput(
            "time budgeting requires at least one master bar".into(),
        ));
    }
    let cycle = playback_duration(document, &order)?;
    let target = ExactDuration::new(u128::from(minutes) * 60, 1);
    let whole_cycles = target.whole_units_ratio(cycle)?;
    let mut bars = whole_cycles
        .checked_mul(order.len() as u128)
        .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
    let mut achieved = cycle.checked_mul(whole_cycles)?;
    for bar in &order {
        let next = achieved.checked_add(document_bar_duration(document, *bar)?)?;
        if next.checked_cmp(target)? == Ordering::Greater {
            break;
        }
        achieved = next;
        bars = bars
            .checked_add(1)
            .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
    }
    Ok(BarsForMinutes {
        requested_minutes: minutes,
        bars: usize::try_from(bars).map_err(|_| ArrangementSearchError::ArithmeticOverflow)?,
        achieved,
    })
}

/// Chooses the closest positive whole-number repeat count to a time budget.
///
/// The returned achieved duration makes unreachable targets explicit instead
/// of rounding or silently changing the score's bar lengths.
///
/// # Errors
///
/// Returns [`ArrangementSearchError::InvalidInput`] if the document has no playable master
/// bar, and [`ArrangementSearchError::ArithmeticOverflow`] if the exact duration
/// arithmetic exceeds its range. Also returns any error from [`document_bar_duration`].
pub fn resolve_repeats(
    document: &Document,
    minutes: u64,
) -> Result<RepeatResolution, ArrangementSearchError> {
    let order = inspect::playback_order(document);
    if order.is_empty() {
        return Err(ArrangementSearchError::InvalidInput(
            "repeat resolution requires at least one master bar".into(),
        ));
    }
    let cycle = playback_duration(document, &order)?;
    let target = ExactDuration::new(u128::from(minutes) * 60, 1);
    let lower = target.whole_units_ratio(cycle)?;
    let lower = lower.max(1);
    let upper = lower
        .checked_add(1)
        .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
    let lower_duration = cycle.checked_mul(lower)?;
    let upper_duration = cycle.checked_mul(upper)?;
    let lower_gap = absolute_difference(lower_duration, target)?;
    let upper_gap = absolute_difference(upper_duration, target)?;
    let (repeats, achieved) = if lower_gap.checked_cmp(upper_gap)? != Ordering::Greater {
        (lower, lower_duration)
    } else {
        (upper, upper_duration)
    };
    Ok(RepeatResolution {
        requested_minutes: minutes,
        repeats: u64::try_from(repeats).map_err(|_| ArrangementSearchError::ArithmeticOverflow)?,
        achieved,
        exact: achieved.checked_cmp(target)? == Ordering::Equal,
    })
}

/// Builds, measures, and ranks deterministic candidate arrangements.
///
/// Each sampled plan chooses one of the caller-listed motifs per section. A
/// repeated call with the same inputs and seed returns the same ordered deck.
///
/// # Errors
///
/// Returns [`ArrangementSearchError::InvalidInput`] if the request's candidate count is
/// outside `1..=128`, its ranking weights are not finite or a penalty is negative, the
/// template is not a version-1 plan with sections, or a section lists no candidates.
/// Returns [`ArrangementSearchError::InsufficientCandidates`] if fewer distinct
/// arrangements than requested could be composed, and
/// [`ArrangementSearchError::ArithmeticOverflow`] if duration arithmetic overflows.
pub fn search_arrangements(
    source: &Document,
    motifs: &[Motif],
    template: &ArrangementPlan,
    request: &ArrangementSearchRequest,
) -> Result<Vec<ArrangementCandidate>, ArrangementSearchError> {
    validate_search_request(request)?;
    if template.version != 1 || template.sections.is_empty() {
        return Err(ArrangementSearchError::InvalidInput(
            "search requires a version-1 plan with at least one section".into(),
        ));
    }
    if template
        .sections
        .iter()
        .any(|section| section.candidates.is_empty())
    {
        return Err(ArrangementSearchError::InvalidInput(
            "every section must list at least one motif candidate".into(),
        ));
    }

    let mut candidates = Vec::new();
    let mut state = request.seed;
    let attempts = request.candidate_count.saturating_mul(128).min(16_384);
    for _ in 0..attempts {
        let mut plan = template.clone();
        state = splitmix64(state);
        plan.seed = state;
        for section in &mut plan.sections {
            state = splitmix64(state);
            let candidate_count = u64::try_from(section.candidates.len())
                .map_err(|_| ArrangementSearchError::ArithmeticOverflow)?;
            let selected_index = usize::try_from(state % candidate_count)
                .map_err(|_| ArrangementSearchError::ArithmeticOverflow)?;
            let selected = section.candidates[selected_index];
            section.candidates = vec![selected];
        }
        let document = match arrangement::compose_plan(source, motifs, &plan) {
            Ok(document) => document,
            Err(ArrangementError::InvalidTransposition { .. })
            | Err(ArrangementError::OverfullBar { .. })
            | Err(ArrangementError::InvalidInput(_)) => continue,
            Err(error) => return Err(error.into()),
        };
        if candidates
            .iter()
            .any(|candidate: &ArrangementCandidate| candidate.document == document)
        {
            continue;
        }
        let metrics = assessment::assess(&document);
        let violations = requirements::check(&document);
        let duration = playback_duration(&document, &inspect::playback_order(&document))?;
        let score = rank_score(&metrics, &violations, duration, request)?;
        candidates.push(ArrangementCandidate {
            plan,
            document,
            metrics,
            violations,
            duration,
            score,
        });
        if candidates.len() == request.candidate_count {
            break;
        }
    }
    if candidates.len() < request.candidate_count {
        return Err(ArrangementSearchError::InsufficientCandidates {
            requested: request.candidate_count,
            available: candidates.len(),
        });
    }
    candidates.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.plan.seed.cmp(&right.plan.seed))
    });
    Ok(candidates)
}

fn validate_search_request(
    request: &ArrangementSearchRequest,
) -> Result<(), ArrangementSearchError> {
    if request.candidate_count == 0 || request.candidate_count > 128 {
        return Err(ArrangementSearchError::InvalidInput(
            "candidate_count must be in 1..=128".into(),
        ));
    }
    if request.weights.hard_violation_penalty < 0.0
        || !request.weights.hard_violation_penalty.is_finite()
        || request.weights.duration_penalty_per_second < 0.0
        || !request.weights.duration_penalty_per_second.is_finite()
        || request
            .weights
            .metrics
            .values()
            .any(|value| !value.is_finite())
    {
        return Err(ArrangementSearchError::InvalidInput(
            "ranking weights must be finite and penalties non-negative".into(),
        ));
    }
    Ok(())
}

fn rank_score(
    report: &AssessmentReport,
    violations: &[Violation],
    duration: ExactDuration,
    request: &ArrangementSearchRequest,
) -> Result<f64, ArrangementSearchError> {
    let mut score = 0.0;
    for (metric, weight) in &request.weights.metrics {
        let value = report.metrics.get(metric).ok_or_else(|| {
            ArrangementSearchError::InvalidInput(format!(
                "assessment report lacks metric {}",
                metric.name()
            ))
        })?;
        score += value.numerator as f64 / value.denominator as f64 * weight;
    }
    score -= violations.len() as f64 * request.weights.hard_violation_penalty;
    if let Some(minutes) = request.target_minutes {
        let seconds = ExactDuration::new(u128::from(minutes) * 60, 1).as_seconds();
        score -=
            (duration.as_seconds() - seconds).abs() * request.weights.duration_penalty_per_second;
    }
    if !score.is_finite() {
        return Err(ArrangementSearchError::InvalidInput(
            "ranking weights produced a non-finite score".into(),
        ));
    }
    Ok(score)
}

fn playback_duration(
    document: &Document,
    order: &[usize],
) -> Result<ExactDuration, ArrangementSearchError> {
    let mut total = ExactDuration::new(0, 1);
    for bar in order {
        total = total.checked_add(document_bar_duration(document, *bar)?)?;
    }
    Ok(total)
}

fn tempo_at(map: &[TempoChange], bar_index: usize) -> Option<f64> {
    map.iter()
        .filter(|change| change.at_bar <= bar_index)
        .max_by_key(|change| change.at_bar)
        .map(|change| change.bpm)
}

fn exact_positive_float(value: f64) -> Result<(u128, u128), ArrangementSearchError> {
    let bits = value.to_bits();
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (significand, exponent) = if exponent_bits == 0 {
        (u128::from(fraction), -1022 - 52)
    } else {
        (
            u128::from((1_u64 << 52) | fraction),
            exponent_bits - 1023 - 52,
        )
    };
    let (numerator, denominator) = if exponent >= 0 {
        let shift =
            u32::try_from(exponent).map_err(|_| ArrangementSearchError::ArithmeticOverflow)?;
        (
            significand
                .checked_shl(shift)
                .ok_or(ArrangementSearchError::ArithmeticOverflow)?,
            1,
        )
    } else {
        let shift =
            u32::try_from(-exponent).map_err(|_| ArrangementSearchError::ArithmeticOverflow)?;
        (
            significand,
            1_u128
                .checked_shl(shift)
                .ok_or(ArrangementSearchError::ArithmeticOverflow)?,
        )
    };
    let divisor = gcd(numerator, denominator);
    Ok((numerator / divisor, denominator / divisor))
}

fn absolute_difference(
    left: ExactDuration,
    right: ExactDuration,
) -> Result<ExactDuration, ArrangementSearchError> {
    let shared = gcd(left.denominator, right.denominator);
    let left_scale = right.denominator / shared;
    let right_scale = left.denominator / shared;
    let left_num = left
        .numerator
        .checked_mul(left_scale)
        .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
    let right_num = right
        .numerator
        .checked_mul(right_scale)
        .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
    let numerator = left_num.abs_diff(right_num);
    let denominator = left
        .denominator
        .checked_mul(left_scale)
        .ok_or(ArrangementSearchError::ArithmeticOverflow)?;
    Ok(ExactDuration::new(numerator, denominator))
}

fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left.max(1)
}

fn splitmix64(state: u64) -> u64 {
    let mut value = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
#[path = "arrangement_search_test.rs"]
mod tests;
