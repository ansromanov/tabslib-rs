//! Chord labels, playable voicings, and deterministic harmonic riff writing.
//!
//! This module provides style-neutral harmonic building blocks. It does not
//! choose a genre progression or a measured voicing preference; callers
//! supply the progression and the instrument tuning.

use crate::edits::{new_beat, new_note};
use crate::model::{Bar, Document, KeySignature, MasterBar, NoteValue, Rhythm, Track, Voice};

/// Chord quality understood by the harmonic helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChordQuality {
    /// Major triad.
    Major,
    /// Minor triad.
    Minor,
    /// Diminished triad.
    Diminished,
    /// Augmented triad.
    Augmented,
    /// Suspended second.
    Suspended2,
    /// Suspended fourth.
    Suspended4,
    /// Dominant seventh.
    Dominant7,
    /// Major seventh.
    Major7,
    /// Minor seventh.
    Minor7,
    /// Diminished seventh.
    Diminished7,
}

/// A parsed chord label, with root as a pitch class (`C` is zero).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChordLabel {
    /// Root pitch class.
    pub root: i8,
    /// Chord quality.
    pub quality: ChordQuality,
}

/// Errors returned by harmonic parsing and writing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HarmonicError {
    /// A label is not supported by the parser.
    #[error("invalid chord label: {0}")]
    InvalidLabel(String),
    /// A progression or tuning is empty.
    #[error("{0}")]
    InvalidInput(String),
    /// A requested voicing cannot be placed on the tuning.
    #[error("no playable voicing found")]
    NoVoicing,
}

fn root_pitch(letter: char) -> Option<i8> {
    match letter.to_ascii_uppercase() {
        'C' => Some(0),
        'D' => Some(2),
        'E' => Some(4),
        'F' => Some(5),
        'G' => Some(7),
        'A' => Some(9),
        'B' => Some(11),
        _ => None,
    }
}

/// Parses common chord-label spellings such as `C`, `F#m`, and `Bbmaj7`.
///
/// # Errors
///
/// Returns [`HarmonicError::InvalidLabel`] if the label is empty, does not start with a
/// note letter `A` to `G`, or has a suffix that is not a supported chord quality.
pub fn parse_chord_label(label: &str) -> Result<ChordLabel, HarmonicError> {
    let mut chars = label.chars();
    let letter = chars
        .next()
        .ok_or_else(|| HarmonicError::InvalidLabel(label.into()))?;
    let mut root = root_pitch(letter).ok_or_else(|| HarmonicError::InvalidLabel(label.into()))?;
    let accidental = chars.next();
    let suffix = match accidental {
        Some('#') => {
            root += 1;
            chars.as_str().to_string()
        }
        Some('b') | Some('B') => {
            root -= 1;
            chars.as_str().to_string()
        }
        Some(other) => {
            let mut suffix = String::new();
            suffix.push(other);
            suffix.push_str(chars.as_str());
            suffix
        }
        None => String::new(),
    };
    let quality = match suffix.to_ascii_lowercase().as_str() {
        "" | "maj" => ChordQuality::Major,
        "m" | "min" => ChordQuality::Minor,
        "dim" | "o" => ChordQuality::Diminished,
        "aug" | "+" => ChordQuality::Augmented,
        "sus2" => ChordQuality::Suspended2,
        "sus4" | "sus" => ChordQuality::Suspended4,
        "7" => ChordQuality::Dominant7,
        "maj7" => ChordQuality::Major7,
        "m7" | "min7" => ChordQuality::Minor7,
        "dim7" | "o7" => ChordQuality::Diminished7,
        _ => return Err(HarmonicError::InvalidLabel(label.into())),
    };
    Ok(ChordLabel {
        root: root.rem_euclid(12),
        quality,
    })
}

/// Infers a key signature from a chord label, using minor for minor qualities.
pub fn key_from_chord_label(chord: ChordLabel) -> KeySignature {
    let minor = matches!(
        chord.quality,
        ChordQuality::Minor | ChordQuality::Minor7 | ChordQuality::Diminished7
    );
    KeySignature {
        tonic: chord.root,
        minor,
    }
}

