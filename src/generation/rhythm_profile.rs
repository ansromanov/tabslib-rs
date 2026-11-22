//! Measures caller-supplied rhythmic vocabulary and applies caller profiles.
//!
//! The module owns rhythm-cell and interval extraction, normalized
//! distributions, cosine similarity, vocabulary blending, and caller-derived
//! form priors. It stores no repertoire or learned profile tables.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Document, Rhythm, Technique};

/// One stable rhythmic cell extracted from a document bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RhythmCell {
    /// Zero-based bar index.
    pub bar: usize,
    /// Stable track/voice and step-grid encoding.
    pub encoding: String,
}

/// A bar and voice omitted by tolerant rhythm extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedRhythm {
    /// Zero-based bar index.
    pub bar: usize,
    /// Track containing the malformed voice.
    pub track: usize,
    /// Voice containing the malformed timing.
    pub voice: usize,
    /// Human-readable reason for omission.
    pub reason: String,
}

/// Rhythm cells together with malformed bars that were skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RhythmExtraction {
    /// Successfully measured bars.
    pub cells: Vec<RhythmCell>,
    /// Malformed bar/voice entries omitted from the measurement.
    pub skipped: Vec<SkippedRhythm>,
}

/// One caller-selected phrase grouping of rhythmic cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RhythmPhrase {
    /// First bar index in the phrase.
    pub start_bar: usize,
    /// Stable concatenation of the phrase's bar cells.
    pub encoding: String,
}

/// Normalized rhythm-cell, interval, and caller-supplied form distributions.
#[derive(Debug, Clone, PartialEq)]
pub struct RhythmicVocabulary {
    /// Relative frequency of each encoded rhythm cell.
    pub cells: BTreeMap<String, f64>,
    /// Relative frequency of each encoded melodic interval n-gram.
    pub intervals: BTreeMap<String, f64>,
    /// Relative frequency of each caller-supplied section-role sequence.
    pub forms: BTreeMap<String, f64>,
}

/// A caller-supplied profile for changing selected riff specification fields.
#[cfg(feature = "riffs")]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CallerRiffProfile {
    /// Replacement rhythmic archetype.
    pub archetype: Option<super::riff_spec::RiffArchetype>,
    /// Replacement register.
    pub register: Option<super::riff_spec::Register>,
    /// Replacement voicing.
    pub voicing: Option<super::riff_spec::Voicing>,
    /// Replacement degrees per bar.
    pub degrees: Option<Vec<u8>>,
    /// Replacement tempo in beats per minute.
    pub tempo: Option<f64>,
    /// Replacement palm-mute flag.
    pub palm_mute: Option<bool>,
}

/// Field names in an applied riff profile.
#[cfg(feature = "riffs")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiffProfileField {
    /// Rhythmic archetype.
    Archetype,
    /// Register.
    Register,
    /// Voicing.
    Voicing,
    /// Scale degrees.
    Degrees,
    /// Tempo.
    Tempo,
    /// Palm-mute flag.
    PalmMute,
}

/// Typed before and after values for one changed specification field.
#[cfg(feature = "riffs")]
#[derive(Debug, Clone, PartialEq)]
pub enum RiffProfileValue {
    /// Rhythmic archetype value.
    Archetype(super::riff_spec::RiffArchetype),
    /// Register value.
    Register(super::riff_spec::Register),
    /// Voicing value.
    Voicing(super::riff_spec::Voicing),
    /// Scale-degree sequence.
    Degrees(Vec<u8>),
    /// Tempo in beats per minute.
    Tempo(f64),
    /// Palm-mute value.
    PalmMute(bool),
}

/// One field changed by applying a caller profile.
#[cfg(feature = "riffs")]
#[derive(Debug, Clone, PartialEq)]
pub struct RiffProfileChange {
    /// Specification field that changed.
    pub field: RiffProfileField,
    /// Previous value.
    pub before: RiffProfileValue,
    /// Value written from the profile.
    pub after: RiffProfileValue,
}

