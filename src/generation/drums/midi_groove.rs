//! Caller-owned MIDI groove conversion, indexing, selection, and riff adaptation.
//!
//! The library accepts MIDI bytes and metadata supplied by the application. It
//! does not scan directories or compile a repertoire into the crate.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::format::midi;
use crate::generation::drums::performance::{generate_seeded_hits, DrumProfile};
use crate::generation::drums::{DrumHit, DrumPattern, RiffOnset};
use crate::generation::pattern_catalog::PatternGrid;
use crate::inspect::{self, BarFeel, Fraction};
use crate::percussion::{lookup, DrumRole};

mod ranking;
pub use ranking::find_best_midi_grooves;

/// Rhythmic feel inferred from MIDI event timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MidiGrooveFeel {
    /// Even written subdivisions.
    Straight,
    /// Regular three-in-the-time-of-two subdivisions.
    Triplet,
    /// Unequal paired subdivisions indicating swing.
    Swung,
    /// Sounding bars use more than one supported feel.
    Mixed,
    /// A bar uses a meter outside the feel detector's grid.
    Unsupported,
}

/// Caller-supplied MIDI groove indexed by role, meter, and tempo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiGroove {
    id: String,
    role: String,
    grid: PatternGrid,
    tempo_bpm: u32,
    feel: MidiGrooveFeel,
    bars: Vec<Vec<DrumHit>>,
    timekeeping_voice: Option<u8>,
    genre: String,
}

impl MidiGroove {
    /// Stable identifier assigned by the caller.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Caller-defined use or role, such as a section name.
    pub fn role(&self) -> &str {
        &self.role
    }

    /// Grid on which the imported groove is expressed.
    pub fn grid(&self) -> PatternGrid {
        self.grid
    }

    /// Caller-supplied tempo metadata in beats per minute.
    pub fn tempo_bpm(&self) -> u32 {
        self.tempo_bpm
    }

    /// Feel detected from imported note-on spacing.
    pub fn feel(&self) -> MidiGrooveFeel {
        self.feel
    }

    /// Drum hits grouped by bar, with each step relative to its bar.
    pub fn bars(&self) -> &[Vec<DrumHit>] {
        &self.bars
    }

    /// Most common recognized cymbal voice, if the MIDI groove has one.
    pub fn timekeeping_voice(&self) -> Option<u8> {
        self.timekeeping_voice
    }

    /// Normalized genre label inferred from the caller's identifier.
    pub fn genre(&self) -> &str {
        &self.genre
    }
}

/// In-memory index of MIDI grooves provided by the caller.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MidiGrooveIndex {
    grooves: Vec<MidiGroove>,
}

/// Multi-factor request used to rank indexed grooves.
#[derive(Debug, Clone, PartialEq)]
pub struct MidiGrooveQuery {
    /// Requested role or section name.
    pub role: String,
    /// Required meter.
    pub meter: (u32, u32),
    /// Desired tempo, used as a proximity preference.
    pub tempo_bpm: u32,
    /// Preferred rhythmic feel.
    pub feel: Option<MidiGrooveFeel>,
    /// Preferred genre label.
    pub genre: Option<String>,
    /// Desired riff onset density in the range `0.0..=1.0`.
    pub riff_density: Option<f32>,
    /// Desired sustained-note ratio in the range `0.0..=1.0`.
    pub sustain_ratio: Option<f32>,
    /// Whether blast-labelled candidates are allowed.
    pub allow_blast: bool,
}

impl Default for MidiGrooveQuery {
    fn default() -> Self {
        Self {
            role: String::new(),
            meter: (4, 4),
            tempo_bpm: 120,
            feel: None,
            genre: None,
            riff_density: None,
            sustain_ratio: None,
            allow_blast: false,
        }
    }
}

/// A groove and its deterministic compatibility score.
#[derive(Debug, Clone, PartialEq)]
pub struct MidiGrooveCandidate {
    /// Indexed groove being considered.
    pub groove: MidiGroove,
    /// Higher scores are better.
    pub score: i32,
}

impl MidiGrooveCandidate {
    /// Stable identifier of the ranked groove.
    pub fn id(&self) -> &str {
        self.groove.id()
    }