fn intervals(quality: ChordQuality) -> &'static [i32] {
    match quality {
        ChordQuality::Major => &[0, 4, 7],
        ChordQuality::Minor => &[0, 3, 7],
        ChordQuality::Diminished => &[0, 3, 6],
        ChordQuality::Augmented => &[0, 4, 8],
        ChordQuality::Suspended2 => &[0, 2, 7],
        ChordQuality::Suspended4 => &[0, 5, 7],
        ChordQuality::Dominant7 => &[0, 4, 7, 10],
        ChordQuality::Major7 => &[0, 4, 7, 11],
        ChordQuality::Minor7 => &[0, 3, 7, 10],
        ChordQuality::Diminished7 => &[0, 3, 6, 9],
    }
}

/// Returns chord tones as pitch classes in ascending interval order.
pub fn build_chord_tones(chord: ChordLabel) -> Vec<i32> {
    intervals(chord.quality)
        .iter()
        .map(|interval| (i32::from(chord.root) + interval).rem_euclid(12))
        .collect()
}

/// Returns chord tones transposed into a key-relative pitch-class space.
pub fn build_chord_tones_in_key(chord: ChordLabel, key: KeySignature) -> Vec<i32> {
    build_chord_tones(chord)
        .into_iter()
        .map(|pitch| (pitch - i32::from(key.tonic)).rem_euclid(12))
        .collect()
}

/// Builds a seventh chord from a root and quality.
pub fn build_seventh_chord(root: i8, quality: ChordQuality) -> ChordLabel {
    ChordLabel {
        root: root.rem_euclid(12),
        quality,
    }
}

/// Returns the nearest octave placement of a chord tone to `pitch`.
pub fn nearest_chord_tone(pitch: i32, chord: ChordLabel) -> i32 {
    build_chord_tones(chord)
        .into_iter()
        .map(|tone| pitch + (tone - pitch).rem_euclid(12))
        .min_by_key(|candidate| candidate.abs_diff(pitch))
        .unwrap_or(pitch)
}

/// Reports whether a MIDI pitch belongs to the chord.
pub fn is_chord_tone(pitch: i32, chord: ChordLabel) -> bool {
    build_chord_tones(chord).contains(&pitch.rem_euclid(12))
}

/// A concrete voicing, with one fret or mute per string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordVoicing {
    /// Frets in tuning order; `None` means that string is muted.
    pub frets: Vec<Option<i32>>,
    /// Sounding MIDI pitches in string order.
    pub pitches: Vec<i32>,
    /// Lower is better; the score favours compact, low-fret shapes.
    pub score: u32,
}

/// Search preferences for enumerating guitar chord voicings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoicingOptions {
    /// Lowest fret in the search window.
    pub min_fret: i32,
    /// Highest fret in the search window.
    pub max_fret: i32,
    /// Maximum number of candidate shapes to return.
    pub max_candidates: usize,
    /// Prefer open strings when selecting the best candidate.
    pub open_string_bias: bool,
}

impl Default for VoicingOptions {
    fn default() -> Self {
        Self {
            min_fret: 0,
            max_fret: 12,
            max_candidates: 256,
            open_string_bias: true,
        }
    }
}

/// Finds deterministic, playable chord shapes up to `max_fret`.
///
/// # Errors
///
/// Returns [`HarmonicError::InvalidInput`] if `tuning` is empty or `max_fret` is negative,
/// and [`HarmonicError::NoVoicing`] if a string has no fret up to `max_fret` that
/// plays a chord tone.
pub fn find_voicings(
    chord: ChordLabel,
    tuning: &[i32],
    max_fret: i32,
) -> Result<Vec<ChordVoicing>, HarmonicError> {
    if tuning.is_empty() || max_fret < 0 {
        return Err(HarmonicError::InvalidInput(
            "tuning and fret range are required".into(),
        ));
    }
    let tones = build_chord_tones(chord);
    let mut frets = Vec::with_capacity(tuning.len());
    let mut pitches = Vec::new();
    for open in tuning {
        let choice = (0..=max_fret)
            .find(|fret| tones.contains(&((*open + *fret).rem_euclid(12))))
            .ok_or(HarmonicError::NoVoicing)?;
        frets.push(Some(choice));
        pitches.push(*open + choice);
    }
    let score = frets
        .iter()
        .flatten()
        .copied()
        .map(|fret| u32::try_from(fret).unwrap_or(0))
        .sum();
    Ok(vec![ChordVoicing {
        frets,
        pitches,
        score,
    }])
}

