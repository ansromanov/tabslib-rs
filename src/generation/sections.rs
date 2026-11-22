//! Content-based section boundaries, conservative cleanup proposals, and riff stabs.
//!
//! Detection is read-only. Marker writes are a separate explicit operation, and
//! cleanup helpers return proposals for the caller to review and apply.

use thiserror::Error;

use crate::edits;
use crate::generation::arrangement::{
    fingerprint_bar, score_compatibility, ArrangementError, MotifFingerprint,
};
use crate::generation::drums::{combine_track_onsets, extract_bar_onsets};
use crate::model::Document;

/// A proposed section boundary before `bar_index` and its measured confidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionBoundary {
    /// Master-bar index where the new section starts.
    pub bar_index: usize,
    /// Confidence derived from adjacent-bar dissimilarity, or `1.0` for an existing marker.
    pub confidence: f64,
    /// Adjacent-bar similarity score, when this is a content-detected boundary.
    pub similarity: Option<f64>,
    /// Whether the score already has a marker at this boundary.
    pub already_marked: bool,
}

/// A contiguous, inclusive bar range in the detected section layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionRange {
    /// First master bar in this section.
    pub start_bar: usize,
    /// Last master bar in this section.
    pub end_bar: usize,
}

/// A proposal to merge adjacent sections by removing their separating marker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionMergeProposal {
    /// Boundary to remove if the caller accepts this proposal.
    pub remove_marker_at: usize,
    /// Bar on the left side of the proposed merge.
    pub left_bar: usize,
    /// Bar on the right side of the proposed merge.
    pub right_bar: usize,
    /// Similarity between the bars, used to rank proposals.
    pub similarity: f64,
}

/// A proposal to fold a one-bar section into one of its neighbours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SingleBarFoldProposal {
    /// The only bar in the short section.
    pub section_bar: usize,
    /// A neighbouring bar whose section should absorb it.
    pub into_bar: usize,
    /// Marker to clear if the caller accepts this proposal.
    pub remove_marker_at: usize,
    /// Similarity between the short section and its proposed neighbour.
    pub similarity: f64,
}

/// A caller-labeled marker edit proposed for a detected boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionMarkerProposal {
    /// Master-bar index that would receive a marker.
    pub bar_index: usize,
    /// Caller-selected marker text.
    pub label: String,
}

/// An isolated accented attack in one or more pitched tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiffStab {
    /// Master-bar index containing the attack.
    pub bar_index: usize,
    /// Grid step within the bar.
    pub step: u32,
    /// Lowest MIDI pitch at the attack, when present.
    pub lowest_pitch: Option<i32>,
    /// Number of simultaneous notes represented by the attack.
    pub polyphony: usize,
    /// Names of tracks contributing the attack.
    pub track_names: Vec<String>,
}

/// Finds content boundaries using adjacent-bar fingerprints and preserves authored markers.
pub fn detect_section_boundaries(
    document: &Document,
    threshold: f64,
) -> Result<Vec<SectionBoundary>, SectionAnalysisError> {
    if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
        return Err(SectionAnalysisError::InvalidThreshold);
    }
    let fingerprints = (0..document.master_bars.len())
        .map(|bar| fingerprint_bar(document, bar))
        .collect::<Result<Vec<MotifFingerprint>, ArrangementError>>()?;
    let mut boundaries = Vec::new();
    for index in 1..document.master_bars.len() {
        let marked = document.master_bars[index].section.is_some();
        let similarity = score_compatibility(&fingerprints[index - 1], &fingerprints[index]).score;
        if marked || similarity < threshold {
            boundaries.push(SectionBoundary {
                bar_index: index,
                confidence: if marked {
                    1.0
                } else {
                    (1.0 - similarity).clamp(0.0, 1.0)
                },
                similarity: Some(similarity),
                already_marked: marked,
            });
        }
    }
    Ok(boundaries)
}

