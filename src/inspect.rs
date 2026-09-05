//! Read-only score inspection with exact rational timing.

use crate::model::{Document, Rhythm, Technique};
use std::collections::BTreeMap;

/// An exact non-negative rational number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fraction {
    /// Numerator in lowest terms.
    pub numerator: u64,
    /// Positive denominator in lowest terms.
    pub denominator: u64,
}
impl Fraction {
    /// Creates and reduces a fraction.
    pub fn new(numerator: u64, denominator: u64) -> Self {
        if denominator == 0 {
            return Self {
                numerator: 0,
                denominator: 1,
            };
        }
        let g = gcd(numerator, denominator);
        Self {
            numerator: numerator / g,
            denominator: denominator / g,
        }
    }
    /// Adds two fractions exactly.
    pub fn plus(self, other: Self) -> Self {
        Self::new(
            self.numerator * other.denominator + other.numerator * self.denominator,
            self.denominator * other.denominator,
        )
    }
}

impl PartialOrd for Fraction {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Fraction {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.numerator * other.denominator).cmp(&(other.numerator * self.denominator))
    }
}
fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a.max(1)
    } else {
        gcd(b, a % b)
    }
}

/// Exact meter capacity, measured in whole notes.
pub fn bar_capacity(meter: (u32, u32)) -> Fraction {
    Fraction::new(meter.0 as u64, meter.1 as u64)
}

/// Returns master-bar indices in playback order, expanding simple repeats.
///
/// ```
/// use tabslib::{fixtures, inspect};
///
/// let mut score = fixtures::repeated_frets();
/// score.master_bars[0].repeat_start = true;
/// let last = score.master_bars.len() - 1;
/// score.master_bars[last].repeat_end = Some(2);
/// assert_eq!(inspect::playback_order(&score).len(), score.master_bars.len() * 2);
/// ```
pub fn playback_order(doc: &Document) -> Vec<usize> {
    let mut order = Vec::new();
    let mut repeat_start = 0;
    for (index, master) in doc.master_bars.iter().enumerate() {
        if master.repeat_start {
            repeat_start = index;
        }
        order.push(index);
        if let Some(count) = master.repeat_end {
            for _ in 1..count.max(1) {
                order.extend(repeat_start..=index);
            }
        }
    }
    order
}

/// Returns note locations whose tie endpoints are missing or have different pitches.
///
/// The returned tuple is `(bar, voice, origin_beat, destination_beat)`.
///
/// ```
/// use tabslib::{fixtures, inspect};
/// use tabslib::model::Technique;
///
/// let mut score = fixtures::repeated_frets();
/// score.bars[0].voices[0].beats[0].notes[0].techniques.push(Technique::TieOrigin);
/// score.bars[0].voices[0].beats[1].notes[0].techniques.push(Technique::TieDestination);
/// assert!(inspect::tied_pitch_mismatches(&score).is_empty());
/// ```
pub fn tied_pitch_mismatches(doc: &Document) -> Vec<(usize, usize, usize, usize)> {
    let mut mismatches = Vec::new();
    for track_index in 0..doc.tracks.len() {
        let mut pending: Vec<Vec<PendingTie>> = Vec::new();
        for bar_index in 0..doc.master_bars.len() {
            let Some(bar_id) = doc.master_bars[bar_index].bar_ids.get(track_index) else {
                continue;
            };
            let Some(bar) = doc.bars.iter().find(|bar| bar.id == *bar_id as u32) else {
                continue;
            };
            for (voice_index, voice) in bar.voices.iter().enumerate() {
                if pending.len() <= voice_index {
                    pending.resize_with(voice_index + 1, Vec::new);
                }
                for (beat_index, beat) in voice.beats.iter().enumerate() {
                    for note in &beat.notes {
                        let destination = note
                            .techniques
                            .iter()
                            .any(|technique| matches!(technique, Technique::TieDestination));
                        let origin = note
                            .techniques
                            .iter()
                            .any(|technique| matches!(technique, Technique::TieOrigin));
                        if destination {
                            if let Some(position) = pending[voice_index]
                                .iter()
                                .position(|(_, _, _, midi)| *midi == note.midi)
                            {
                                pending[voice_index].remove(position);
                            } else {
                                mismatches.push((bar_index, voice_index, beat_index, beat_index));
                            }
                        }
                        if origin {
                            pending[voice_index].push((
                                bar_index,
                                voice_index,
                                beat_index,
                                note.midi,
                            ));
                        }
                    }
                }
            }
        }
        for (voice_index, pending) in pending.into_iter().enumerate() {
            for (bar_index, _, beat_index, _) in pending {
                mismatches.push((bar_index, voice_index, beat_index, beat_index));
            }
        }
    }
    mismatches
}
/// Exact duration of a written rhythm, measured in whole notes.
pub fn rhythm_duration(rhythm: Rhythm) -> Fraction {
    let (n, d) = rhythm.as_fraction();
    Fraction::new(n, d)
}

