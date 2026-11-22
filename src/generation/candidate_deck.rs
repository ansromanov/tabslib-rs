//! Feature-vector inspection and deterministic candidate-deck paging.

use crate::inspect::{self, BarFeel, TrackKind};
use crate::model::{Document, Technique};

/// Style-neutral summary of a score's audible structure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeatureVector {
    /// Mean sounding note count per sounding beat, capped at four.
    pub density: f64,
    /// Mean MIDI pitch normalized to 0..=1.
    pub register: f64,
    /// Shares of single-note, two-note, and extended fretted attacks.
    pub voicing_mix: [f64; 3],
    /// Share of fretted notes carrying palm mute.
    pub palm_mute_rate: f64,
    /// Share of fretted bars with explicit triplet or swung notation.
    pub time_feel: f64,
}

impl FeatureVector {
    /// Computes score features without file access or caller style data.
    pub fn from_document(document: &Document) -> Self {
        let mut notes = 0usize;
        let mut sounding_beats = 0usize;
        let mut fretted_notes = 0usize;
        let mut palm_muted = 0usize;
        let mut pitch_sum = 0i64;
        let mut positioned_pitches = 0usize;
        let mut voicings = [0usize; 3];
        let mut voicing_beats = 0usize;
        let mut measured_bars = 0usize;
        let mut non_straight_bars = 0usize;

        for (track_index, _) in document.tracks.iter().enumerate() {
            let kind = inspect::classify_track(document, track_index).map(|(kind, _)| kind);
            let is_fretted = kind == Some(TrackKind::Other);
            for (bar_index, master) in document.master_bars.iter().enumerate() {
                let Some(bar_id) = master.bar_ids.get(track_index).copied() else {
                    continue;
                };
                let Some(bar_id) = u32::try_from(bar_id).ok() else {
                    continue;
                };
                let Some(bar) = document.bars.iter().find(|bar| bar.id == bar_id) else {
                    continue;
                };
                if is_fretted {
                    measured_bars += 1;
                    if bar.voices.iter().enumerate().any(|(voice_index, _)| {
                        matches!(
                            inspect::detect_bar_feel(document, bar_index, track_index, voice_index),
                            BarFeel::Triplet | BarFeel::Swung
                        )
                    }) {
                        non_straight_bars += 1;
                    }
                }
                for voice in &bar.voices {
                    for beat in &voice.beats {
                        if beat.notes.is_empty() {
                            continue;
                        }
                        notes += beat.notes.len();
                        sounding_beats += 1;
                        if !is_fretted {
                            continue;
                        }
                        voicing_beats += 1;
                        voicings[match beat.notes.len() {
                            1 => 0,
                            2 => 1,
                            _ => 2,
                        }] += 1;
                        for note in &beat.notes {
                            fretted_notes += 1;
                            if note.techniques.contains(&Technique::PalmMute) {
                                palm_muted += 1;
                            }
                            if let Some(pitch) = note.midi {
                                pitch_sum += i64::from(pitch);
                                positioned_pitches += 1;
                            }
                        }
                    }
                }
            }
        }

        let density = (notes as f64 / sounding_beats.max(1) as f64 / 4.0).clamp(0.0, 1.0);
        let register =
            (pitch_sum as f64 / positioned_pitches.max(1) as f64 / 127.0).clamp(0.0, 1.0);
        let voicing_mix = voicings.map(|count| count as f64 / voicing_beats.max(1) as f64);
        let palm_mute_rate = palm_muted as f64 / fretted_notes.max(1) as f64;
        let time_feel = non_straight_bars as f64 / measured_bars.max(1) as f64;
        Self {
            density,
            register,
            voicing_mix,
            palm_mute_rate,
            time_feel,
        }
    }

    /// Euclidean distance across the normalized density, register, voicing,
    /// palm-mute, and time-feel dimensions. The result is normalized to 0..=1.
    pub fn distance(self, other: Self) -> f64 {
        let mut squared = (self.density - other.density).powi(2)
            + (self.register - other.register).powi(2)
            + (self.palm_mute_rate - other.palm_mute_rate).powi(2)
            + (self.time_feel - other.time_feel).powi(2);
        for index in 0..3 {
            squared += (self.voicing_mix[index] - other.voicing_mix[index]).powi(2);
        }
        (squared / 7.0).sqrt()
    }
}

/// Caller-owned candidate material with a caller-defined archetype label.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckCandidateInput {
    /// Stable candidate identifier.
    pub id: String,
    /// Caller-defined archetype label; the crate does not enumerate styles.
    pub archetype: String,
    /// Candidate score used to compute its feature vector.
    pub document: Document,
}