    /// Detected feel of the ranked groove.
    pub fn feel(&self) -> MidiGrooveFeel {
        self.groove.feel()
    }
}

/// One MIDI file supplied to [`index_midi_directory`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiGrooveFile {
    /// Path-like label used for stable identifiers and role inference.
    pub path: String,
    /// MIDI payload supplied by the application.
    pub bytes: Vec<u8>,
}

/// Indexes caller-supplied MIDI directory entries without accessing the filesystem.
///
/// Role and tempo are inferred from path components containing `drums`,
/// `bass`, or `guitar`, and from the first numeric component in the filename.
/// Applications can still use [`MidiGrooveIndex::add_midi`] when metadata is
/// known explicitly.
///
/// # Errors
///
/// Returns the first error from [`MidiGrooveIndex::add_midi`] for any file, such as
/// [`MidiGrooveError::Midi`] for unreadable bytes, [`MidiGrooveError::NoDrumNotes`] or
/// [`MidiGrooveError::DuplicateId`] when two files share a path.
pub fn index_midi_directory(files: &[MidiGrooveFile]) -> Result<MidiGrooveIndex, MidiGrooveError> {
    let grid = PatternGrid::new((4, 4), 4)
        .map_err(|error| MidiGrooveError::InvalidGrid(error.to_string()))?;
    let mut index = MidiGrooveIndex::new();
    for file in files {
        let path = std::path::Path::new(&file.path);
        let id = path.to_string_lossy().into_owned();
        let lower = file.path.to_ascii_lowercase();
        let role = if lower.contains("drum") {
            "drums"
        } else if lower.contains("bass") {
            "bass"
        } else if lower.contains("guitar") {
            "guitar"
        } else {
            "unknown"
        };
        let tempo = lower
            .split(|character: char| !character.is_ascii_digit())
            .find(|component| !component.is_empty())
            .and_then(|value| value.parse().ok())
            .unwrap_or(120);
        index.add_midi(id, role, tempo, grid, &file.bytes)?;
    }
    Ok(index)
}

impl MidiGrooveIndex {
    /// Creates an empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a MIDI groove after parsing and projecting it onto `grid`.
    ///
    /// # Errors
    ///
    /// Returns [`MidiGrooveError::EmptyId`], [`MidiGrooveError::EmptyRole`] or
    /// [`MidiGrooveError::InvalidTempo`] for a blank id, a blank role or a zero tempo, and
    /// [`MidiGrooveError::DuplicateId`] if `id` is already indexed. Also returns any error from
    /// [`midi_to_drum_hits`] if the bytes cannot be parsed or projected onto `grid`.
    pub fn add_midi(
        &mut self,
        id: impl Into<String>,
        role: impl Into<String>,
        tempo_bpm: u32,
        grid: PatternGrid,
        bytes: &[u8],
    ) -> Result<(), MidiGrooveError> {
        let id = id.into();
        let role = role.into();
        if id.trim().is_empty() {
            return Err(MidiGrooveError::EmptyId);
        }
        if role.trim().is_empty() {
            return Err(MidiGrooveError::EmptyRole);
        }
        if tempo_bpm == 0 {
            return Err(MidiGrooveError::InvalidTempo);
        }
        if self.grooves.iter().any(|groove| groove.id == id) {
            return Err(MidiGrooveError::DuplicateId { id });
        }
        let parsed = parse_midi_groove(bytes, grid)?;
        let genre = infer_genre(&id);
        self.grooves.push(MidiGroove {
            id,
            role,
            grid,
            tempo_bpm,
            feel: parsed.feel,
            bars: parsed.bars,
            timekeeping_voice: parsed.timekeeping_voice,
            genre,
        });
        Ok(())
    }

    /// Grooves in insertion order.
    pub fn grooves(&self) -> &[MidiGroove] {
        &self.grooves
    }

    /// Selects the nearest-tempo groove with an exact role and meter match.
    /// Ties resolve by the smaller tempo and then identifier.
    pub fn select(&self, role: &str, meter: (u32, u32), tempo_bpm: u32) -> Option<&MidiGroove> {
        self.grooves
            .iter()
            .filter(|groove| groove.role == role && groove.grid.meter == meter)
            .min_by_key(|groove| {
                (
                    groove.tempo_bpm.abs_diff(tempo_bpm),
                    groove.tempo_bpm,
                    groove.id.as_str(),
                )
            })
    }

