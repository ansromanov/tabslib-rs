//! Blind preference records and a calibration gate that can fail.
//!
//! The module owns [`PreferenceRecord`], [`gate`] and [`discrimination`].
//!
//! [`calibrate`](super::assessment::calibrate) compares an assessment against
//! caller-supplied samples and returns values. Values cannot fail a check, so
//! nothing here could ever say a candidate was not good enough. [`gate`] adds
//! that verdict, with the threshold supplied by the caller.
//!
//! A record is the other half: what a listener actually preferred, logged
//! blind so the record is about the music rather than about which side the
//! reader knew was generated.
//!
//! Nothing in this module reads a clock, a file, or reference data. The caller
//! supplies the time, the storage and the thresholds, because all three are
//! decisions about a particular collection of music.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::assessment::{AssessmentReport, Measurement, Metric};

/// Schema version written into every record.
pub const RECORD_VERSION: u32 = 1;

/// Which side of a blind pair a candidate was shown on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    /// The side presented first.
    First,
    /// The side presented second.
    Second,
}

/// What a listener chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Choice {
    /// The candidate shown first.
    First,
    /// The candidate shown second.
    Second,
    /// Both were acceptable and neither was better.
    NoPreference,
    /// Neither was acceptable.
    BothBad,
    /// The optional third candidate in a legacy A/B/C capture was preferred.
    Third,
}

impl Choice {
    /// Whether this choice separates the two candidates.
    ///
    /// A no-preference and a both-bad are real answers about the pair and
    /// useless for ranking one candidate above the other, so a ranking built
    /// from a log has to skip them rather than count them as a tie for one
    /// side.
    pub fn is_decisive(self) -> bool {
        matches!(self, Self::First | Self::Second | Self::Third)
    }
}

/// What produced one candidate, as the caller described it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateRef {
    /// Caller-owned identifier.
    pub label: String,
    /// Seed the candidate was generated from.
    pub seed: u64,
    /// Caller-owned description of the configuration behind it.
    pub context: String,
}

/// Provenance retained when a preference was imported from an external log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreferenceProvenance {
    /// Timestamp written by the capture tool.
    pub timestamp: Option<String>,
    /// Dimension being judged, when supplied by the capture tool.
    pub dimension: Option<String>,
    /// Source score path and content hash.
    pub source_path: Option<String>,
    /// SHA-256 hash of the source score.
    pub source_content_hash: Option<String>,
    /// Directory containing the blinded session artifacts.
    pub session_dir: Option<String>,
    /// Free-form listening rationale.
    pub reason: Option<String>,
}

/// One blind paired comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreferenceRecord {
    /// Schema version.
    pub version: u32,
    /// Stable identifier for this record.
    pub id: String,
    /// Seed the side assignment was derived from.
    pub seed: u64,
    /// The candidate assigned to the first side.
    pub first: CandidateRef,
    /// The candidate assigned to the second side.
    pub second: CandidateRef,
    /// What the listener chose.
    pub choice: Choice,
    /// Optional third candidate retained from a legacy A/B/C capture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub third: Option<CandidateRef>,
    /// Capture provenance, absent for records created by the native API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<PreferenceProvenance>,
}

impl PreferenceRecord {
    /// The candidate the listener preferred, if the choice was decisive.
    pub fn preferred(&self) -> Option<&CandidateRef> {
        match self.choice {
            Choice::First => Some(&self.first),
            Choice::Second => Some(&self.second),
            Choice::Third => self.third.as_ref(),
            Choice::NoPreference | Choice::BothBad => None,
        }
    }
}

/// Which side a candidate lands on, derived from the seed.
///
/// The assignment is deterministic so a record can be replayed, and it is
/// derived rather than chosen so the person logging the record does not decide
/// which candidate they are about to hear first.
pub fn assign_side(seed: u64) -> Side {
    // Fold the whole seed rather than reading one bit: a caller incrementing a
    // seed by one per comparison would otherwise alternate sides exactly, and
    // a reader who noticed would no longer be blind.
    let mut mixed = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    mixed ^= mixed >> 29;
    if mixed.count_ones().is_multiple_of(2) {
        Side::First
    } else {
        Side::Second
    }
}

/// Builds a record with the sides assigned blind from the seed.
///
/// `clock` is whatever the caller counts time in; nothing here reads a clock,
/// because a library that stamps its own time cannot be replayed.
pub fn blind_record(
    clock: u64,
    seed: u64,
    left: CandidateRef,
    right: CandidateRef,
    choice: Choice,
) -> PreferenceRecord {
    let (first, second) = match assign_side(seed) {
        Side::First => (left, right),
        Side::Second => (right, left),
    };
    PreferenceRecord {
        version: RECORD_VERSION,
        id: format!("{clock:016x}-{seed:016x}"),
        seed,
        first,
        second,
        choice,
        third: None,
        provenance: None,
    }
}