/// Selected candidate and its computed feature vector.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckEntry {
    /// Stable candidate identifier.
    pub id: String,
    /// Caller-defined archetype label.
    pub archetype: String,
    /// Features measured from the score.
    pub features: FeatureVector,
    /// Selected candidate score.
    pub document: Document,
}

/// Parameters for deterministic max-distance selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeckRequest {
    /// Number of candidates to select.
    pub candidate_count: usize,
    /// Minimum feature-vector distance between every selected pair.
    pub minimum_distance: f64,
    /// Number of entries returned by one page.
    pub page_size: usize,
    /// Seed for deterministic initial choice and tie breaks.
    pub seed: u64,
}

/// Candidate selection or paging input failure.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DeckError {
    /// A count is zero, or the minimum distance is invalid.
    #[error("candidate count and page size must be positive and distance finite/non-negative")]
    InvalidRequest,
    /// Too few candidates satisfy the requested minimum pairwise distance.
    #[error("could select only {available} candidates; {requested} were requested")]
    InsufficientCandidates { requested: usize, available: usize },
}

/// A deterministic deck with forward and backward page navigation.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateDeck {
    entries: Vec<DeckEntry>,
    page_size: usize,
    page: usize,
}

impl CandidateDeck {
    /// All selected candidates in deck order.
    pub fn entries(&self) -> &[DeckEntry] {
        &self.entries
    }

    /// Current page of candidates.
    pub fn current_page(&self) -> &[DeckEntry] {
        let start = self.page * self.page_size;
        &self.entries[start..(start + self.page_size).min(self.entries.len())]
    }

    /// Advances one page, clamping at the final page.
    pub fn next_page(&mut self) -> &[DeckEntry] {
        let final_page = self.entries.len().saturating_sub(1) / self.page_size;
        self.page = (self.page + 1).min(final_page);
        self.current_page()
    }

    /// Goes back one page, clamping at the first page.
    pub fn previous_page(&mut self) -> &[DeckEntry] {
        self.page = self.page.saturating_sub(1);
        self.current_page()
    }
}

/// Builds a seeded, maximally-spaced deck from caller-supplied candidates.
///
/// Archetype labels are opaque strings so callers can provide their own data
/// without adding a style enum or genre vocabulary to the library.
pub fn build_candidate_deck(
    candidates: &[DeckCandidateInput],
    request: DeckRequest,
) -> Result<CandidateDeck, DeckError> {
    if request.candidate_count == 0
        || request.page_size == 0
        || !request.minimum_distance.is_finite()
        || request.minimum_distance < 0.0
    {
        return Err(DeckError::InvalidRequest);
    }
    let features = candidates
        .iter()
        .map(|candidate| FeatureVector::from_document(&candidate.document))
        .collect::<Vec<_>>();
    let mut available = (0..candidates.len()).collect::<Vec<_>>();
    if available.len() < request.candidate_count {
        return Err(DeckError::InsufficientCandidates {
            requested: request.candidate_count,
            available: available.len(),
        });
    }

    let first = available
        .iter()
        .copied()
        .min_by_key(|index| seeded_hash(request.seed, &candidates[*index].id))
        .ok_or(DeckError::InsufficientCandidates {
            requested: request.candidate_count,
            available: 0,
        })?;
    let mut selected = vec![first];
    available.retain(|index| *index != first);
    while selected.len() < request.candidate_count {
        let next = available
            .iter()
            .copied()
            .map(|candidate| {
                let nearest = selected
                    .iter()
                    .map(|selected| features[candidate].distance(features[*selected]))
                    .fold(f64::INFINITY, f64::min);
                (candidate, nearest)
            })
            .filter(|(_, nearest)| *nearest + f64::EPSILON >= request.minimum_distance)
            .max_by(
                |(left_index, left_distance), (right_index, right_distance)| {
                    left_distance.total_cmp(right_distance).then_with(|| {
                        seeded_hash(request.seed, &candidates[*right_index].id)
                            .cmp(&seeded_hash(request.seed, &candidates[*left_index].id))
                    })
                },
            )
            .map(|(candidate, _)| candidate);
        let Some(next) = next else {
            return Err(DeckError::InsufficientCandidates {
                requested: request.candidate_count,
                available: selected.len(),
            });
        };
        selected.push(next);
        available.retain(|index| *index != next);
    }

    let entries = selected
        .into_iter()
        .map(|index| DeckEntry {
            id: candidates[index].id.clone(),
            archetype: candidates[index].archetype.clone(),
            features: features[index],
            document: candidates[index].document.clone(),
        })
        .collect();
    Ok(CandidateDeck {
        entries,
        page_size: request.page_size,
        page: 0,
    })
}

fn seeded_hash(seed: u64, value: &str) -> u64 {
    value
        .bytes()
        .fold(seed ^ 0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
        })
}

#[cfg(test)]
#[path = "candidate_deck_test.rs"]
mod tests;
