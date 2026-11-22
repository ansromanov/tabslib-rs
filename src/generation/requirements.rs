//! Hard requirements a generated document must not violate.
//!
//! The module owns [`Requirement`], [`Violation`] and [`check`]. Unlike
//! [`assessment`](super::assessment), which reports ratios a caller compares
//! against its own samples, a requirement is a boolean with a location: an
//! empty result means the document is releasable, and a non-empty one names
//! what to regenerate.
//!
//! The distinction matters because repairing a violated document in place is
//! how material that reads green on every metric still fails by ear. Each
//! check therefore reports; it never edits.
//!
//! Requirement identifiers are stable and match the written specification the
//! callers share. Every requirement documents the meaning of
//! [`Violation::detail`], because a violation with no measurement cannot be
//! ranked or regression-tested.

use std::collections::BTreeMap;

use super::assessment::Region;
use crate::inspect::{self, Fraction, TrackKind};
use crate::model::{sounding_pitch, Bar, Document, Technique};

/// Largest interval, in semitones, allowed between adjacent sounding strings
/// before a voicing counts as spanning a gap.
pub const ADJACENT_STRING_GAP: u32 = 1;
/// Shapes whose string gaps are idiomatic rather than defective, as semitone
/// offsets above the lowest sounding note: root, fifth, octave, twelfth and
/// double octave.
pub const SHAPE_INTERVALS: [i32; 5] = [0, 7, 12, 19, 24];
/// Fret distance at which a hand position change stops being a shift.
pub const HAND_POSITION_LEAP: i32 = 7;
/// Most fretted, non-bass tracks a releasable document may carry.
pub const MAX_FRETTED_TRACKS: usize = 2;
/// Share of identical bars, in percent, above which two tracks are clones.
pub const CLONE_PERCENT: i64 = 90;
/// Tempo above which subdivisions shorter than a sixteenth stop being playable.
pub const FAST_SUBDIVISION_BPM: f64 = 80.0;
/// String-index changes per second a live player can sustain.
pub const STRING_MOTION_PER_SECOND: f64 = 12.0;
/// Tempo assumed when a document carries no tempo map.
pub const DEFAULT_BPM: f64 = 120.0;
/// Bend units, as the payload writes them, for one semitone.
pub const BEND_SEMITONE: i32 = 50;
/// Bend units, as the payload writes them, for one whole tone.
pub const BEND_WHOLE_TONE: i32 = 100;

/// A property a releasable document must satisfy.
///
/// Identifiers are stable across releases; the set is open to additions at the
/// end, never to renumbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Requirement {
    /// H1 -- every sounding note lies in the whole-document detected scale.
    ///
    /// `detail` is the offending MIDI pitch.
    ScaleConformance,
    /// H2 -- no note is staccato or let-ring.
    ///
    /// `detail` is `0` for staccato and `1` for let-ring.
    SustainTechnique,
    /// H3 -- no chorded beat spans non-adjacent strings, except shapes built
    /// from roots, fifths and octaves.
    ///
    /// `detail` is the largest string gap in the beat.
    AdjacentStrings,
    /// H4 -- no hand-position leap of [`HAND_POSITION_LEAP`] frets or more
    /// between consecutive attacks faster than an eighth, unless the chord
    /// shape is preserved.
    ///
    /// `detail` is the leap in frets.
    HandPositionLeap,
    /// H5 -- no fully silent bar inside a section that sounds elsewhere.
    ///
    /// `detail` is the number of tracks silent at that bar.
    SilentBar,
    /// H6 -- at most [`MAX_FRETTED_TRACKS`] fretted, non-bass, non-percussion
    /// tracks.
    ///
    /// `detail` is the number of such tracks found.
    FrettedTrackCount,
    /// H7 -- no track repeats more than [`CLONE_PERCENT`] percent of another
    /// track's bars verbatim.
    ///
    /// `detail` is the identical-bar share in percent.
    TrackDuplication,
    /// H9 -- no beat shorter than a sixteenth above [`FAST_SUBDIVISION_BPM`].
    ///
    /// `detail` is the tempo at that bar, rounded to whole beats per minute.
    FastSubdivision,
    /// H10 -- every voice fills exactly its bar's meter capacity.
    ///
    /// `detail` is the written duration minus the capacity, in sixty-fourths
    /// of a whole note; it is negative when the voice is short.
    BarCapacity,
    /// H11 -- every bend point is a whole number of semitones and at most a
    /// whole tone, and the destination lands in the detected scale.
    ///
    /// `detail` is the offending bend value in the payload's own units, where
    /// [`BEND_WHOLE_TONE`] is a whole tone.
    BendInterval,
    /// H13 -- no two tracks share a name.
    ///
    /// `detail` is the number of tracks sharing that name.
    TrackNameCollision,
    /// H14 -- no transition exceeds [`STRING_MOTION_PER_SECOND`] string-index
    /// changes per second.
    ///
    /// `detail` is the observed rate, rounded to whole changes per second.
    StringMotionRate,
}