/// Converts boundary indexes into inclusive section ranges.
pub fn sections_from_boundaries(
    bar_count: usize,
    boundaries: &[SectionBoundary],
) -> Result<Vec<SectionRange>, SectionAnalysisError> {
    if bar_count == 0 {
        return Ok(Vec::new());
    }
    let mut starts = vec![0];
    for boundary in boundaries {
        if boundary.bar_index == 0 || boundary.bar_index >= bar_count {
            return Err(SectionAnalysisError::InvalidBoundary {
                bar_index: boundary.bar_index,
                bar_count,
            });
        }
        starts.push(boundary.bar_index);
    }
    starts.sort_unstable();
    starts.dedup();
    Ok(starts
        .iter()
        .enumerate()
        .map(|(index, start_bar)| SectionRange {
            start_bar: *start_bar,
            end_bar: starts
                .get(index + 1)
                .copied()
                .unwrap_or(bar_count)
                .saturating_sub(1),
        })
        .collect())
}

/// Proposes merges where adjacent sections are at least `similarity_threshold` similar.
pub fn propose_section_merges(
    document: &Document,
    boundaries: &[SectionBoundary],
    similarity_threshold: f64,
) -> Result<Vec<SectionMergeProposal>, SectionAnalysisError> {
    if !similarity_threshold.is_finite() || !(0.0..=1.0).contains(&similarity_threshold) {
        return Err(SectionAnalysisError::InvalidThreshold);
    }
    let ranges = sections_from_boundaries(document.master_bars.len(), boundaries)?;
    let mut proposals = Vec::new();
    for pair in ranges.windows(2) {
        let left_bar = pair[0].end_bar;
        let right_bar = pair[1].start_bar;
        let left = fingerprint_bar(document, left_bar)?;
        let right = fingerprint_bar(document, right_bar)?;
        let similarity = score_compatibility(&left, &right).score;
        if similarity >= similarity_threshold {
            proposals.push(SectionMergeProposal {
                remove_marker_at: right_bar,
                left_bar,
                right_bar,
                similarity,
            });
        }
    }
    Ok(proposals)
}

/// Proposes folding every one-bar section into its most similar neighbour.
pub fn propose_single_bar_folds(
    document: &Document,
    boundaries: &[SectionBoundary],
) -> Result<Vec<SingleBarFoldProposal>, SectionAnalysisError> {
    let ranges = sections_from_boundaries(document.master_bars.len(), boundaries)?;
    let mut proposals = Vec::new();
    for (index, section) in ranges.iter().enumerate() {
        if section.start_bar != section.end_bar {
            continue;
        }
        let mut candidates = Vec::new();
        if let Some(previous) = index.checked_sub(1).and_then(|prior| ranges.get(prior)) {
            let similarity = score_compatibility(
                &fingerprint_bar(document, section.start_bar)?,
                &fingerprint_bar(document, previous.end_bar)?,
            )
            .score;
            candidates.push((previous.end_bar, section.start_bar, similarity));
        }
        if let Some(next) = ranges.get(index + 1) {
            let similarity = score_compatibility(
                &fingerprint_bar(document, section.start_bar)?,
                &fingerprint_bar(document, next.start_bar)?,
            )
            .score;
            candidates.push((next.start_bar, next.start_bar, similarity));
        }
        let Some((into_bar, remove_marker_at, similarity)) =
            candidates.into_iter().max_by(|left, right| {
                left.2
                    .total_cmp(&right.2)
                    .then_with(|| right.0.cmp(&left.0))
            })
        else {
            continue;
        };
        proposals.push(SingleBarFoldProposal {
            section_bar: section.start_bar,
            into_bar,
            remove_marker_at,
            similarity,
        });
    }
    Ok(proposals)
}

/// Builds caller-labeled marker proposals in boundary order.
pub fn propose_section_markers(
    boundaries: &[SectionBoundary],
    labels: &[String],
) -> Result<Vec<SectionMarkerProposal>, SectionAnalysisError> {
    if boundaries.len() != labels.len() {
        return Err(SectionAnalysisError::MarkerLabelCount {
            boundaries: boundaries.len(),
            labels: labels.len(),
        });
    }
    if labels.iter().any(|label| label.trim().is_empty()) {
        return Err(SectionAnalysisError::EmptyMarkerLabel);
    }
    Ok(boundaries
        .iter()
        .zip(labels)
        .map(|(boundary, label)| SectionMarkerProposal {
            bar_index: boundary.bar_index,
            label: label.clone(),
        })
        .collect())
}