/// Rhythmic feel inferred from explicit beat tuplets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarFeel {
    /// Even subdivisions.
    Straight,
    /// Three subdivisions per quarter.
    Triplet,
    /// A swung passage.
    Swung,
    /// The meter denominator is outside the supported written grid.
    Unsupported,
}
/// A beat's exact location within its bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeatPosition {
    /// Zero-based beat index.
    pub beat_index: usize,
    /// Exact onset.
    pub onset: Fraction,
    /// Exact duration.
    pub duration: Fraction,
    /// Derived grid step.
    pub step: u32,
    /// Whether onset is on the grid.
    pub exact: bool,
}

fn bar_for(doc: &Document, bar_index: usize, track_index: usize) -> Option<&crate::model::Bar> {
    let id = *doc.master_bars.get(bar_index)?.bar_ids.get(track_index)?;
    doc.bars.iter().find(|bar| bar.id == id as u32)
}
/// Returns exact beat positions for one track's voice in one bar.
pub fn beat_positions(
    doc: &Document,
    bar_index: usize,
    track_index: usize,
    voice_index: usize,
) -> Vec<BeatPosition> {
    let Some(bar) = bar_for(doc, bar_index, track_index) else {
        return Vec::new();
    };
    let Some(voice) = bar.voices.get(voice_index) else {
        return Vec::new();
    };
    let feel = detect_bar_feel(doc, bar_index, track_index, voice_index);
    let step = Fraction::new(
        1,
        if matches!(feel, BarFeel::Triplet | BarFeel::Swung) {
            12
        } else {
            16
        },
    );
    let mut onset = Fraction::new(0, 1);
    voice
        .beats
        .iter()
        .enumerate()
        .map(|(i, beat)| {
            let duration = rhythm_duration(beat.rhythm);
            let quotient = onset.numerator * step.denominator;
            let divisor = onset.denominator * step.numerator;
            let result = BeatPosition {
                beat_index: i,
                onset,
                duration,
                step: (quotient / divisor) as u32,
                exact: quotient % divisor == 0,
            };
            onset = onset.plus(duration);
            result
        })
        .collect()
}
/// Detects straight or triplet feel from explicit tuplets.
pub fn detect_bar_feel(
    doc: &Document,
    bar_index: usize,
    track_index: usize,
    voice_index: usize,
) -> BarFeel {
    let Some(master) = doc.master_bars.get(bar_index) else {
        return BarFeel::Straight;
    };
    if !matches!(master.time.1, 1 | 2 | 4 | 8 | 16 | 32 | 64) {
        return BarFeel::Unsupported;
    }
    let Some(voice) =
        bar_for(doc, bar_index, track_index).and_then(|bar| bar.voices.get(voice_index))
    else {
        return BarFeel::Straight;
    };
    let sounding = voice
        .beats
        .iter()
        .filter(|beat| !beat.notes.is_empty())
        .collect::<Vec<_>>();
    if sounding.is_empty() {
        return BarFeel::Straight;
    };
    let triplets = sounding
        .iter()
        .filter(|beat| beat.rhythm.tuplet == Some((3, 2)))
        .count();
    if triplets * 2 >= sounding.len() {
        BarFeel::Triplet
    } else if sounding
        .chunks(2)
        .filter(|pair| {
            pair.len() == 2
                && pair[0].rhythm.as_fraction().0 * pair[1].rhythm.as_fraction().1
                    == pair[1].rhythm.as_fraction().0 * pair[0].rhythm.as_fraction().1 * 3
        })
        .count()
        * 2
        >= sounding.len()
    {
        BarFeel::Swung
    } else {
        BarFeel::Straight
    }
}

