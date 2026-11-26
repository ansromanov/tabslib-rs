//! Deterministic score evaluation and reproducible run records.
//!
//! The public functions in this module inspect written [`Document`] values only;
//! they do not use a style profile, reference data, or external service.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::model::Document;

/// A precise duration suitable for stable JSON output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactDuration {
    /// Reduced numerator.
    pub numerator: u64,
    /// Reduced positive denominator.
    pub denominator: u64,
}

/// One playability issue found in a pitched track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayabilityViolation {
    /// Zero-based master-bar index.
    pub bar: usize,
    /// Human-readable invariant that was violated.
    pub kind: String,
    /// String or fret involved, when applicable.
    pub value: Option<i32>,
}

/// Playability and pitch-integrity measures for one track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackEvaluation {
    /// Track index.
    pub track: usize,
    /// Number of written notes with a pitch.
    pub pitched_notes: usize,
    /// Number of pitched notes outside the conventional fret range.
    pub out_of_range: usize,
    /// Number of simultaneous notes sharing a string.
    pub string_collisions: usize,
    /// Number of adjacent notes whose pitch does not match its fret and tuning.
    pub pitch_mismatches: usize,
    /// Detailed violations, in score order.
    pub violations: Vec<PlayabilityViolation>,
}

/// Complete deterministic evaluation of a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationReport {
    /// Number of bars in written order.
    pub bars: usize,
    /// Total written duration in whole notes.
    pub duration: ExactDuration,
    /// Pitched-track evaluations.
    pub tracks: Vec<TrackEvaluation>,
    /// Count of written notes by MIDI pitch class.
    pub pitch_classes: BTreeMap<i32, usize>,
}

/// Evaluates all pitched tracks in a document.
pub fn evaluate(doc: &Document) -> EvaluationReport {
    let mut pitch_classes = BTreeMap::new();
    for bar in &doc.bars {
        for voice in &bar.voices {
            for beat in &voice.beats {
                for note in &beat.notes {
                    if let Some(midi) = note.midi {
                        *pitch_classes.entry(midi.rem_euclid(12)).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    let duration = crate::inspect::summary(doc).duration;
    EvaluationReport {
        bars: doc.master_bars.len(),
        duration: ExactDuration {
            numerator: duration.numerator,
            denominator: duration.denominator,
        },
        tracks: doc
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| !track.tuning.is_empty())
            .map(|(index, _)| evaluate_track(doc, index))
            .collect(),
        pitch_classes,
    }
}

/// Evaluates a single pitched track. Invalid indexes produce an empty report.
pub fn evaluate_track(doc: &Document, track: usize) -> TrackEvaluation {
    let mut result = TrackEvaluation {
        track,
        pitched_notes: 0,
        out_of_range: 0,
        string_collisions: 0,
        pitch_mismatches: 0,
        violations: Vec::new(),
    };
    let Some(instrument) = doc.tracks.get(track) else {
        return result;
    };
    for (bar_index, master) in doc.master_bars.iter().enumerate() {
        let Some(bar_id) = master.bar_ids.get(track) else {
            continue;
        };
        let Some(bar_id) = u32::try_from(*bar_id).ok() else {
            continue;
        };
        let Some(bar) = doc.bars.iter().find(|bar| bar.id == bar_id) else {
            continue;
        };
        for voice in &bar.voices {
            for beat in &voice.beats {
                let mut strings = Vec::new();
                for note in &beat.notes {
                    let Some(midi) = note.midi else { continue };
                    result.pitched_notes += 1;
                    if let Some(string) = note.string {
                        if strings.contains(&string) {
                            result.string_collisions += 1;
                        }
                        strings.push(string);
                        if usize::try_from(string)
                            .ok()
                            .is_none_or(|s| s >= instrument.tuning.len())
                        {
                            result.violations.push(PlayabilityViolation {
                                bar: bar_index,
                                kind: "string".into(),
                                value: i32::try_from(string).ok(),
                            });
                        } else if note.fret.is_some_and(|f| !(0..=24).contains(&f)) {
                            result.out_of_range += 1;
                            result.violations.push(PlayabilityViolation {
                                bar: bar_index,
                                kind: "fret_range".into(),
                                value: note.fret,
                            });
                        }
                    }
                    if let (Some(string), Some(fret)) = (note.string, note.fret) {
                        let Some(open) = usize::try_from(string)
                            .ok()
                            .and_then(|index| instrument.tuning.get(index))
                        else {
                            continue;
                        };
                        if open + fret != midi {
                            result.pitch_mismatches += 1;
                        }
                    }
                }
            }
        }
    }
    result
}

/// Hashes arbitrary score or run input bytes with SHA-256.
pub fn content_hash(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

/// An input or output captured by a reproducible run record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunArtifact {
    /// Stable path or logical name.
    pub path: String,
    /// SHA-256 content hash.
    pub content_hash: String,
}

/// Provenance supplied by the caller that executed a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunProvenance {
    /// Application name.
    pub application: String,
    /// Application version or build identifier.
    pub version: String,
    /// Git commit, when known.
    pub git_commit: Option<String>,
    /// Files changed since the calibration or baseline commit.
    pub changed_files: Vec<String>,
}

/// A serializable, deterministic record of one evaluation or generation run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    /// Schema version.
    pub schema_version: u32,
    /// Caller-provided RFC 3339 timestamp.
    pub timestamp: String,
    /// Scenario identifier.
    pub scenario: String,
    /// Inputs consumed by the run.
    pub inputs: Vec<RunArtifact>,
    /// Outputs produced by the run.
    pub outputs: Vec<RunArtifact>,
    /// Measured score facts.
    pub metrics: EvaluationReport,
    /// Execution provenance.
    pub provenance: RunProvenance,
}

impl RunRecord {
    /// Current run-record schema version.
    pub const SCHEMA_VERSION: u32 = 1;

    /// Serializes the record in stable pretty JSON.
    ///
    /// # Errors
    ///
    /// Returns the `serde_json::Error` if the record cannot be serialized to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Returns whether a calibration is stale relative to a current commit.
pub fn calibration_is_stale(
    calibrated_commit: Option<&str>,
    current_commit: Option<&str>,
    changed_files: &[String],
) -> bool {
    calibrated_commit != current_commit || !changed_files.is_empty()
}

#[cfg(test)]
#[path = "evaluation_test.rs"]
mod tests;