/// Applies marker proposals explicitly, filling empty marker positions only.
pub fn apply_section_marker_proposals(
    document: &mut Document,
    proposals: &[SectionMarkerProposal],
) -> Result<Vec<usize>, SectionAnalysisError> {
    for proposal in proposals {
        if proposal.bar_index >= document.master_bars.len() {
            return Err(SectionAnalysisError::InvalidBoundary {
                bar_index: proposal.bar_index,
                bar_count: document.master_bars.len(),
            });
        }
        if proposal.label.trim().is_empty() {
            return Err(SectionAnalysisError::EmptyMarkerLabel);
        }
    }
    let mut applied = Vec::new();
    for proposal in proposals {
        if document.master_bars[proposal.bar_index].section.is_none() {
            edits::set_section(document, proposal.bar_index, Some(proposal.label.clone()))
                .map_err(|error| SectionAnalysisError::Edit(format!("{error:?}")))?;
            applied.push(proposal.bar_index);
        }
    }
    Ok(applied)
}

/// Finds accented onsets with no other attack within `isolation_steps`.
pub fn detect_riff_stabs(
    document: &Document,
    tracks: &[usize],
    isolation_steps: u32,
) -> Result<Vec<RiffStab>, SectionAnalysisError> {
    if tracks.is_empty() {
        return Err(SectionAnalysisError::NoSourceTracks);
    }
    if tracks.iter().any(|track| *track >= document.tracks.len()) {
        return Err(SectionAnalysisError::InvalidSourceTrack);
    }
    let mut stabs = Vec::new();
    for bar_index in 0..document.master_bars.len() {
        let by_track = tracks
            .iter()
            .map(|track| extract_bar_onsets(document, *track, bar_index))
            .collect::<Vec<_>>();
        let onsets = combine_track_onsets(&by_track);
        for onset in onsets.iter().filter(|onset| onset.accent) {
            let isolated = onsets
                .iter()
                .filter(|other| other.step != onset.step)
                .all(|other| other.step.abs_diff(onset.step) > isolation_steps);
            if isolated {
                stabs.push(RiffStab {
                    bar_index,
                    step: onset.step,
                    lowest_pitch: onset.lowest_pitch,
                    polyphony: onset.polyphony,
                    track_names: onset.track_names.clone(),
                });
            }
        }
    }
    Ok(stabs)
}

/// Errors returned by section and riff-stab analysis.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SectionAnalysisError {
    /// Similarity thresholds must be finite values between zero and one.
    #[error("section similarity threshold must be in 0..=1")]
    InvalidThreshold,
    /// A boundary must begin after the first bar and before the end of the score.
    #[error("boundary at bar {bar_index} is outside a {bar_count}-bar score")]
    InvalidBoundary {
        /// Zero-based bar index of the boundary.
        bar_index: usize,
        /// Number of bars in the score.
        bar_count: usize,
    },
    /// A supplied label count must match the number of boundary proposals.
    #[error("received {labels} labels for {boundaries} section boundaries")]
    MarkerLabelCount {
        /// Number of section boundaries proposed.
        boundaries: usize,
        /// Number of labels supplied.
        labels: usize,
    },
    /// Marker labels cannot be empty.
    #[error("section marker label cannot be empty")]
    EmptyMarkerLabel,
    /// At least one pitched source track is required.
    #[error("riff-stab detection requires at least one source track")]
    NoSourceTracks,
    /// A source track index lies outside the document.
    #[error("riff-stab source track index is outside the document")]
    InvalidSourceTrack,
    /// Existing fingerprinting could not read the score content.
    #[error(transparent)]
    Arrangement(#[from] ArrangementError),
    /// The explicit marker edit could not be applied.
    #[error("section marker edit failed: {0}")]
    Edit(String),
}

#[cfg(test)]
#[path = "sections_test.rs"]
mod tests;