    /// Returns all compatible grooves in descending deterministic score order.
    pub fn find_best(&self, query: &MidiGrooveQuery) -> Vec<MidiGrooveCandidate> {
        find_best_midi_grooves(self, query)
    }
}

fn infer_genre(id: &str) -> String {
    let lower = id.to_ascii_lowercase();
    ["metal", "rock", "jazz", "blues", "funk", "pop", "punk"]
        .iter()
        .find(|genre| lower.contains(**genre))
        .copied()
        .unwrap_or("unknown")
        .to_string()
}

/// Explicit seeded-generation fallback used only when no indexed groove matches.
pub struct SeededGrooveFallback<'a> {
    /// Source rhythm pattern for the existing seeded generator.
    pub pattern: &'a DrumPattern,
    /// Voice and probability profile for the seeded generator.
    pub profile: &'a DrumProfile,
    /// Reproducible random seed.
    pub seed: u64,
}

/// Where a groove-generation result came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiGrooveSource {
    /// A groove selected from the caller's in-memory index.
    Indexed {
        /// Stable identifier of the selected groove.
        groove_id: String,
    },
    /// The explicit seeded fallback supplied by the caller.
    SeededFallback,
}

/// Hits and provenance returned by indexed selection or explicit fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiGrooveGeneration {
    /// Hit events across the selected phrase.
    pub hits: Vec<DrumHit>,
    /// Selection path used to produce the hits.
    pub source: MidiGrooveSource,
}

/// Converts MIDI bytes to drum hits on a caller-selected grid.
///
/// # Errors
///
/// Returns [`MidiGrooveError::InvalidGrid`] if `grid` has no whole bar width and
/// [`MidiGrooveError::Midi`] if the bytes are not readable MIDI. Returns
/// [`MidiGrooveError::NoDrumNotes`] if no General MIDI drum note is found, and
/// [`MidiGrooveError::UnrepresentableOnset`] or [`MidiGrooveError::GridOverflow`] if an onset
/// cannot be placed exactly on `grid`.
pub fn midi_to_drum_hits(
    bytes: &[u8],
    grid: PatternGrid,
) -> Result<Vec<Vec<DrumHit>>, MidiGrooveError> {
    Ok(parse_midi_groove(bytes, grid)?.bars)
}

/// Detects the feel of a MIDI groove from its imported event timing.
///
/// # Errors
///
/// Returns [`MidiGrooveError::Midi`] if the bytes are not readable MIDI.
pub fn detect_midi_groove_feel(bytes: &[u8]) -> Result<MidiGrooveFeel, MidiGrooveError> {
    let document = midi::read(bytes).map_err(|error| MidiGrooveError::Midi(error.to_string()))?;
    Ok(detect_feel(&document))
}

/// Moves kick hits to the nearest riff attack in the corresponding bar.
///
/// Other drum voices keep their original steps and MIDI identities, including
/// the selected groove's timekeeping cymbal voice.
///
/// # Errors
///
/// Returns [`MidiGrooveError::InvalidGrid`] if the groove's grid has no whole bar width and
/// [`MidiGrooveError::GridOverflow`] if a step offset overflows.
pub fn adapt_midi_groove_to_riff(
    groove: &MidiGroove,
    riff_onsets_by_bar: &[Vec<RiffOnset>],
    kick_voice: u8,
) -> Result<Vec<DrumHit>, MidiGrooveError> {
    let width = groove
        .grid
        .steps_per_bar()
        .map_err(|error| MidiGrooveError::InvalidGrid(error.to_string()))?;
    let width_u32 = u32::try_from(width).map_err(|_| MidiGrooveError::GridOverflow)?;
    let mut hits = Vec::new();
    for (bar_index, bar) in groove.bars.iter().enumerate() {
        let bar_offset = u32::try_from(bar_index)
            .ok()
            .and_then(|index| index.checked_mul(width_u32))
            .ok_or(MidiGrooveError::GridOverflow)?;
        let riff_onsets = riff_onsets_by_bar.get(bar_index).map(Vec::as_slice);
        for source in bar {
            let mut hit = source.clone();
            if hit.midi == kick_voice {
                if let Some(nearest) = riff_onsets
                    .into_iter()
                    .flatten()
                    .filter(|onset| onset.step < width_u32)
                    .min_by_key(|onset| (onset.step.abs_diff(hit.step), onset.step))
                {
                    hit.step = nearest.step;
                }
            }
            hit.step = bar_offset
                .checked_add(hit.step)
                .ok_or(MidiGrooveError::GridOverflow)?;
            hits.push(hit);
        }
    }
    hits.sort_by_key(|hit| (hit.step, hit.midi));
    Ok(hits)
}