/// Why a record log could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PreferenceError {
    /// A record could not be serialized or parsed.
    #[error("preference record serialization failed: {0}")]
    Serialization(String),
    /// A record was written by a different schema version.
    #[error("unsupported preference record version {0}")]
    UnsupportedVersion(u32),
}

/// Appends a record to a log, one JSON object per line.
///
/// The caller owns the storage. This returns the new log text rather than
/// writing anywhere, because where a log lives is not a decision this crate
/// makes.
pub fn append_record(log: &str, record: &PreferenceRecord) -> Result<String, PreferenceError> {
    let line = serde_json::to_string(record)
        .map_err(|error| PreferenceError::Serialization(error.to_string()))?;
    let mut out = String::from(log);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&line);
    out.push('\n');
    Ok(out)
}

/// Parses every record in a log, rejecting one from another schema version.
pub fn read_records(log: &str) -> Result<Vec<PreferenceRecord>, PreferenceError> {
    let mut out = Vec::new();
    for line in log.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value = serde_json::from_str(line)
            .map_err(|error| PreferenceError::Serialization(error.to_string()))?;
        if value.get("schemaVersion").is_some() {
            out.push(import_legacy_record(&value)?);
        } else {
            let record: PreferenceRecord = serde_json::from_value(value)
                .map_err(|error| PreferenceError::Serialization(error.to_string()))?;
            if record.version != RECORD_VERSION {
                return Err(PreferenceError::UnsupportedVersion(record.version));
            }
            out.push(record);
        }
    }
    Ok(out)
}