impl Requirement {
    /// Stable identifier, such as `H1`.
    pub fn id(self) -> &'static str {
        match self {
            Self::ScaleConformance => "H1",
            Self::SustainTechnique => "H2",
            Self::AdjacentStrings => "H3",
            Self::HandPositionLeap => "H4",
            Self::SilentBar => "H5",
            Self::FrettedTrackCount => "H6",
            Self::TrackDuplication => "H7",
            Self::FastSubdivision => "H9",
            Self::BarCapacity => "H10",
            Self::BendInterval => "H11",
            Self::TrackNameCollision => "H13",
            Self::StringMotionRate => "H14",
        }
    }

    /// Stable machine-readable name, matching the assessment diagnostic style.
    pub fn code(self) -> &'static str {
        match self {
            Self::ScaleConformance => "note-outside-scale",
            Self::SustainTechnique => "sustain-technique",
            Self::AdjacentStrings => "non-adjacent-strings",
            Self::HandPositionLeap => "hand-position-leap",
            Self::SilentBar => "silent-bar",
            Self::FrettedTrackCount => "too-many-fretted-tracks",
            Self::TrackDuplication => "duplicate-track",
            Self::FastSubdivision => "subdivision-too-fast",
            Self::BarCapacity => "bar-capacity",
            Self::BendInterval => "bend-interval",
            Self::TrackNameCollision => "duplicate-track-name",
            Self::StringMotionRate => "string-motion-too-fast",
        }
    }
}

/// One located failure of one requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Violation {
    /// Requirement that failed.
    pub requirement: Requirement,
    /// Region responsible for the failure.
    pub region: Region,
    /// Measurement whose meaning is documented on the requirement.
    pub detail: i64,
}

impl Violation {
    /// Stable identifier of the failed requirement, such as `H1`.
    pub fn id(self) -> &'static str {
        self.requirement.id()
    }
}

/// Checks every hard requirement and returns the failures in requirement order.
///
/// An empty result means the document violates nothing this module can see. It
/// is not a claim that the document is musically good; that is what
/// [`assessment`](super::assessment) measures.
///
/// Section-marker ordering (H12) has no document-level expression: the model
/// stores a marker on a master bar and the payload writer is what sequences it
/// before the bar list, so it is covered by the adapter's own tests rather
/// than here.
///
/// ```
/// use tabslib::{fixtures, generation::requirements};
///
/// assert!(requirements::check(&fixtures::power_chords()).is_empty());
/// ```
pub fn check(doc: &Document) -> Vec<Violation> {
    let mut out = Vec::new();
    check_scale_conformance(doc, &mut out);
    check_sustain_techniques(doc, &mut out);
    check_adjacent_strings(doc, &mut out);
    check_hand_position(doc, &mut out);
    check_silent_bars(doc, &mut out);
    check_fretted_track_count(doc, &mut out);
    check_track_duplication(doc, &mut out);
    check_fast_subdivisions(doc, &mut out);
    check_bar_capacity(doc, &mut out);
    check_bend_intervals(doc, &mut out);
    check_track_name_collisions(doc, &mut out);
    check_string_motion(doc, &mut out);
    out
}