/// Why caller-supplied rhythmic data could not be measured or applied.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RhythmProfileError {
    /// The step grid must have a positive subdivision count.
    #[error("steps per quarter must be positive")]
    InvalidGrid,
    /// A selected track index does not exist in a document.
    #[error("track index {track} is outside document with {tracks} tracks")]
    MissingTrack {
        /// Requested zero-based track index.
        track: usize,
        /// Number of tracks in the document.
        tracks: usize,
    },
    /// A document bar has no corresponding track bar.
    #[error("bar {bar} has no data for track {track}")]
    MissingBar {
        /// Zero-based bar index.
        bar: usize,
        /// Zero-based track index.
        track: usize,
    },
    /// The written duration does not land exactly on the requested grid.
    #[error("bar {bar}, track {track}, voice {voice} contains an off-grid duration")]
    OffGrid {
        /// Zero-based bar index.
        bar: usize,
        /// Zero-based track index.
        track: usize,
        /// Zero-based voice index.
        voice: usize,
    },
    /// The encoded material does not fill its time signature.
    #[error("bar {bar}, track {track}, voice {voice} does not fill the bar")]
    IncompleteBar {
        /// Zero-based bar index.
        bar: usize,
        /// Zero-based track index.
        track: usize,
        /// Zero-based voice index.
        voice: usize,
    },
    /// A document voice runs past its meter.
    #[error("bar {bar}, track {track}, voice {voice} exceeds its meter")]
    OverfullBar {
        /// Zero-based bar index.
        bar: usize,
        /// Track containing the malformed voice.
        track: usize,
        /// Voice containing the malformed timing.
        voice: usize,
    },
    /// The requested phrase length or n-gram size is zero.
    #[error("phrase length and interval n-gram size must be positive")]
    EmptyWindow,
    /// A requested distribution has no observations.
    #[error("cannot build a distribution without observations")]
    EmptyDistribution,
    /// Caller-supplied blend weight is not finite or in 0..=1.
    #[error("blend weight must be finite and in 0..=1")]
    InvalidBlendWeight,
    /// A caller-supplied section role is empty.
    #[error("section-role sequences must not contain empty labels")]
    EmptyRole,
    /// Applying the profile would create an invalid riff specification.
    #[cfg(feature = "riffs")]
    #[error("profile produced an invalid riff specification: {0}")]
    InvalidRiffSpec(#[from] super::riff_spec::RiffSpecError),
}

/// Extracts one stable attack/rest/palm-mute cell for each bar.
///
/// `steps_per_quarter` controls resolution. Each voice is kept independent in
/// the encoding so simultaneous parts never collapse into an ambiguous cell.
pub fn extract_rhythm_cells(
    document: &Document,
    tracks: &[usize],
    steps_per_quarter: u32,
) -> Result<Vec<RhythmCell>, RhythmProfileError> {
    validate_tracks(document, tracks, steps_per_quarter)?;
    let mut cells = Vec::with_capacity(document.master_bars.len());
    for bar_index in 0..document.master_bars.len() {
        cells.push(extract_one_rhythm_cell(
            document,
            tracks,
            steps_per_quarter,
            bar_index,
        )?);
    }
    Ok(cells)
}