/// Selects an indexed groove, or uses seeded generation only when `fallback` is supplied.
///
/// # Errors
///
/// Returns [`MidiGrooveError::NoMatchingGroove`] if no indexed groove matches `role` and the
/// meter of `grid` and `fallback` is `None`. If a groove matches, returns
/// [`MidiGrooveError::InvalidGrid`], [`MidiGrooveError::UnrepresentableGridStep`] or
/// [`MidiGrooveError::GridOverflow`] when it cannot be adapted to `grid`.
pub fn generate_from_midi_index(
    index: &MidiGrooveIndex,
    role: &str,
    grid: PatternGrid,
    tempo_bpm: u32,
    riff_onsets_by_bar: &[Vec<RiffOnset>],
    kick_voice: u8,
    fallback: Option<SeededGrooveFallback<'_>>,
) -> Result<MidiGrooveGeneration, MidiGrooveError> {
    if let Some(groove) = index.select(role, grid.meter, tempo_bpm) {
        let groove = adapt_resolution(groove, grid)?;
        let hits = adapt_midi_groove_to_riff(&groove, riff_onsets_by_bar, kick_voice)?;
        return Ok(MidiGrooveGeneration {
            hits,
            source: MidiGrooveSource::Indexed {
                groove_id: groove.id,
            },
        });
    }
    let Some(fallback) = fallback else {
        return Err(MidiGrooveError::NoMatchingGroove {
            role: role.to_string(),
            meter: grid.meter,
            tempo_bpm,
        });
    };
    Ok(MidiGrooveGeneration {
        hits: generate_seeded_hits(fallback.pattern, fallback.profile, fallback.seed),
        source: MidiGrooveSource::SeededFallback,
    })
}

struct ParsedMidiGroove {
    bars: Vec<Vec<DrumHit>>,
    feel: MidiGrooveFeel,
    timekeeping_voice: Option<u8>,
}

fn parse_midi_groove(bytes: &[u8], grid: PatternGrid) -> Result<ParsedMidiGroove, MidiGrooveError> {
    let width = grid
        .steps_per_bar()
        .map_err(|error| MidiGrooveError::InvalidGrid(error.to_string()))?;
    let document = midi::read(bytes).map_err(|error| MidiGrooveError::Midi(error.to_string()))?;
    let mut bars = vec![Vec::new(); document.master_bars.len()];
    let mut cymbal_counts = BTreeMap::<u8, usize>::new();
    let mut drum_note_count = 0;
    for bar_index in 0..document.master_bars.len() {
        let Some(bar) = document
            .master_bars
            .get(bar_index)
            .and_then(|master| master.bar_ids.first())
            .and_then(|id| u32::try_from(*id).ok())
            .and_then(|id| document.bars.iter().find(|bar| bar.id == id))
        else {
            continue;
        };
        let Some(voice) = bar.voices.first() else {
            continue;
        };
        let positions = inspect::beat_positions(&document, bar_index, 0, 0);
        for (beat_index, beat) in voice.beats.iter().enumerate() {
            let Some(position) = positions.get(beat_index) else {
                continue;
            };
            if beat.notes.is_empty() {
                continue;
            }
            let (target_bar, step) = project_onset(bar_index, position.onset, grid, width)?;
            while bars.len() <= target_bar {
                bars.push(Vec::new());
            }
            for note in &beat.notes {
                let Some(raw) = note.midi else {
                    continue;
                };
                let Ok(midi) = u8::try_from(raw) else {
                    continue;
                };
                let Some((midi, role)) = lookup(i32::from(midi)) else {
                    continue;
                };
                drum_note_count += 1;
                if matches!(role, DrumRole::HiHat | DrumRole::Ride | DrumRole::Cymbal) {
                    *cymbal_counts.entry(midi).or_insert(0) += 1;
                }
                bars[target_bar].push(DrumHit::new(midi, step));
            }
        }
    }
    if drum_note_count == 0 {
        return Err(MidiGrooveError::NoDrumNotes);
    }
    for bar in &mut bars {
        bar.sort_by_key(|hit| (hit.step, hit.midi));
    }
    let timekeeping_voice = cymbal_counts
        .iter()
        .max_by_key(|(midi, count)| (**count, std::cmp::Reverse(**midi)))
        .map(|(midi, _)| *midi);
    Ok(ParsedMidiGroove {
        bars,
        feel: detect_feel(&document),
        timekeeping_voice,
    })
}