/// Checks only the local, note-level requirements for one generated track.
pub fn check_track(doc: &Document, track: usize) -> Vec<Violation> {
    if track >= doc.tracks.len() {
        return Vec::new();
    }
    check(doc)
        .into_iter()
        .filter(|violation| {
            violation.region.track == Some(track)
                && matches!(
                    violation.requirement,
                    Requirement::ScaleConformance
                        | Requirement::SustainTechnique
                        | Requirement::AdjacentStrings
                        | Requirement::HandPositionLeap
                        | Requirement::FastSubdivision
                        | Requirement::BarCapacity
                        | Requirement::BendInterval
                        | Requirement::StringMotionRate
                )
        })
        .collect()
}

fn bar_of(doc: &Document, bar_index: usize, track_index: usize) -> Option<&Bar> {
    let id = *doc.master_bars.get(bar_index)?.bar_ids.get(track_index)?;
    doc.bars.iter().find(|bar| bar.id == id as u32)
}

fn tempo_at(doc: &Document, bar_index: usize) -> f64 {
    doc.tempo_map
        .iter()
        .filter(|change| change.at_bar <= bar_index)
        .max_by_key(|change| change.at_bar)
        .map(|change| change.bpm)
        .unwrap_or(DEFAULT_BPM)
}

fn is_pitched(doc: &Document, track_index: usize) -> bool {
    !matches!(
        inspect::classify_track(doc, track_index).map(|(kind, _)| kind),
        Some(TrackKind::Percussion) | None
    )
}

fn region(bar: usize, track: usize, voice: usize, beat: usize) -> Region {
    Region {
        bar,
        track: Some(track),
        voice: Some(voice),
        beat: Some(beat),
    }
}

/// Visits every beat of every pitched track, in document order.
fn for_each_pitched_beat<F>(doc: &Document, mut visit: F)
where
    F: FnMut(usize, usize, usize, usize, &crate::model::Beat),
{
    for track_index in 0..doc.tracks.len() {
        if !is_pitched(doc, track_index) {
            continue;
        }
        for bar_index in 0..doc.master_bars.len() {
            let Some(bar) = bar_of(doc, bar_index, track_index) else {
                continue;
            };
            for (voice_index, voice) in bar.voices.iter().enumerate() {
                for (beat_index, beat) in voice.beats.iter().enumerate() {
                    visit(track_index, bar_index, voice_index, beat_index, beat);
                }
            }
        }
    }
}

fn check_scale_conformance(doc: &Document, out: &mut Vec<Violation>) {
    let Some(estimate) = inspect::determine_key(doc) else {
        return;
    };
    let allowed = estimate.mode.pitch_classes(estimate.key.tonic);
    for_each_pitched_beat(doc, |track, bar, voice, beat_index, beat| {
        for note in &beat.notes {
            let Some(midi) = note.midi else { continue };
            if !allowed.contains(&midi.rem_euclid(12)) {
                out.push(Violation {
                    requirement: Requirement::ScaleConformance,
                    region: region(bar, track, voice, beat_index),
                    detail: i64::from(midi),
                });
            }
        }
    });
}

fn check_sustain_techniques(doc: &Document, out: &mut Vec<Violation>) {
    for_each_pitched_beat(doc, |track, bar, voice, beat_index, beat| {
        for note in &beat.notes {
            for technique in &note.techniques {
                let detail = match technique {
                    Technique::Staccato => 0,
                    Technique::LetRing => 1,
                    _ => continue,
                };
                out.push(Violation {
                    requirement: Requirement::SustainTechnique,
                    region: region(bar, track, voice, beat_index),
                    detail,
                });
            }
        }
    });
}

/// Strings sounding in a beat, ascending, paired with their sounding pitches.
fn sounding_strings(
    doc: &Document,
    track_index: usize,
    beat: &crate::model::Beat,
) -> Vec<(u32, i32)> {
    let Some(track) = doc.tracks.get(track_index) else {
        return Vec::new();
    };
    let mut strings: Vec<(u32, i32)> = beat
        .notes
        .iter()
        .filter_map(|note| {
            let string = note.string?;
            let pitch = note
                .midi
                .or_else(|| sounding_pitch(&track.tuning, string, note.fret?))?;
            Some((string, pitch))
        })
        .collect();
    strings.sort_unstable();
    strings
}