fn extract_one_rhythm_cell(
    document: &Document,
    tracks: &[usize],
    steps_per_quarter: u32,
    bar_index: usize,
) -> Result<RhythmCell, RhythmProfileError> {
    let master = document
        .master_bars
        .get(bar_index)
        .ok_or(RhythmProfileError::InvalidGrid)?;
    let bar_ticks =
        ticks_per_bar(master.time, steps_per_quarter).ok_or(RhythmProfileError::InvalidGrid)?;
    let mut voices = Vec::new();
    for track_index in tracks {
        let bar_id = *master
            .bar_ids
            .get(*track_index)
            .ok_or(RhythmProfileError::MissingBar {
                bar: bar_index,
                track: *track_index,
            })?;
        let bar = document
            .bars
            .iter()
            .find(|bar| i64::from(bar.id) == i64::from(bar_id))
            .ok_or(RhythmProfileError::MissingBar {
                bar: bar_index,
                track: *track_index,
            })?;
        for (voice_index, voice) in bar.voices.iter().enumerate() {
            let mut ticks = vec![b'-'; bar_ticks];
            let mut cursor = 0usize;
            for beat in &voice.beats {
                let steps = rhythm_steps(beat.rhythm, steps_per_quarter).ok_or(
                    RhythmProfileError::OffGrid {
                        bar: bar_index,
                        track: *track_index,
                        voice: voice_index,
                    },
                )?;
                if !beat.notes.is_empty() && cursor < ticks.len() {
                    let palm_muted = beat.notes.iter().any(|note| {
                        note.techniques
                            .iter()
                            .any(|technique| matches!(technique, Technique::PalmMute))
                    });
                    ticks[cursor] = if palm_muted { b'p' } else { b'x' };
                }
                cursor = cursor.saturating_add(steps);
            }
            if cursor < bar_ticks {
                return Err(RhythmProfileError::IncompleteBar {
                    bar: bar_index,
                    track: *track_index,
                    voice: voice_index,
                });
            }
            if cursor > bar_ticks {
                return Err(RhythmProfileError::OverfullBar {
                    bar: bar_index,
                    track: *track_index,
                    voice: voice_index,
                });
            }
            let pattern = String::from_utf8(ticks).map_err(|_| RhythmProfileError::InvalidGrid)?;
            voices.push(format!("t{track_index}v{voice_index}:{pattern}"));
        }
    }
    Ok(RhythmCell {
        bar: bar_index,
        encoding: voices.join("|"),
    })
}

/// Chooses an exact grid capable of representing every written duration.
pub fn recommended_steps_per_quarter(document: &Document) -> Result<u32, RhythmProfileError> {
    let mut grid = 1u128;
    for master in &document.master_bars {
        grid = lcm_grid(
            grid,
            u128::from(master.time.1),
            u128::from(master.time.0).saturating_mul(4),
        )?;
        for bar_id in &master.bar_ids {
            let Some(id) = u32::try_from(*bar_id).ok() else {
                continue;
            };
            let Some(bar) = document.bars.iter().find(|bar| bar.id == id) else {
                continue;
            };
            for voice in &bar.voices {
                for beat in &voice.beats {
                    let (numerator, denominator) = beat.rhythm.as_fraction();
                    grid = lcm_grid(
                        grid,
                        u128::from(denominator),
                        u128::from(numerator).saturating_mul(4),
                    )?;
                }
            }
        }
    }
    u32::try_from(grid).map_err(|_| RhythmProfileError::InvalidGrid)
}

/// Extracts all measurable cells while recording malformed bars instead of aborting.
pub fn extract_rhythm_cells_tolerant(
    document: &Document,
    tracks: &[usize],
    steps_per_quarter: u32,
) -> Result<RhythmExtraction, RhythmProfileError> {
    validate_tracks(document, tracks, steps_per_quarter)?;
    let mut cells = Vec::new();
    let mut skipped = Vec::new();
    for bar in 0..document.master_bars.len() {
        match extract_one_rhythm_cell(document, tracks, steps_per_quarter, bar) {
            Ok(cell) => cells.push(cell),
            Err(error) => match error {
                RhythmProfileError::OffGrid { bar, track, voice }
                | RhythmProfileError::IncompleteBar { bar, track, voice }
                | RhythmProfileError::OverfullBar { bar, track, voice } => {
                    skipped.push(SkippedRhythm {
                        bar,
                        track,
                        voice,
                        reason: error.to_string(),
                    })
                }
                other => return Err(other),
            },
        }
    }
    Ok(RhythmExtraction { cells, skipped })
}