/// Enumerates playable shapes across a caller-selected fret window.
///
/// # Errors
///
/// Returns [`HarmonicError::InvalidInput`] if `tuning` is empty, `options.min_fret` is
/// negative or above `options.max_fret`, or `options.max_candidates` is zero. Returns
/// [`HarmonicError::NoVoicing`] if no playable shape exists in the fret window.
pub fn find_voicings_with_options(
    chord: ChordLabel,
    tuning: &[i32],
    options: VoicingOptions,
) -> Result<Vec<ChordVoicing>, HarmonicError> {
    if tuning.is_empty()
        || options.min_fret < 0
        || options.min_fret > options.max_fret
        || options.max_candidates == 0
    {
        return Err(HarmonicError::InvalidInput(
            "invalid voicing search options".into(),
        ));
    }
    let tones = build_chord_tones(chord);
    let mut shapes = Vec::new();
    enumerate_voicings(
        tuning,
        &tones,
        options.min_fret,
        options.max_fret,
        options.max_candidates,
        0,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut shapes,
    );
    if shapes.is_empty() {
        return Err(HarmonicError::NoVoicing);
    }
    Ok(shapes)
}

/// Selects the lowest-scoring voicing, breaking ties lexicographically.
pub fn pick_best_voicing(voicings: &[ChordVoicing]) -> Option<ChordVoicing> {
    voicings.iter().cloned().min_by(|left, right| {
        left.score
            .cmp(&right.score)
            .then_with(|| left.frets.cmp(&right.frets))
    })
}

/// Selects a voicing using the requested open-string preference.
pub fn pick_best_voicing_with_options(
    voicings: &[ChordVoicing],
    options: VoicingOptions,
) -> Option<ChordVoicing> {
    voicings.iter().cloned().min_by(|left, right| {
        let left_open = left.frets.iter().filter(|fret| **fret == Some(0)).count();
        let right_open = right.frets.iter().filter(|fret| **fret == Some(0)).count();
        let left_key = if options.open_string_bias {
            (left.score, usize::MAX - left_open)
        } else {
            (left.score, 0)
        };
        let right_key = if options.open_string_bias {
            (right.score, usize::MAX - right_open)
        } else {
            (right.score, 0)
        };
        left_key
            .cmp(&right_key)
            .then_with(|| left.frets.cmp(&right.frets))
    })
}

#[allow(clippy::too_many_arguments)]
fn enumerate_voicings(
    tuning: &[i32],
    tones: &[i32],
    min_fret: i32,
    max_fret: i32,
    max_candidates: usize,
    string: usize,
    frets: &mut Vec<Option<i32>>,
    pitches: &mut Vec<i32>,
    output: &mut Vec<ChordVoicing>,
) {
    if output.len() >= max_candidates {
        return;
    }
    if string == tuning.len() {
        if pitches.is_empty() {
            return;
        }
        let span = frets.iter().flatten().copied().max().unwrap_or(0)
            - frets.iter().flatten().copied().min().unwrap_or(0);
        let score = frets
            .iter()
            .flatten()
            .copied()
            .map(|fret| u32::try_from(fret).unwrap_or(0))
            .sum::<u32>()
            .saturating_add(u32::try_from(span.max(0)).unwrap_or(u32::MAX));
        output.push(ChordVoicing {
            frets: frets.clone(),
            pitches: pitches.clone(),
            score,
        });
        return;
    }
    frets.push(None);
    enumerate_voicings(
        tuning,
        tones,
        min_fret,
        max_fret,
        max_candidates,
        string + 1,
        frets,
        pitches,
        output,
    );
    frets.pop();
    for fret in min_fret..=max_fret {
        let pitch = tuning[string] + fret;
        if !tones.contains(&pitch.rem_euclid(12)) {
            continue;
        }
        frets.push(Some(fret));
        pitches.push(pitch);
        enumerate_voicings(
            tuning,
            tones,
            min_fret,
            max_fret,
            max_candidates,
            string + 1,
            frets,
            pitches,
            output,
        );
        pitches.pop();
        frets.pop();
    }
}

/// Returns the MIDI pitches represented by a voicing.
pub fn voicing_to_midi_pitches(voicing: &ChordVoicing) -> Vec<i32> {
    voicing.pitches.clone()
}

/// Settings for [`write_harmonic_riff`].
#[derive(Debug, Clone, PartialEq)]
pub struct HarmonicRiffConfig {
    /// Open-string MIDI pitches, lowest string first.
    pub tuning: Vec<i32>,
    /// Highest fret to consider.
    pub max_fret: i32,
    /// Number of bars for each chord.
    pub bars_per_chord: usize,
    /// Initial tempo.
    pub tempo: f64,
    /// Track name.
    pub name: String,
}