/// Whether a voicing is built only from roots, fifths and octaves, which the
/// format's own shapes routinely spread across a skipped string.
fn is_shape_voicing(strings: &[(u32, i32)]) -> bool {
    let Some((_, lowest)) = strings.first() else {
        return true;
    };
    strings
        .iter()
        .all(|(_, pitch)| SHAPE_INTERVALS.contains(&(pitch - lowest)))
}

fn check_adjacent_strings(doc: &Document, out: &mut Vec<Violation>) {
    for_each_pitched_beat(doc, |track, bar, voice, beat_index, beat| {
        let strings = sounding_strings(doc, track, beat);
        if strings.len() < 2 || is_shape_voicing(&strings) {
            return;
        }
        let widest = strings
            .windows(2)
            .map(|pair| pair[1].0.saturating_sub(pair[0].0))
            .max()
            .unwrap_or(0);
        if widest > ADJACENT_STRING_GAP {
            out.push(Violation {
                requirement: Requirement::AdjacentStrings,
                region: region(bar, track, voice, beat_index),
                detail: i64::from(widest),
            });
        }
    });
}

/// Fret offsets above the lowest fretted note, which identify a chord shape
/// independently of where on the neck it is played.
fn shape_offsets(beat: &crate::model::Beat) -> Option<(i32, Vec<i32>)> {
    let mut frets: Vec<i32> = beat.notes.iter().filter_map(|note| note.fret).collect();
    frets.sort_unstable();
    let lowest = *frets.first()?;
    Some((lowest, frets.iter().map(|fret| fret - lowest).collect()))
}

fn check_hand_position(doc: &Document, out: &mut Vec<Violation>) {
    let eighth = Fraction::new(1, 8);
    for track_index in 0..doc.tracks.len() {
        if !is_pitched(doc, track_index) {
            continue;
        }
        for bar_index in 0..doc.master_bars.len() {
            let Some(bar) = bar_of(doc, bar_index, track_index) else {
                continue;
            };
            for (voice_index, voice) in bar.voices.iter().enumerate() {
                for (beat_index, pair) in voice.beats.windows(2).enumerate() {
                    let (first, second) = (&pair[0], &pair[1]);
                    if inspect::rhythm_duration(first.rhythm) >= eighth {
                        continue;
                    }
                    let (Some((from, from_shape)), Some((to, to_shape))) =
                        (shape_offsets(first), shape_offsets(second))
                    else {
                        continue;
                    };
                    let leap = (to - from).abs();
                    // A single note has no shape to preserve: its offset list
                    // is always [0], so exempting it would exempt every leap.
                    let preserved =
                        from_shape.len() > 1 && to_shape.len() > 1 && from_shape == to_shape;
                    if leap < HAND_POSITION_LEAP || preserved {
                        continue;
                    }
                    out.push(Violation {
                        requirement: Requirement::HandPositionLeap,
                        region: region(bar_index, track_index, voice_index, beat_index + 1),
                        detail: i64::from(leap),
                    });
                }
            }
        }
    }
}

fn bar_sounds(doc: &Document, bar_index: usize, track_index: usize) -> bool {
    bar_of(doc, bar_index, track_index).is_some_and(|bar| {
        bar.voices
            .iter()
            .any(|voice| voice.beats.iter().any(|beat| !beat.notes.is_empty()))
    })
}

/// Half-open master-bar ranges delimited by section markers.
fn section_ranges(doc: &Document) -> Vec<(usize, usize)> {
    let mut starts: Vec<usize> = doc
        .master_bars
        .iter()
        .enumerate()
        .filter(|(_, master)| master.section.is_some())
        .map(|(index, _)| index)
        .collect();
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    let end = doc.master_bars.len();
    starts
        .iter()
        .enumerate()
        .map(|(i, start)| (*start, starts.get(i + 1).copied().unwrap_or(end)))
        .collect()
}

