//! Read-only score inspection with exact rational timing.

use crate::model::{Document, Rhythm};
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
        assert!(denominator > 0);
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
    let capacity = bar_capacity(master.time);
    if capacity.numerator * 12 % capacity.denominator != 0 {
        return BarFeel::Straight;
    };
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
    let kind = if track.tuning.is_empty() || track.midi_program == Some(0) {
        TrackKind::Percussion
    } else if track.tuning.len() <= 4
        || track.midi_program.is_some_and(|p| (32..=39).contains(&p))
        || track.name.to_ascii_lowercase().contains("bass")
    {
        TrackKind::Bass
    } else if sounding == 0 {
        TrackKind::Empty
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
                        *result
                            .techniques
                            .entry(format!("{technique:?}"))
                            .or_insert(0) += 1;
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
    let duration = doc
        .master_bars
        .iter()
        .fold(Fraction::new(0, 1), |sum, bar| {
            sum.plus(bar_capacity(bar.time))
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