/// Coarse classification of a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    /// Percussion.
    Percussion,
    /// Bass.
    Bass,
    /// Empty.
    Empty,
    /// Other sounding instrument.
    Other,
}
/// Counts notes and techniques in a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteCensus {
    /// Number of notes.
    pub notes: usize,
    /// Counts keyed by debug technique name.
    pub techniques: BTreeMap<String, usize>,
    /// Percussion articulation counts.
    pub articulations: BTreeMap<i32, usize>,
}
/// A voice whose duration differs from meter capacity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarIntegrity {
    /// Track index.
    pub track_index: usize,
    /// Track name.
    pub track_name: String,
    /// Bar index.
    pub bar_index: usize,
    /// Voice index.
    pub voice_index: usize,
    /// Written duration.
    pub duration: Fraction,
    /// Meter capacity.
    pub capacity: Fraction,
}
type PendingTie = (usize, usize, usize, Option<i32>);
/// Summary of structural score facts.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreSummary {
    /// Track names and kinds.
    pub tracks: Vec<(String, TrackKind)>,
    /// Number of bars.
    pub bars: usize,
    /// Total exact duration.
    pub duration: Fraction,
    /// Meter map.
    pub meters: Vec<(u32, u32)>,
    /// Tempo map.
    pub tempo_map: Vec<crate::model::TempoChange>,
    /// Key signature.
    pub key: Option<crate::model::KeySignature>,
    /// Tunings in track order.
    pub tunings: Vec<Vec<i32>>,
}
/// Classifies a track and returns its sounding-bar count.
pub fn classify_track(doc: &Document, track_index: usize) -> Option<(TrackKind, usize)> {
    let track = doc.tracks.get(track_index)?;
    let mut sounding = 0;
    for bar in 0..doc.master_bars.len() {
        if bar_for(doc, bar, track_index).is_some_and(|b| {
            b.voices
                .iter()
                .any(|v| v.beats.iter().any(|beat| !beat.notes.is_empty()))
        }) {
            sounding += 1;
        }
    }
    let name = track.name.to_ascii_lowercase();
    let kind = if track.tuning.is_empty() || track.tuning.iter().all(|pitch| *pitch == 0) {
        TrackKind::Percussion
    } else if sounding == 0 {
        TrackKind::Empty
    } else if track.tuning.len() <= 4
        || track.midi_program.is_some_and(|p| (32..=39).contains(&p))
        || name.contains("bass")
    {
        TrackKind::Bass
    } else {
        TrackKind::Other
    };
    Some((kind, sounding))
}
/// Counts notes, techniques, and percussion articulations.
pub fn note_census(doc: &Document) -> NoteCensus {
    let mut result = NoteCensus {
        notes: 0,
        techniques: BTreeMap::new(),
        articulations: BTreeMap::new(),
    };
    for bar in &doc.bars {
        for voice in &bar.voices {
            for beat in &voice.beats {
                for note in &beat.notes {
                    result.notes += 1;
                    if let Some(a) = note.articulation {
                        *result.articulations.entry(a).or_insert(0) += 1;
                    }
                    for technique in &note.techniques {
                        let name = match technique {
                            crate::model::Technique::Slide { .. } => "Slide".to_string(),
                            crate::model::Technique::Bend { .. } => "Bend".to_string(),
                            crate::model::Technique::Harmonic { .. } => "Harmonic".to_string(),
                            _ => format!("{technique:?}"),
                        };
                        *result.techniques.entry(name).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    result
}
/// Finds every under-full or over-full voice.
pub fn bar_integrity(doc: &Document) -> Vec<BarIntegrity> {
    let mut result = Vec::new();
    for (track_index, track) in doc.tracks.iter().enumerate() {
        for (bar_index, master) in doc.master_bars.iter().enumerate() {
            let Some(bar) = bar_for(doc, bar_index, track_index) else {
                continue;
            };
            let capacity = bar_capacity(master.time);
            for (voice_index, voice) in bar.voices.iter().enumerate() {
                let duration = voice.beats.iter().fold(Fraction::new(0, 1), |sum, beat| {
                    sum.plus(rhythm_duration(beat.rhythm))
                });
                if duration != capacity {
                    result.push(BarIntegrity {
                        track_index,
                        track_name: track.name.clone(),
                        bar_index,
                        voice_index,
                        duration,
                        capacity,
                    });
                }
            }
        }
    }
    result
}
/// Builds a complete structural summary.
pub fn summary(doc: &Document) -> ScoreSummary {
    let duration = playback_order(doc)
        .into_iter()
        .fold(Fraction::new(0, 1), |sum, index| {
            sum.plus(bar_capacity(doc.master_bars[index].time))
        });
    ScoreSummary {
        tracks: doc
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(i, track)| {
                classify_track(doc, i).map(|kind| (track.name.clone(), kind.0))
            })
            .collect(),
        bars: doc.master_bars.len(),
        duration,
        meters: doc.master_bars.iter().map(|bar| bar.time).collect(),
        tempo_map: doc.tempo_map.clone(),
        key: doc.key,
        tunings: doc
            .tracks
            .iter()
            .map(|track| track.tuning.clone())
            .collect(),
    }
}
/// Parses `C# minor`, `Bb major`, or `F#`.
pub fn parse_key(input: &str) -> Option<crate::model::KeySignature> {
    let mut words = input.split_whitespace();
    let tonic = words.next()?.to_ascii_lowercase();
    let minor = words
        .next()
        .is_some_and(|mode| mode == "minor" || mode == "m");
    let tonic = match tonic.as_str() {
        "c" => 0,
        "c#" | "db" => 1,
        "d" => 2,
        "d#" | "eb" => 3,
        "e" => 4,
        "f" => 5,
        "f#" | "gb" => 6,
        "g" => 7,
        "g#" | "ab" => 8,
        "a" => 9,
        "a#" | "bb" => 10,
        "b" | "cb" => 11,
        _ => return None,
    };
    Some(crate::model::KeySignature { tonic, minor })
}
/// Parses note names with octaves, or the conventional `Drop D` tuning.
pub fn parse_tuning(input: &str) -> Option<Vec<i32>> {
    if input.trim().is_empty() {
        return None;
    }
    if input.trim().eq_ignore_ascii_case("drop d") {
        return Some(vec![38, 45, 50, 55, 59, 64]);
    }
    input
        .split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            let split = lower.len().checked_sub(1)?;
            let (name, octave) = lower.split_at(split);
            let octave: i32 = octave.parse().ok()?;
            let pitch = match name {
                "c" => 0,
                "c#" | "db" => 1,
                "d" => 2,
                "d#" | "eb" => 3,
                "e" => 4,
                "f" => 5,
                "f#" | "gb" => 6,
                "g" => 7,
                "g#" | "ab" => 8,
                "a" => 9,
                "a#" | "bb" => 10,
                "b" => 11,
                _ => return None,
            };
            Some((octave + 1) * 12 + pitch)
        })
        .collect()
}