fn detect_feel(document: &crate::Document) -> MidiGrooveFeel {
    let feels = (0..document.master_bars.len())
        .map(
            |bar_index| match inspect::detect_bar_feel(document, bar_index, 0, 0) {
                BarFeel::Straight => MidiGrooveFeel::Straight,
                BarFeel::Triplet => MidiGrooveFeel::Triplet,
                BarFeel::Swung => MidiGrooveFeel::Swung,
                BarFeel::Unsupported => MidiGrooveFeel::Unsupported,
            },
        )
        .collect::<BTreeSet<_>>();
    match (feels.len(), feels.first()) {
        (0, _) => MidiGrooveFeel::Straight,
        (1, Some(feel)) => *feel,
        _ => MidiGrooveFeel::Mixed,
    }
}

fn project_onset(
    source_bar: usize,
    onset: Fraction,
    grid: PatternGrid,
    width: usize,
) -> Result<(usize, u32), MidiGrooveError> {
    let (meter_numerator, meter_denominator) = grid.meter;
    let scale = u128::try_from(width).map_err(|_| MidiGrooveError::GridOverflow)?;
    let absolute_numerator = u128::try_from(source_bar)
        .map_err(|_| MidiGrooveError::GridOverflow)?
        .checked_mul(u128::from(onset.denominator))
        .and_then(|value| value.checked_add(u128::from(onset.numerator)))
        .ok_or(MidiGrooveError::GridOverflow)?;
    let scaled_numerator = absolute_numerator
        .checked_mul(u128::from(meter_denominator))
        .ok_or(MidiGrooveError::GridOverflow)?;
    let scaled_denominator = u128::from(onset.denominator)
        .checked_mul(u128::from(meter_numerator))
        .ok_or(MidiGrooveError::GridOverflow)?;
    if scaled_denominator == 0 {
        return Err(MidiGrooveError::UnrepresentableOnset {
            numerator: onset.numerator,
            denominator: onset.denominator,
        });
    }
    let target_bar = scaled_numerator / scaled_denominator;
    let step_numerator = (scaled_numerator % scaled_denominator)
        .checked_mul(scale)
        .ok_or(MidiGrooveError::GridOverflow)?;
    if step_numerator % scaled_denominator != 0 {
        return Err(MidiGrooveError::UnrepresentableOnset {
            numerator: onset.numerator,
            denominator: onset.denominator,
        });
    }
    let step = step_numerator / scaled_denominator;
    if step >= scale {
        return Err(MidiGrooveError::UnrepresentableOnset {
            numerator: onset.numerator,
            denominator: onset.denominator,
        });
    }
    Ok((
        usize::try_from(target_bar).map_err(|_| MidiGrooveError::GridOverflow)?,
        u32::try_from(step).map_err(|_| MidiGrooveError::GridOverflow)?,
    ))
}