fn import_legacy_record(value: &serde_json::Value) -> Result<PreferenceRecord, PreferenceError> {
    let candidates = value
        .get("candidates")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| PreferenceError::Serialization("legacy record has no candidates".into()))?;
    let candidate = |side: &str| -> Result<CandidateRef, PreferenceError> {
        let value = candidates.get(side).ok_or_else(|| {
            PreferenceError::Serialization(format!("legacy record has no candidate {side}"))
        })?;
        let label = value
            .get("side")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(side)
            .to_string();
        let seed = value
            .get("config")
            .and_then(|config| config.get("seed"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let context = serde_json::to_string(value).map_err(|error| {
            PreferenceError::Serialization(format!(
                "legacy candidate serialization failed: {error}"
            ))
        })?;
        Ok(CandidateRef {
            label,
            seed,
            context,
        })
    };
    let choice = match value.get("choice").and_then(serde_json::Value::as_str) {
        Some("a") => Choice::First,
        Some("b") => Choice::Second,
        Some("c") => Choice::Third,
        Some("no-preference") => Choice::NoPreference,
        Some("both-bad") => Choice::BothBad,
        Some(other) => {
            return Err(PreferenceError::Serialization(format!(
                "unknown legacy choice {other}"
            )))
        }
        None => {
            return Err(PreferenceError::Serialization(
                "legacy record has no choice".into(),
            ))
        }
    };
    let source = value.get("source");
    Ok(PreferenceRecord {
        version: RECORD_VERSION,
        id: value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("legacy")
            .to_string(),
        seed: value
            .get("candidates")
            .and_then(|candidates| candidates.get("a"))
            .and_then(|candidate| candidate.get("config"))
            .and_then(|config| config.get("seed"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        first: candidate("a")?,
        second: candidate("b")?,
        choice,
        third: candidates.get("c").map(|_| candidate("c")).transpose()?,
        provenance: Some(PreferenceProvenance {
            timestamp: value
                .get("timestamp")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            dimension: value
                .get("dimension")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            source_path: source
                .and_then(|source| source.get("path"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            source_content_hash: source
                .and_then(|source| source.get("contentHash"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            session_dir: value
                .get("sessionDir")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            reason: value
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        }),
    })
}

/// How many decisive comparisons each candidate label won and lost.
pub fn tally(records: &[PreferenceRecord]) -> BTreeMap<String, (usize, usize)> {
    let mut out: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for record in records {
        if !record.choice.is_decisive() {
            continue;
        }
        let (won, lost): (&CandidateRef, Vec<&CandidateRef>) = match record.choice {
            Choice::First => (&record.first, vec![&record.second]),
            Choice::Second => (&record.second, vec![&record.first]),
            Choice::Third => {
                let Some(third) = record.third.as_ref() else {
                    continue;
                };
                (third, vec![&record.first, &record.second])
            }
            Choice::NoPreference | Choice::BothBad => continue,
        };
        out.entry(won.label.clone()).or_default().0 += lost.len();
        for candidate in lost {
            out.entry(candidate.label.clone()).or_default().1 += 1;
        }
    }
    out
}

/// What a gate decided about one metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    /// The observed value is within tolerance of every reference.
    Pass,
    /// The observed value is outside tolerance.
    Fail,
    /// The metric was excluded by the caller, so its failure does not count.
    Quarantined,
    /// No reference was supplied, so there was nothing to compare against.
    Unmeasured,
}

/// Caller-supplied thresholds for a gate.
///
/// No default tolerance is offered. A tolerance that makes a metric meaningful
/// is measured over a collection of music, and guessing one here would turn a
/// gate into a coin flip that looks authoritative.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatePolicy {
    /// Permitted distance from the nearest reference, as a fraction of that
    /// reference, keyed by metric.
    pub tolerance: BTreeMap<Metric, Measurement>,
    /// Metrics whose failure is reported but does not fail the gate.
    pub quarantined: BTreeSet<Metric>,
}

/// One metric's gate result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricVerdict {
    /// Metric being gated.
    pub metric: Metric,
    /// What the gate decided.
    pub verdict: Verdict,
    /// Value measured from the document.
    pub observed: Measurement,
    /// Nearest reference the observation was compared against.
    pub nearest: Option<(String, Measurement)>,
}

/// The gate's result over every metric.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateReport {
    /// Per-metric verdicts in stable metric order.
    pub verdicts: Vec<MetricVerdict>,
}

impl GateReport {
    /// Whether the gate passes: no metric failed outside quarantine.
    pub fn passed(&self) -> bool {
        self.verdicts
            .iter()
            .all(|entry| entry.verdict != Verdict::Fail)
    }

    /// Metrics that failed outside quarantine.
    pub fn failures(&self) -> Vec<Metric> {
        self.verdicts
            .iter()
            .filter(|entry| entry.verdict == Verdict::Fail)
            .map(|entry| entry.metric)
            .collect()
    }

    /// A short report a check can print.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for entry in &self.verdicts {
            let verdict = match entry.verdict {
                Verdict::Pass => "pass",
                Verdict::Fail => "FAIL",
                Verdict::Quarantined => "quarantined",
                Verdict::Unmeasured => "unmeasured",
            };
            out.push_str(&format!(
                "{verdict:>12}  {:<22} {}/{}\n",
                entry.metric.name(),
                entry.observed.numerator,
                entry.observed.denominator
            ));
        }
        out
    }
}

/// Distance between two measurements, as a fraction of the reference.
fn relative_distance(observed: Measurement, reference: Measurement) -> Measurement {
    // |o/od - r/rd| / (r/rd) = |o*rd - r*od| / (r * od)
    let left = u128::from(observed.numerator) * u128::from(reference.denominator);
    let right = u128::from(reference.numerator) * u128::from(observed.denominator);
    let numerator = left.abs_diff(right);
    let denominator = u128::from(reference.numerator) * u128::from(observed.denominator);
    if denominator == 0 {
        // A zero reference has no proportional distance, so any difference is
        // total and any match is exact.
        return Measurement::new(u64::from(numerator != 0), 1);
    }
    let scale = numerator.max(denominator) / u128::from(u64::MAX).max(1) + 1;
    Measurement::new(
        u64::try_from(numerator / scale).unwrap_or(u64::MAX),
        u64::try_from(denominator / scale).unwrap_or(1),
    )
}

/// Turns a calibration comparison into pass, fail or quarantine verdicts.
///
/// A metric with no reference is `Unmeasured` rather than `Pass`: a gate that
/// counts a missing comparison as a success reports green for a report that
/// measured nothing at all.
pub fn gate(
    report: &AssessmentReport,
    samples: &[super::assessment::CalibrationSample],
    policy: &GatePolicy,
) -> GateReport {
    let calibration = super::assessment::calibrate(report, samples);
    let verdicts = calibration
        .values
        .iter()
        .map(|value| {
            let nearest = value
                .references
                .iter()
                .min_by(|left, right| {
                    relative_distance(value.observed, left.1)
                        .cmp(&relative_distance(value.observed, right.1))
                })
                .cloned();
            let verdict = match (&nearest, policy.tolerance.get(&value.metric)) {
                (None, _) => Verdict::Unmeasured,
                (Some(_), None) => Verdict::Unmeasured,
                (Some((_, reference)), Some(tolerance)) => {
                    if relative_distance(value.observed, *reference) <= *tolerance {
                        Verdict::Pass
                    } else if policy.quarantined.contains(&value.metric) {
                        Verdict::Quarantined
                    } else {
                        Verdict::Fail
                    }
                }
            };
            MetricVerdict {
                metric: value.metric,
                verdict,
                observed: value.observed,
                nearest,
            }
        })
        .collect();
    GateReport { verdicts }
}

/// Whether a metric separates the samples it was given.
///
/// A metric that reads the same for every sample cannot tell them apart, so a
/// gate built on it always agrees with itself and never catches anything. This
/// reports the number of distinct values observed; one means the metric is
/// uninformative over this set.
pub fn discrimination(samples: &[super::assessment::CalibrationSample], metric: Metric) -> usize {
    samples
        .iter()
        .filter_map(|sample| sample.metrics.get(&metric))
        .map(|value| (value.numerator, value.denominator))
        .collect::<BTreeSet<_>>()
        .len()
}

impl PartialOrd for Measurement {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Measurement {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (u128::from(self.numerator) * u128::from(other.denominator))
            .cmp(&(u128::from(other.numerator) * u128::from(self.denominator)))
    }
}

#[cfg(test)]
#[path = "preference_test.rs"]
mod tests;
