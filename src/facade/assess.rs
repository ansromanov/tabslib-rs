//! Deterministic drum assessment and section marker detection.
//!
//! Provides fast, offline assessment of drum parts against reference pitched
//! rhythm tracks and extracts structural sections with authored rehearsal marks.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::generation::drums::improvement::{assess, DrumAssessment};
use crate::generation::drums::{extract_bar_onsets, voices, DrumHit, DrumPattern};
use crate::inspect::{beat_positions, classify_track, TrackKind};
use crate::model::{Bar, Document, Technique};
use crate::percussion::lookup;

/// A detected score section, annotated with its authored marker name if present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkedSection {
    /// Zero-based sequential section identifier.
    pub id: usize,
    /// Author-supplied marker name, or fallback `Section {id + 1}`.
    pub name: String,
    /// First master-bar index (inclusive).
    pub start_bar: usize,
    /// Last master-bar index (inclusive).
    pub end_bar: usize,
}

/// Options for scoping fast drum assessment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssessDrumsOptions {
    /// Explicit percussion track index (defaults to first percussion track).
    pub track: Option<usize>,
    /// Explicit pitched source track index (defaults to first non-percussion track).
    pub source_track: Option<usize>,
    /// Optional section name substring or 1-based index to filter by.
    pub section: Option<String>,
    /// Optional start bar (0-based inclusive).
    pub start_bar: Option<usize>,
    /// Optional end bar (0-based inclusive).
    pub end_bar: Option<usize>,
}

/// Assessment score for one section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FastSectionScore {
    /// Name of the section.
    pub section_name: String,
    /// 0-based inclusive start bar.
    pub start_bar: usize,
    /// 0-based inclusive end bar.
    pub end_bar: usize,
    /// Drum assessment metrics for this section.
    pub assessment: DrumAssessment,
}

/// Result of a fast, deterministic drum assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FastAssessmentResult {
    /// Name of the assessed drum track.
    pub drum_track_name: String,
    /// Name of the reference source track.
    pub source_track_name: String,
    /// Range of bars covered.
    pub bar_range: (usize, usize),
    /// Per-section assessment scores.
    pub sections: Vec<FastSectionScore>,
    /// Overall score across the entire evaluated bar range.
    pub overall: DrumAssessment,
}

/// Errors during drum assessment.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DrumAssessError {
    /// No percussion track found.
    #[error("no percussion track found in document")]
    NoPercussionTrack,
    /// No pitched source track found.
    #[error("no non-percussion source track found in document")]
    NoSourceTrack,
    /// Specified section name not found.
    #[error("section '{0}' not found in document")]
    SectionNotFound(String),
    /// Invalid bar range.
    #[error("bar range {start}..={end} outside document ({total_bars} bars)")]
    InvalidBarRange {
        /// Requested start bar.
        start: usize,
        /// Requested end bar.
        end: usize,
        /// Total bars in document.
        total_bars: usize,
    },
}

/// Applies existing score section markers to detected sections.
pub fn detect_marked_sections(document: &Document, source_track: usize) -> Vec<MarkedSection> {
    let total_bars = document.master_bars.len();
    if total_bars == 0 {
        return Vec::new();
    }

    let raw_ranges = match crate::generation::sections::detect_section_boundaries(document, 0.65) {
        Ok(boundaries) => {
            crate::generation::sections::sections_from_boundaries(total_bars, &boundaries)
                .unwrap_or_else(|_| {
                    vec![crate::generation::sections::SectionRange {
                        start_bar: 0,
                        end_bar: total_bars - 1,
                    }]
                })
        }
        Err(_) => vec![crate::generation::sections::SectionRange {
            start_bar: 0,
            end_bar: total_bars - 1,
        }],
    };

    let _ = source_track;

    raw_ranges
        .into_iter()
        .enumerate()
        .map(|(id, range)| {
            let marker = document
                .master_bars
                .get(range.start_bar)
                .and_then(|mb| mb.section.as_ref())
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string());
            let name = marker.unwrap_or_else(|| format!("Section {}", id + 1));
            MarkedSection {
                id,
                name,
                start_bar: range.start_bar,
                end_bar: range.end_bar,
            }
        })
        .collect()
}

fn bar_for(doc: &Document, bar_index: usize, track_index: usize) -> Option<&Bar> {
    let id = *doc.master_bars.get(bar_index)?.bar_ids.get(track_index)?;
    doc.bars.iter().find(|bar| bar.id == id as u32)
}

fn extract_hits_for_range(
    doc: &Document,
    track: usize,
    start_bar: usize,
    end_bar: usize,
    steps_per_bar: u32,
) -> Vec<DrumHit> {
    let mut hits = Vec::new();
    for (offset, bar_idx) in (start_bar..=end_bar).enumerate() {
        let bar_step = (offset as u32).saturating_mul(steps_per_bar);
        let Some(bar) = bar_for(doc, bar_idx, track) else {
            continue;
        };
        for (voice_idx, voice) in bar.voices.iter().enumerate() {
            let positions = beat_positions(doc, bar_idx, track, voice_idx);
            for pos in positions {
                let Some(beat) = voice.beats.get(pos.beat_index) else {
                    continue;
                };
                for note in &beat.notes {
                    let mapped = note
                        .articulation
                        .and_then(lookup)
                        .or_else(|| note.midi.and_then(lookup));
                    let Some((midi, _)) = mapped else {
                        continue;
                    };
                    let mut hit = DrumHit::new(midi, bar_step.saturating_add(pos.step));
                    hit.accent = note
                        .techniques
                        .iter()
                        .any(|t| matches!(t, Technique::Accent));
                    hit.ghost = note
                        .techniques
                        .iter()
                        .any(|t| matches!(t, Technique::Ghost | Technique::Dead));
                    hits.push(hit);
                }
            }
        }
    }
    hits
}