/// Groups bar cells into fixed-length phrases with stable ordering.
pub fn extract_rhythm_phrases(
    cells: &[RhythmCell],
    bars_per_phrase: usize,
) -> Result<Vec<RhythmPhrase>, RhythmProfileError> {
    if bars_per_phrase == 0 {
        return Err(RhythmProfileError::EmptyWindow);
    }
    Ok(cells
        .chunks(bars_per_phrase)
        .map(|chunk| RhythmPhrase {
            start_bar: chunk.first().map_or(0, |cell| cell.bar),
            encoding: chunk
                .iter()
                .map(|cell| format!("{}={}", cell.bar, cell.encoding))
                .collect::<Vec<_>>()
                .join("||"),
        })
        .collect())
}

/// Extracts successive pitched-onset interval n-grams on the requested grid.
pub fn extract_interval_ngrams(
    document: &Document,
    tracks: &[usize],
    steps_per_quarter: u32,
    n: usize,
) -> Result<Vec<Vec<i32>>, RhythmProfileError> {
    validate_tracks(document, tracks, steps_per_quarter)?;
    if n == 0 {
        return Err(RhythmProfileError::EmptyWindow);
    }
    let mut absolute_start = 0u64;
    let mut onsets: BTreeMap<u64, BTreeSet<i32>> = BTreeMap::new();
    for (bar_index, master) in document.master_bars.iter().enumerate() {
        let bar_ticks =
            ticks_per_bar(master.time, steps_per_quarter).ok_or(RhythmProfileError::InvalidGrid)?;
        for track_index in tracks {
            let bar_id =
                *master
                    .bar_ids
                    .get(*track_index)
                    .ok_or(RhythmProfileError::MissingBar {
                        bar: bar_index,
                        track: *track_index,
                    })?;
            let bar = document
                .bars
                .iter()
                .find(|bar| i64::from(bar.id) == i64::from(bar_id))
                .ok_or(RhythmProfileError::MissingBar {
                    bar: bar_index,
                    track: *track_index,
                })?;
            for (voice_index, voice) in bar.voices.iter().enumerate() {
                let mut cursor = 0u64;
                for beat in &voice.beats {
                    let pitches = beat
                        .notes
                        .iter()
                        .filter_map(|note| note.midi)
                        .collect::<BTreeSet<_>>();
                    if !pitches.is_empty() {
                        let onset = absolute_start.saturating_add(cursor);
                        onsets.entry(onset).or_default().extend(pitches);
                    }
                    let steps = rhythm_steps(beat.rhythm, steps_per_quarter).ok_or(
                        RhythmProfileError::OffGrid {
                            bar: bar_index,
                            track: *track_index,
                            voice: voice_index,
                        },
                    )?;
                    cursor = cursor.saturating_add(
                        u64::try_from(steps).map_err(|_| RhythmProfileError::InvalidGrid)?,
                    );
                }
                let bar_ticks_u64 =
                    u64::try_from(bar_ticks).map_err(|_| RhythmProfileError::InvalidGrid)?;
                if cursor != bar_ticks_u64 {
                    return Err(RhythmProfileError::IncompleteBar {
                        bar: bar_index,
                        track: *track_index,
                        voice: voice_index,
                    });
                }
            }
        }
        absolute_start = absolute_start
            .saturating_add(u64::try_from(bar_ticks).map_err(|_| RhythmProfileError::InvalidGrid)?);
    }
    let pitches = onsets
        .values()
        .filter_map(|values| values.first().copied())
        .collect::<Vec<_>>();
    let intervals = pitches
        .windows(2)
        .map(|window| window[1] - window[0])
        .collect::<Vec<_>>();
    Ok(intervals.windows(n).map(|window| window.to_vec()).collect())
}