fn check_silent_bars(doc: &Document, out: &mut Vec<Violation>) {
    for (start, end) in section_ranges(doc) {
        let sounds_somewhere =
            (start..end).any(|bar| (0..doc.tracks.len()).any(|track| bar_sounds(doc, bar, track)));
        if !sounds_somewhere {
            continue;
        }
        for bar in start..end {
            if (0..doc.tracks.len()).any(|track| bar_sounds(doc, bar, track)) {
                continue;
            }
            let Ok(silent) = i64::try_from(doc.tracks.len()) else {
                continue;
            };
            out.push(Violation {
                requirement: Requirement::SilentBar,
                region: Region {
                    bar,
                    track: None,
                    voice: None,
                    beat: None,
                },
                detail: silent,
            });
        }
    }
}

fn check_fretted_track_count(doc: &Document, out: &mut Vec<Violation>) {
    let fretted = (0..doc.tracks.len())
        .filter(|index| {
            matches!(
                inspect::classify_track(doc, *index).map(|(kind, _)| kind),
                Some(TrackKind::Other)
            )
        })
        .count();
    if fretted <= MAX_FRETTED_TRACKS {
        return;
    }
    let Ok(detail) = i64::try_from(fretted) else {
        return;
    };
    out.push(Violation {
        requirement: Requirement::FrettedTrackCount,
        region: Region {
            bar: 0,
            track: None,
            voice: None,
            beat: None,
        },
        detail,
    });
}

/// A bar's written content, as the string a comparison can be made on.
fn bar_signature(doc: &Document, bar_index: usize, track_index: usize) -> Option<String> {
    let bar = bar_of(doc, bar_index, track_index)?;
    let mut signature = String::new();
    for voice in &bar.voices {
        for beat in &voice.beats {
            let (num, den) = beat.rhythm.as_fraction();
            signature.push_str(&format!("{num}/{den}:"));
            for note in &beat.notes {
                signature.push_str(&format!(
                    "{:?}.{:?}.{:?};",
                    note.string, note.fret, note.midi
                ));
            }
            signature.push('|');
        }
        signature.push('&');
    }
    Some(signature)
}

fn check_track_duplication(doc: &Document, out: &mut Vec<Violation>) {
    let sounding: Vec<usize> = (0..doc.tracks.len())
        .filter(|index| {
            !matches!(
                inspect::classify_track(doc, *index).map(|(kind, _)| kind),
                Some(TrackKind::Empty) | None
            )
        })
        .collect();
    for (position, later) in sounding.iter().enumerate() {
        for earlier in &sounding[..position] {
            let mut compared = 0i64;
            let mut identical = 0i64;
            for bar in 0..doc.master_bars.len() {
                let (Some(left), Some(right)) = (
                    bar_signature(doc, bar, *earlier),
                    bar_signature(doc, bar, *later),
                ) else {
                    continue;
                };
                compared += 1;
                if left == right {
                    identical += 1;
                }
            }
            if compared == 0 {
                continue;
            }
            let percent = identical * 100 / compared;
            if percent > CLONE_PERCENT {
                out.push(Violation {
                    requirement: Requirement::TrackDuplication,
                    region: Region {
                        bar: 0,
                        track: Some(*later),
                        voice: None,
                        beat: None,
                    },
                    detail: percent,
                });
            }
        }
    }
}

fn check_fast_subdivisions(doc: &Document, out: &mut Vec<Violation>) {
    let sixteenth = Fraction::new(1, 16);
    for_each_pitched_beat(doc, |track, bar, voice, beat_index, beat| {
        if inspect::rhythm_duration(beat.rhythm) >= sixteenth {
            return;
        }
        let bpm = tempo_at(doc, bar);
        if bpm <= FAST_SUBDIVISION_BPM {
            return;
        }
        out.push(Violation {
            requirement: Requirement::FastSubdivision,
            region: region(bar, track, voice, beat_index),
            detail: bpm.round() as i64,
        });
    });
}

fn check_bar_capacity(doc: &Document, out: &mut Vec<Violation>) {
    for entry in inspect::bar_integrity(doc) {
        let written = entry.duration.numerator as i64 * 64 / entry.duration.denominator as i64;
        let capacity = entry.capacity.numerator as i64 * 64 / entry.capacity.denominator as i64;
        out.push(Violation {
            requirement: Requirement::BarCapacity,
            region: Region {
                bar: entry.bar_index,
                track: Some(entry.track_index),
                voice: Some(entry.voice_index),
                beat: None,
            },
            detail: written - capacity,
        });
    }
}