impl Default for HarmonicRiffConfig {
    fn default() -> Self {
        Self {
            tuning: vec![40, 45, 50, 55, 59, 64],
            max_fret: 12,
            bars_per_chord: 1,
            tempo: 120.0,
            name: "Harmonic riff".into(),
        }
    }
}

/// Writes one whole-note chord per bar from a supplied progression.
///
/// # Errors
///
/// Returns [`HarmonicError::InvalidInput`] if the progression is empty,
/// `config.bars_per_chord` is zero, or `config.tempo` is not positive, or if a bar id
/// does not fit the score. Returns any error from [`find_voicings`] if a chord has no
/// playable voicing on the configured tuning.
pub fn write_harmonic_riff(
    progression: &[ChordLabel],
    config: &HarmonicRiffConfig,
) -> Result<Document, HarmonicError> {
    if progression.is_empty() || config.bars_per_chord == 0 || config.tempo <= 0.0 {
        return Err(HarmonicError::InvalidInput(
            "progression, bars, and tempo are required".into(),
        ));
    }
    let mut document = Document {
        title: config.name.clone(),
        tracks: vec![Track {
            id: 1,
            name: config.name.clone(),
            color: None,
            tuning: config.tuning.clone(),
            midi_program: None,
            midi_channel: None,
            pan: None,
            volume: None,
            mute: false,
            solo: false,
            percussion_articulations: Vec::new(),
            generator_metadata: None,
        }],
        ..Document::default()
    };
    let mut bar_id = 1u32;
    let mut beat_id = 1u32;
    let mut note_id = 1u32;
    for (index, chord) in progression.iter().enumerate() {
        let voicing = pick_best_voicing(&find_voicings(*chord, &config.tuning, config.max_fret)?)
            .ok_or(HarmonicError::NoVoicing)?;
        for repeat in 0..config.bars_per_chord {
            let mut bar = Bar {
                id: bar_id,
                clef: None,
                voices: vec![Voice {
                    id: bar_id,
                    beats: Vec::new(),
                }],
            };
            let notes = voicing
                .frets
                .iter()
                .enumerate()
                .filter_map(|(string, fret)| {
                    fret.map(|fret| {
                        new_note(
                            note_id + u32::try_from(string).unwrap_or(0),
                            Some(voicing.pitches[string]),
                            Some(u32::try_from(string).unwrap_or(0)),
                            Some(fret),
                        )
                    })
                })
                .collect();
            bar.voices[0]
                .beats
                .push(new_beat(beat_id, Rhythm::new(NoteValue::Whole), notes));
            document.bars.push(bar);
            document.master_bars.push(MasterBar {
                index: document.master_bars.len(),
                time: (4, 4),
                section: (repeat == 0).then(|| format!("Chord {}", index + 1)),
                double_bar: false,
                bar_ids: vec![i32::try_from(bar_id)
                    .map_err(|_| HarmonicError::InvalidInput("bar id overflow".into()))?],
                repeat_start: false,
                repeat_end: None,
                alternate_ending: 0,
                direction: None,
            });
            bar_id = bar_id.saturating_add(1);
            beat_id = beat_id.saturating_add(1);
            note_id = note_id.saturating_add(16);
        }
    }
    document.tempo_map.push(crate::model::TempoChange {
        at_bar: 0,
        bpm: config.tempo,
        linear: false,
    });
    document.key = progression.first().copied().map(key_from_chord_label);
    Ok(document)
}

/// Samples a caller-supplied list of chord labels into a validated progression.
///
/// # Errors
///
/// Returns [`HarmonicError::InvalidLabel`] for the first label that
/// [`parse_chord_label`] rejects.
pub fn sample_progression(labels: &[&str]) -> Result<Vec<ChordLabel>, HarmonicError> {
    labels
        .iter()
        .map(|label| parse_chord_label(label))
        .collect()
}

/// Writes multiple progressions using one deterministic configuration.
///
/// # Errors
///
/// Returns the first error from [`write_harmonic_riff`] for any of the progressions.
pub fn generate_harmonic_riffs(
    progressions: &[Vec<ChordLabel>],
    config: &HarmonicRiffConfig,
) -> Result<Vec<Document>, HarmonicError> {
    progressions
        .iter()
        .map(|progression| write_harmonic_riff(progression, config))
        .collect()
}

#[cfg(test)]
#[path = "harmonic_test.rs"]
mod tests;