/// Builds normalized cell, interval, and caller-supplied form distributions.
pub fn build_rhythmic_vocabulary(
    documents: &[Document],
    tracks: &[usize],
    steps_per_quarter: u32,
    interval_ngram_size: usize,
    section_role_sequences: &[Vec<String>],
) -> Result<RhythmicVocabulary, RhythmProfileError> {
    if documents.is_empty() {
        return Err(RhythmProfileError::EmptyDistribution);
    }
    let mut cells = BTreeMap::new();
    let mut intervals = BTreeMap::new();
    for document in documents {
        for cell in extract_rhythm_cells(document, tracks, steps_per_quarter)? {
            *cells.entry(cell.encoding).or_insert(0u64) += 1;
        }
        for ngram in
            extract_interval_ngrams(document, tracks, steps_per_quarter, interval_ngram_size)?
        {
            *intervals.entry(encode_intervals(&ngram)).or_insert(0u64) += 1;
        }
    }
    let forms = form_prior(section_role_sequences)?;
    Ok(RhythmicVocabulary {
        cells: normalize_counts(cells),
        intervals: normalize_counts(intervals),
        forms,
    })
}

/// Builds a normalized distribution over caller-supplied section-role sequences.
pub fn form_prior(
    section_role_sequences: &[Vec<String>],
) -> Result<BTreeMap<String, f64>, RhythmProfileError> {
    let mut counts = BTreeMap::new();
    for sequence in section_role_sequences {
        if sequence.iter().any(|role| role.trim().is_empty()) {
            return Err(RhythmProfileError::EmptyRole);
        }
        if !sequence.is_empty() {
            *counts.entry(sequence.join(">")).or_insert(0u64) += 1;
        }
    }
    if counts.is_empty() {
        return Ok(BTreeMap::new());
    }
    Ok(normalize_counts(counts))
}

/// Cosine similarity for two sparse caller-owned distributions.
pub fn cosine_similarity(left: &BTreeMap<String, f64>, right: &BTreeMap<String, f64>) -> f64 {
    let keys = left.keys().chain(right.keys()).collect::<BTreeSet<_>>();
    let dot = keys
        .iter()
        .map(|key| left.get(*key).copied().unwrap_or(0.0) * right.get(*key).copied().unwrap_or(0.0))
        .sum::<f64>();
    let left_norm = keys
        .iter()
        .map(|key| left.get(*key).copied().unwrap_or(0.0).powi(2))
        .sum::<f64>()
        .sqrt();
    let right_norm = keys
        .iter()
        .map(|key| right.get(*key).copied().unwrap_or(0.0).powi(2))
        .sum::<f64>()
        .sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm * right_norm)
    }
}

/// Blends two normalized vocabularies using the caller's left-side weight.
pub fn blend_vocabularies(
    left: &RhythmicVocabulary,
    right: &RhythmicVocabulary,
    left_weight: f64,
) -> Result<RhythmicVocabulary, RhythmProfileError> {
    if !left_weight.is_finite() || !(0.0..=1.0).contains(&left_weight) {
        return Err(RhythmProfileError::InvalidBlendWeight);
    }
    let right_weight = 1.0 - left_weight;
    Ok(RhythmicVocabulary {
        cells: blend_distribution(&left.cells, &right.cells, left_weight, right_weight),
        intervals: blend_distribution(&left.intervals, &right.intervals, left_weight, right_weight),
        forms: blend_distribution(&left.forms, &right.forms, left_weight, right_weight),
    })
}

fn validate_tracks(
    document: &Document,
    tracks: &[usize],
    steps_per_quarter: u32,
) -> Result<(), RhythmProfileError> {
    if steps_per_quarter == 0 {
        return Err(RhythmProfileError::InvalidGrid);
    }
    for track in tracks {
        if *track >= document.tracks.len() {
            return Err(RhythmProfileError::MissingTrack {
                track: *track,
                tracks: document.tracks.len(),
            });
        }
    }
    if tracks.is_empty() {
        return Err(RhythmProfileError::EmptyDistribution);
    }
    Ok(())
}

fn ticks_per_bar(meter: (u32, u32), steps_per_quarter: u32) -> Option<usize> {
    if meter.1 == 0 {
        return None;
    }
    let numerator = u128::from(meter.0) * 4 * u128::from(steps_per_quarter);
    if numerator % u128::from(meter.1) != 0 {
        return None;
    }
    usize::try_from(numerator / u128::from(meter.1)).ok()
}