fn adapt_resolution(
    groove: &MidiGroove,
    target: PatternGrid,
) -> Result<MidiGroove, MidiGrooveError> {
    let source_width = groove
        .grid
        .steps_per_bar()
        .map_err(|error| MidiGrooveError::InvalidGrid(error.to_string()))?;
    let target_width = target
        .steps_per_bar()
        .map_err(|error| MidiGrooveError::InvalidGrid(error.to_string()))?;
    let source_width = u64::try_from(source_width).map_err(|_| MidiGrooveError::GridOverflow)?;
    let target_width_u64 =
        u64::try_from(target_width).map_err(|_| MidiGrooveError::GridOverflow)?;
    let target_width_u32 =
        u32::try_from(target_width).map_err(|_| MidiGrooveError::GridOverflow)?;
    let mut bars = Vec::with_capacity(groove.bars.len());
    for (bar_index, bar) in groove.bars.iter().enumerate() {
        let mut target_bar = Vec::with_capacity(bar.len());
        for source in bar {
            let scaled = u64::from(source.step)
                .checked_mul(target_width_u64)
                .ok_or(MidiGrooveError::GridOverflow)?;
            if scaled % source_width != 0 {
                return Err(MidiGrooveError::UnrepresentableGridStep {
                    bar: bar_index,
                    step: source.step,
                });
            }
            let step =
                u32::try_from(scaled / source_width).map_err(|_| MidiGrooveError::GridOverflow)?;
            if step >= target_width_u32 {
                return Err(MidiGrooveError::UnrepresentableGridStep {
                    bar: bar_index,
                    step: source.step,
                });
            }
            let mut hit = source.clone();
            hit.step = step;
            target_bar.push(hit);
        }
        bars.push(target_bar);
    }
    Ok(MidiGroove {
        id: groove.id.clone(),
        role: groove.role.clone(),
        grid: target,
        tempo_bpm: groove.tempo_bpm,
        feel: groove.feel,
        bars,
        timekeeping_voice: groove.timekeeping_voice,
        genre: groove.genre.clone(),
    })
}

/// Why MIDI groove conversion or selection could not complete.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MidiGrooveError {
    /// Groove identifiers cannot be empty.
    #[error("MIDI groove identifier cannot be empty")]
    EmptyId,
    /// Caller-supplied roles cannot be empty.
    #[error("MIDI groove role cannot be empty")]
    EmptyRole,
    /// Groove tempo must be positive.
    #[error("MIDI groove tempo must be positive")]
    InvalidTempo,
    /// The index already contains this stable identifier.
    #[error("MIDI groove identifier {id:?} is already indexed")]
    DuplicateId {
        /// The identifier that is already indexed.
        id: String,
    },
    /// The score has no recognized General MIDI percussion notes.
    #[error("MIDI file contains no recognized General MIDI drum notes")]
    NoDrumNotes,
    /// No indexed groove matches the requested role and meter.
    #[error("no MIDI groove matches role {role:?}, meter {meter:?}, near {tempo_bpm} BPM")]
    NoMatchingGroove {
        /// Requested role.
        role: String,
        /// Requested meter as `(numerator, denominator)`.
        meter: (u32, u32),
        /// Requested tempo in beats per minute.
        tempo_bpm: u32,
    },
    /// The existing MIDI reader rejected the supplied bytes.
    #[error("could not parse MIDI groove: {0}")]
    Midi(String),
    /// The supplied grid is invalid.
    #[error("invalid MIDI groove grid: {0}")]
    InvalidGrid(String),
    /// An onset cannot be placed exactly on the requested grid.
    #[error("MIDI onset {numerator}/{denominator} cannot be represented on the selected grid")]
    UnrepresentableOnset {
        /// Numerator of the onset position as a fraction of a bar.
        numerator: u64,
        /// Denominator of the onset position as a fraction of a bar.
        denominator: u64,
    },
    /// A source event cannot be represented at the requested grid resolution.
    #[error("MIDI event at bar {bar}, step {step} cannot be represented on the target grid")]
    UnrepresentableGridStep {
        /// Zero-based bar of the source event.
        bar: usize,
        /// Zero-based grid step of the source event within its bar.
        step: u32,
    },
    /// Checked grid arithmetic exceeded its integer range.
    #[error("MIDI groove grid arithmetic overflowed")]
    GridOverflow,
}

#[cfg(test)]
#[path = "midi_groove_test.rs"]
mod tests;