fn check_bend_intervals(doc: &Document, out: &mut Vec<Violation>) {
    let scale =
        inspect::determine_key(doc).map(|estimate| estimate.mode.pitch_classes(estimate.key.tonic));
    for_each_pitched_beat(doc, |track, bar, voice, beat_index, beat| {
        for note in &beat.notes {
            for technique in &note.techniques {
                let Technique::Bend {
                    origin,
                    middle,
                    dest,
                } = technique
                else {
                    continue;
                };
                for (value, lands) in [(*origin, false), (*middle, false), (*dest, true)] {
                    let playable = value % BEND_SEMITONE == 0 && value.abs() <= BEND_WHOLE_TONE;
                    // Only the destination has to be diatonic. The origin and
                    // the midpoint are passed through on the way there, and a
                    // bend that rises through a semitone is ordinary.
                    let in_scale = match (lands, scale, note.midi) {
                        (true, Some(classes), Some(midi)) => {
                            let bent = midi + value / BEND_SEMITONE;
                            classes.contains(&bent.rem_euclid(12))
                        }
                        _ => true,
                    };
                    if playable && in_scale {
                        continue;
                    }
                    out.push(Violation {
                        requirement: Requirement::BendInterval,
                        region: region(bar, track, voice, beat_index),
                        detail: i64::from(value),
                    });
                }
            }
        }
    });
}

fn check_track_name_collisions(doc: &Document, out: &mut Vec<Violation>) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for track in &doc.tracks {
        *counts.entry(track.name.as_str()).or_default() += 1;
    }
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, track) in doc.tracks.iter().enumerate() {
        let total = counts.get(track.name.as_str()).copied().unwrap_or(1);
        let occurrence = seen.entry(track.name.as_str()).or_default();
        *occurrence += 1;
        if total < 2 || *occurrence == 1 {
            continue;
        }
        let Ok(detail) = i64::try_from(total) else {
            continue;
        };
        out.push(Violation {
            requirement: Requirement::TrackNameCollision,
            region: Region {
                bar: 0,
                track: Some(index),
                voice: None,
                beat: None,
            },
            detail,
        });
    }
}

/// Seconds a written duration occupies at a tempo, where the tempo counts
/// quarter notes.
fn duration_seconds(duration: Fraction, bpm: f64) -> f64 {
    if bpm <= 0.0 || duration.numerator == 0 {
        return 0.0;
    }
    let whole_notes = duration.numerator as f64 / duration.denominator as f64;
    whole_notes * 4.0 * 60.0 / bpm
}

fn lowest_string(beat: &crate::model::Beat) -> Option<u32> {
    beat.notes.iter().filter_map(|note| note.string).min()
}

fn check_string_motion(doc: &Document, out: &mut Vec<Violation>) {
    for track_index in 0..doc.tracks.len() {
        if !is_pitched(doc, track_index) {
            continue;
        }
        for bar_index in 0..doc.master_bars.len() {
            let Some(bar) = bar_of(doc, bar_index, track_index) else {
                continue;
            };
            let bpm = tempo_at(doc, bar_index);
            for (voice_index, voice) in bar.voices.iter().enumerate() {
                for (beat_index, pair) in voice.beats.windows(2).enumerate() {
                    let (Some(from), Some(to)) = (lowest_string(&pair[0]), lowest_string(&pair[1]))
                    else {
                        continue;
                    };
                    let seconds = duration_seconds(inspect::rhythm_duration(pair[0].rhythm), bpm);
                    if seconds <= 0.0 {
                        continue;
                    }
                    let changes = from.abs_diff(to);
                    if changes == 0 {
                        continue;
                    }
                    let rate = f64::from(changes) / seconds;
                    if rate <= STRING_MOTION_PER_SECOND {
                        continue;
                    }
                    out.push(Violation {
                        requirement: Requirement::StringMotionRate,
                        region: region(bar_index, track_index, voice_index, beat_index + 1),
                        detail: rate.round() as i64,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "requirements_test.rs"]
mod tests;