fn assess_range(
    doc: &Document,
    drum_track: usize,
    source_track: usize,
    start_bar: usize,
    end_bar: usize,
) -> DrumAssessment {
    let steps_per_bar = 16;
    let onsets_by_bar: Vec<_> = (start_bar..=end_bar)
        .map(|bar| extract_bar_onsets(doc, source_track, bar))
        .collect();
    let pattern = DrumPattern::from_onsets(&onsets_by_bar, steps_per_bar);
    let hits = extract_hits_for_range(doc, drum_track, start_bar, end_bar, steps_per_bar);
    assess(&hits, &pattern, voices::ACOUSTIC_BASS_DRUM)
}

/// Performs fast, offline drum track assessment against a reference pitched track.
///
/// # Errors
///
/// Returns [`DrumAssessError::NoPercussionTrack`] or [`DrumAssessError::NoSourceTrack`] if the
/// selected or default track is missing, [`DrumAssessError::SectionNotFound`] if `options.section`
/// matches no section, and [`DrumAssessError::InvalidBarRange`] if the score is empty or the
/// requested bar range is reversed or past the last bar.
pub fn assess_drums_fast(
    document: &Document,
    options: AssessDrumsOptions,
) -> Result<FastAssessmentResult, DrumAssessError> {
    let total_bars = document.master_bars.len();
    if total_bars == 0 {
        return Err(DrumAssessError::InvalidBarRange {
            start: 0,
            end: 0,
            total_bars: 0,
        });
    }

    let drum_track = match options.track {
        Some(t) => {
            if t >= document.tracks.len() {
                return Err(DrumAssessError::NoPercussionTrack);
            }
            t
        }
        None => (0..document.tracks.len())
            .find(|&i| {
                matches!(
                    classify_track(document, i),
                    Some((TrackKind::Percussion, _))
                )
            })
            .ok_or(DrumAssessError::NoPercussionTrack)?,
    };

    let source_track = match options.source_track {
        Some(t) => {
            if t >= document.tracks.len() {
                return Err(DrumAssessError::NoSourceTrack);
            }
            t
        }
        None => (0..document.tracks.len())
            .find(|&i| matches!(classify_track(document, i), Some((TrackKind::Other, _))))
            .ok_or(DrumAssessError::NoSourceTrack)?,
    };

    let all_sections = detect_marked_sections(document, source_track);

    let (scoped_sections, start_bar, end_bar) = if let Some(ref sec_name) = options.section {
        let query = sec_name.trim().to_ascii_lowercase();
        let parsed_num: Option<usize> = query.parse().ok();
        let matched = parsed_num
            .and_then(|num| {
                if num >= 1 && num <= all_sections.len() {
                    Some(&all_sections[num - 1])
                } else {
                    None
                }
            })
            .or_else(|| {
                all_sections
                    .iter()
                    .find(|s| s.name.to_ascii_lowercase().contains(&query))
            })
            .ok_or_else(|| DrumAssessError::SectionNotFound(sec_name.clone()))?;

        (vec![matched.clone()], matched.start_bar, matched.end_bar)
    } else if options.start_bar.is_some() || options.end_bar.is_some() {
        let raw_start = options.start_bar.unwrap_or(0);
        let raw_end = options.end_bar.unwrap_or(total_bars - 1);
        if raw_start >= total_bars || raw_end >= total_bars || raw_start > raw_end {
            return Err(DrumAssessError::InvalidBarRange {
                start: raw_start,
                end: raw_end,
                total_bars,
            });
        }
        let in_range: Vec<MarkedSection> = all_sections
            .iter()
            .filter(|s| s.end_bar >= raw_start && s.start_bar <= raw_end)
            .cloned()
            .collect();
        (in_range, raw_start, raw_end)
    } else {
        (all_sections.clone(), 0, total_bars - 1)
    };

    let per_section = scoped_sections
        .into_iter()
        .map(|sec| {
            let assessment = assess_range(
                document,
                drum_track,
                source_track,
                sec.start_bar,
                sec.end_bar,
            );
            FastSectionScore {
                section_name: sec.name,
                start_bar: sec.start_bar,
                end_bar: sec.end_bar,
                assessment,
            }
        })
        .collect();

    let overall = assess_range(document, drum_track, source_track, start_bar, end_bar);

    Ok(FastAssessmentResult {
        drum_track_name: document.tracks[drum_track].name.clone(),
        source_track_name: document.tracks[source_track].name.clone(),
        bar_range: (start_bar, end_bar),
        sections: per_section,
        overall,
    })
}

#[cfg(test)]
#[path = "assess_test.rs"]
mod tests;