fn lcm_grid(current: u128, denominator: u128, numerator: u128) -> Result<u128, RhythmProfileError> {
    if denominator == 0 || numerator == 0 {
        return Err(RhythmProfileError::InvalidGrid);
    }
    let needed = denominator / gcd_u128(denominator, numerator);
    let gcd = gcd_u128(current, needed);
    current
        .checked_div(gcd)
        .and_then(|value| value.checked_mul(needed))
        .ok_or(RhythmProfileError::InvalidGrid)
}

fn rhythm_steps(rhythm: Rhythm, steps_per_quarter: u32) -> Option<usize> {
    let base_denominator = u128::from(rhythm.value.denominator());
    let dot_denominator = 1u128.checked_shl(u32::from(rhythm.dots))?;
    let dotted_numerator = dot_denominator.checked_mul(2)?.checked_sub(1)?;
    let mut numerator = dotted_numerator;
    let mut denominator = base_denominator.checked_mul(dot_denominator)?;
    if let Some((tuplet_numerator, tuplet_denominator)) = rhythm.tuplet {
        if tuplet_numerator == 0 || tuplet_denominator == 0 {
            return None;
        }
        numerator = numerator.checked_mul(u128::from(tuplet_denominator))?;
        denominator = denominator.checked_mul(u128::from(tuplet_numerator))?;
    }
    let divisor = gcd_u128(numerator, denominator);
    numerator /= divisor;
    denominator /= divisor;
    let grid_numerator = numerator
        .checked_mul(4)?
        .checked_mul(u128::from(steps_per_quarter))?;
    if grid_numerator % denominator != 0 {
        return None;
    }
    usize::try_from(grid_numerator / denominator).ok()
}

fn gcd_u128(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left.max(1)
}

fn encode_intervals(intervals: &[i32]) -> String {
    intervals
        .iter()
        .map(|interval| format!("{interval:+}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn normalize_counts(counts: BTreeMap<String, u64>) -> BTreeMap<String, f64> {
    let total = counts.values().sum::<u64>().max(1) as f64;
    counts
        .into_iter()
        .map(|(key, count)| (key, count as f64 / total))
        .collect()
}

fn blend_distribution(
    left: &BTreeMap<String, f64>,
    right: &BTreeMap<String, f64>,
    left_weight: f64,
    right_weight: f64,
) -> BTreeMap<String, f64> {
    left.keys()
        .chain(right.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|key| {
            let value = left.get(&key).copied().unwrap_or(0.0) * left_weight
                + right.get(&key).copied().unwrap_or(0.0) * right_weight;
            (value != 0.0).then_some((key, value))
        })
        .collect()
}

/// Applies caller-selected fields to a riff specification and reports changes.
#[cfg(feature = "riffs")]
pub fn apply_riff_profile(
    spec: &mut super::riff_spec::RiffSpec,
    profile: &CallerRiffProfile,
) -> Result<Vec<RiffProfileChange>, RhythmProfileError> {
    let mut next = spec.clone();
    let mut changes = Vec::new();
    macro_rules! change_field {
        ($field:ident, $variant:ident, $target:expr) => {
            if let Some(value) = $target.as_ref() {
                if next.$field != *value {
                    changes.push(RiffProfileChange {
                        field: RiffProfileField::$variant,
                        before: RiffProfileValue::$variant(next.$field.clone()),
                        after: RiffProfileValue::$variant(value.clone()),
                    });
                    next.$field = value.clone();
                }
            }
        };
    }
    change_field!(archetype, Archetype, profile.archetype);
    change_field!(register, Register, profile.register);
    change_field!(voicing, Voicing, profile.voicing);
    change_field!(degrees, Degrees, profile.degrees);
    change_field!(tempo, Tempo, profile.tempo);
    change_field!(palm_mute, PalmMute, profile.palm_mute);
    super::riff_spec::validate(&next)?;
    *spec = next;
    Ok(changes)
}

#[cfg(test)]
#[path = "rhythm_profile_test.rs"]
mod tests;
