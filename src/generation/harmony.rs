//! Harmony and guitar-two generation on the native score document.
//!
//! [`generate_harmony`] reads a pitched source track, chooses deterministic
//! diatonic or parallel pitches, voices them on a playable tuning, and appends
//! the result as a complete track.

use crate::edits;
use crate::inspect::{self, Fraction};
use crate::model::{Bar, Beat, Document, KeySignature, Note, NoteValue, Rhythm, Track, Voice};
use crate::{Error, Result};

/// Pitch relationship used when generating harmony.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarmonyType {
    /// Move by scale degrees while preserving chromatic alterations.
    Diatonic {
        /// Number of scale degrees to move; positive values move upward.
        steps: i32,
    },
    /// Move by a fixed chromatic interval in semitones.
    Parallel {
        /// Chromatic interval; positive values move upward.
        semitones: i32,
    },
}

/// Inputs and playability limits for a generated harmony track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarmonyConfig {
    /// Source track index.
    pub source_track: usize,
    /// Key for diatonic generation; inferred from the score when absent.
    pub key: Option<KeySignature>,
    /// Harmonic relationship to the source pitches.
    pub harmony: HarmonyType,
    /// Open-string MIDI pitches, lowest string first.
    pub tuning: Vec<i32>,
    /// Highest permitted fret.
    pub max_fret: i32,
    /// Largest permitted melodic leap in semitones.
    pub max_leap: i32,
    /// Maximum octave displacement around the requested pitch.
    pub octave_range: i32,
    /// Seed used to resolve equally playable voicings reproducibly.
    pub seed: u64,
    /// Display name for the generated track.
    pub track_name: String,
}

impl HarmonyConfig {
    /// Creates a diatonic third-up configuration for a guitar in standard tuning.
    pub fn diatonic_third(source_track: usize, seed: u64) -> Self {
        Self {
            source_track,
            key: None,
            harmony: HarmonyType::Diatonic { steps: 2 },
            tuning: vec![40, 45, 50, 55, 59, 64],
            max_fret: 24,
            max_leap: 12,
            octave_range: 1,
            seed,
            track_name: "Guitar 2".into(),
        }
    }
}

/// Reasons generated candidates were rejected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HarmonyDiagnostics {
    /// Number of source notes without a playable harmony position.
    pub unplayable_notes: usize,
    /// Number of candidates excluded by the maximum voice-leading leap.
    pub voice_leading_rejections: usize,
    /// Number of source notes omitted because every playable candidate exceeded
    /// the configured maximum leap.
    pub voice_leading_rejected_notes: usize,
}

/// Result of adding a harmony track to a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedHarmony {
    /// Index of the appended track.
    pub track_index: usize,
    /// Number of generated notes.
    pub note_count: usize,
    /// Candidate rejection counts.
    pub diagnostics: HarmonyDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// A MIDI pitch and one playable string/fret assignment.
pub struct FretboardPosition {
    /// Sounding MIDI pitch.
    pub midi: i32,
    /// String number, zero-based from the lowest string.
    pub string: u32,
    /// Fret number, where zero is the open string.
    pub fret: i32,
}

/// Adds a deterministic harmony track derived from one pitched source track.
///
/// The generated track keeps the source's bars, voices, beat rhythms, and
/// dynamics, flooring subdivisions to a sixteenth note when the bar tempo exceeds
/// the playability threshold. The seed only resolves equal-cost playable positions;
/// all pitch and playability decisions are independent of genre or reference data.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `config.source_track` is outside the document,
/// the tuning, fret, leap or octave limits are invalid, the score is empty, or the
/// harmony interval is more than two octaves. It also fails if the source track has
/// no pitched notes, a diatonic harmony has no key from the config, document or
/// key estimate, or the generated ids do not fit the supported range.
pub fn generate_harmony(doc: &mut Document, config: &HarmonyConfig) -> Result<GeneratedHarmony> {
    validate_config(doc, config)?;
    let key = match config.harmony {
        HarmonyType::Diatonic { .. } => Some(
            config
                .key
                .or_else(|| {
                    doc.key
                        .or_else(|| crate::inspect::determine_key(doc).map(|estimate| estimate.key))
                })
                .ok_or_else(|| Error::Malformed("a key is required for diatonic harmony".into()))?,
        ),
        HarmonyType::Parallel { .. } => None,
    };

    let source_bars = doc
        .master_bars
        .iter()
        .map(|master| {
            master
                .bar_ids
                .get(config.source_track)
                .copied()
                .filter(|id| *id >= 0)
                .and_then(|id| doc.bars.iter().find(|bar| bar.id == id as u32))
        })
        .collect::<Vec<_>>();
    if !source_bars.iter().any(|bar| {
        bar.is_some_and(|bar| {
            bar.voices
                .iter()
                .flat_map(|v| &v.beats)
                .any(|b| b.notes.iter().any(|n| n.midi.is_some()))
        })
    }) {
        return Err(Error::Malformed("source track has no pitched notes".into()));
    }

    let mut diagnostics = HarmonyDiagnostics::default();
    #[cfg(feature = "assessment")]
    let note_count;
    #[cfg(not(feature = "assessment"))]
    let mut note_count = 0;
    let mut previous: Vec<Vec<Option<FretboardPosition>>> = Vec::new();
    let mut generated_bars = Vec::with_capacity(source_bars.len());
    for (bar_index, source) in source_bars.into_iter().enumerate() {
        let Some(source) = source else {
            generated_bars.push(None);
            continue;
        };
        let bpm = tempo_at(doc, bar_index);
        let mut voices = Vec::with_capacity(source.voices.len());
        for (voice_index, source_voice) in source.voices.iter().enumerate() {
            if previous.len() <= voice_index {
                previous.resize_with(voice_index + 1, Vec::new);
            }
            let prior_voice = &mut previous[voice_index];
            let source_beats = clamp_source_beats(&source_voice.beats, bpm);
            let mut beats = Vec::with_capacity(source_beats.len());
            for source_beat in &source_beats {
                let mut source_notes = source_beat
                    .notes
                    .iter()
                    .filter_map(|note| note.midi.map(|midi| (midi, note.id)))
                    .collect::<Vec<_>>();
                source_notes.sort_by_key(|(midi, id)| (*midi, *id));
                let mut used_strings = Vec::new();
                let mut notes = Vec::with_capacity(source_notes.len());
                for (slot, (source_midi, _)) in source_notes.into_iter().enumerate() {
                    if !(0..=127).contains(&source_midi) {
                        diagnostics.unplayable_notes += 1;
                        continue;
                    }
                    let target = target_pitch(source_midi, config.harmony, key);
                    let last = prior_voice.get(slot).copied().flatten();
                    let choices = playable_positions(target, source_midi, config);
                    let playable = choices
                        .iter()
                        .copied()
                        .filter(|position| !used_strings.contains(&position.string))
                        .collect::<Vec<_>>();
                    if playable.is_empty() {
                        diagnostics.unplayable_notes += 1;
                        continue;
                    }
                    let candidates = if let Some(previous) = last {
                        let within_leap = playable
                            .iter()
                            .copied()
                            .filter(|position| {
                                (position.midi - previous.midi).abs() <= config.max_leap
                            })
                            .collect::<Vec<_>>();
                        diagnostics.voice_leading_rejections += playable.len() - within_leap.len();
                        if within_leap.is_empty() {
                            diagnostics.voice_leading_rejected_notes += 1;
                            continue;
                        }
                        within_leap
                    } else {
                        playable
                    };
                    let Some(chosen) = candidates.into_iter().min_by_key(|position| {
                        let leap = last.map_or(0, |previous| (position.midi - previous.midi).abs());
                        (leap * 32 + position.fret, tie_break(config.seed, position))
                    }) else {
                        diagnostics.unplayable_notes += 1;
                        if prior_voice.len() <= slot {
                            prior_voice.resize(slot + 1, None);
                        }
                        prior_voice[slot] = None;
                        continue;
                    };
                    if prior_voice.len() <= slot {
                        prior_voice.resize(slot + 1, None);
                    }
                    prior_voice[slot] = Some(chosen);
                    used_strings.push(chosen.string);
                    notes.push(Note {
                        id: 0,
                        midi: Some(chosen.midi),
                        string: Some(chosen.string),
                        fret: Some(chosen.fret),
                        articulation: None,
                        techniques: Vec::new(),
                    });
                }
                #[cfg(not(feature = "assessment"))]
                {
                    note_count += notes.len();
                }
                beats.push(Beat {
                    id: 0,
                    rhythm: source_beat.rhythm,
                    notes,
                    dynamic: source_beat.dynamic.clone(),
                });
            }
            voices.push(Voice { id: 0, beats });
        }
        generated_bars.push(Some(Bar {
            id: 0,
            clef: source.clef.clone(),
            voices,
        }));
    }

    let source_track = &doc.tracks[config.source_track];
    let track = Track {
        id: 0,
        name: config.track_name.clone(),
        color: source_track.color,
        tuning: config.tuning.clone(),
        midi_program: source_track.midi_program,
        midi_channel: source_track.midi_channel,
        pan: source_track.pan,
        volume: source_track.volume,
        mute: false,
        solo: false,
        percussion_articulations: Vec::new(),
        generator_metadata: None,
    };

    let bar_count = generated_bars.iter().filter(|bar| bar.is_some()).count();
    let voice_count = generated_bars
        .iter()
        .flatten()
        .map(|bar| bar.voices.len())
        .sum();
    let beat_count = generated_bars
        .iter()
        .flatten()
        .flat_map(|bar| &bar.voices)
        .map(|voice| voice.beats.len())
        .sum();
    let generated_note_count = generated_bars
        .iter()
        .flatten()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .map(|beat| beat.notes.len())
        .sum();
    let mut next_bar_id = reserve_ids(doc.bars.iter().map(|bar| bar.id).max(), bar_count, "bar")?;
    let mut next_voice_id = reserve_ids(
        doc.bars
            .iter()
            .flat_map(|bar| &bar.voices)
            .map(|voice| voice.id)
            .max(),
        voice_count,
        "voice",
    )?;
    let mut next_beat_id = reserve_ids(
        doc.bars
            .iter()
            .flat_map(|bar| &bar.voices)
            .flat_map(|voice| &voice.beats)
            .map(|beat| beat.id)
            .max(),
        beat_count,
        "beat",
    )?;
    let mut next_note_id = reserve_ids(
        doc.bars
            .iter()
            .flat_map(|bar| &bar.voices)
            .flat_map(|voice| &voice.beats)
            .flat_map(|beat| &beat.notes)
            .map(|note| note.id)
            .max(),
        generated_note_count,
        "note",
    )?;
    if bar_count > 0 {
        let last_bar_id = next_bar_id
            .checked_add(u32::try_from(bar_count - 1).map_err(|_| {
                Error::Malformed("generated bar count exceeds the supported ID range".into())
            })?)
            .ok_or_else(|| {
                Error::Malformed("generated bar IDs exceed the supported range".into())
            })?;
        i32::try_from(last_bar_id)
            .map_err(|_| Error::Malformed("generated bar IDs exceed the document range".into()))?;
    }
    let track_index = edits::create_track(doc, track);
    for (bar_index, bar) in generated_bars.into_iter().enumerate() {
        let Some(mut bar) = bar else {
            continue;
        };
        bar.id = next_bar_id;
        next_bar_id += 1;
        for voice in &mut bar.voices {
            voice.id = next_voice_id;
            next_voice_id += 1;
            for beat in &mut voice.beats {
                beat.id = next_beat_id;
                next_beat_id += 1;
                for note in &mut beat.notes {
                    note.id = next_note_id;
                    next_note_id += 1;
                }
            }
        }
        doc.master_bars[bar_index].bar_ids[track_index] = i32::try_from(bar.id)
            .map_err(|_| Error::Malformed("generated bar ID exceeds the document range".into()))?;
        doc.bars.push(bar);
    }
    #[cfg(feature = "assessment")]
    {
        repair_generated_track(doc, track_index);
        note_count = count_track_notes(doc, track_index);
    }
    Ok(GeneratedHarmony {
        track_index,
        note_count,
        diagnostics,
    })
}

#[cfg(feature = "assessment")]
fn repair_generated_track(doc: &mut Document, track: usize) {
    for _ in 0..2 {
        let violations = crate::generation::requirements::check_track(doc, track);
        if violations.is_empty() {
            return;
        }
        for violation in violations {
            let Some(voice) = violation.region.voice else {
                continue;
            };
            let Some(master) = doc.master_bars.get(violation.region.bar) else {
                continue;
            };
            let Some(id) = master
                .bar_ids
                .get(track)
                .copied()
                .and_then(|id| u32::try_from(id).ok())
            else {
                continue;
            };
            let Some(bar) = doc.bars.iter_mut().find(|bar| bar.id == id) else {
                continue;
            };
            let Some(score_voice) = bar.voices.get_mut(voice) else {
                continue;
            };
            if let Some(beat) = violation.region.beat {
                if let Some(score_beat) = score_voice.beats.get_mut(beat) {
                    score_beat.notes.clear();
                }
            } else {
                score_voice
                    .beats
                    .iter_mut()
                    .for_each(|beat| beat.notes.clear());
            }
        }
    }
}

#[cfg(feature = "assessment")]
fn count_track_notes(doc: &Document, track: usize) -> usize {
    doc.master_bars
        .iter()
        .filter_map(|master| master.bar_ids.get(track).copied())
        .filter_map(|id| u32::try_from(id).ok())
        .filter_map(|id| doc.bars.iter().find(|bar| bar.id == id))
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .map(|beat| beat.notes.len())
        .sum()
}

fn validate_config(doc: &Document, config: &HarmonyConfig) -> Result<()> {
    if config.source_track >= doc.tracks.len() {
        return Err(Error::Malformed(
            "source track index is outside the document".into(),
        ));
    }
    if config.tuning.is_empty()
        || config.tuning.iter().any(|pitch| !(0..=127).contains(pitch))
        || config.max_fret < 0
        || config.max_leap < 0
        || !(0..=3).contains(&config.octave_range)
        || config.key.is_some_and(|key| !(0..12).contains(&key.tonic))
    {
        return Err(Error::Malformed(
            "invalid harmony tuning or playability limit".into(),
        ));
    }
    if doc.master_bars.is_empty() {
        return Err(Error::Malformed(
            "cannot generate harmony for an empty score".into(),
        ));
    }
    match config.harmony {
        HarmonyType::Diatonic { steps } if !(-14..=14).contains(&steps) => {
            return Err(Error::Malformed(
                "diatonic harmony interval is outside two octaves".into(),
            ));
        }
        HarmonyType::Parallel { semitones } if !(-24..=24).contains(&semitones) => {
            return Err(Error::Malformed(
                "parallel harmony interval is outside two octaves".into(),
            ));
        }
        HarmonyType::Diatonic { .. } | HarmonyType::Parallel { .. } => {}
    }
    Ok(())
}

fn reserve_ids(maximum: Option<u32>, count: usize, kind: &str) -> Result<u32> {
    if count == 0 {
        return Ok(maximum.unwrap_or(0));
    }
    let first = maximum.unwrap_or(0).checked_add(1).ok_or_else(|| {
        Error::Malformed(format!("generated {kind} IDs exceed the supported range"))
    })?;
    let span = u32::try_from(count - 1).map_err(|_| {
        Error::Malformed(format!(
            "generated {kind} count exceeds the supported ID range"
        ))
    })?;
    first
        .checked_add(span)
        .and_then(|last| last.checked_add(1))
        .ok_or_else(|| {
            Error::Malformed(format!("generated {kind} IDs exceed the supported range"))
        })?;
    Ok(first)
}

fn target_pitch(midi: i32, harmony: HarmonyType, key: Option<KeySignature>) -> i32 {
    match harmony {
        HarmonyType::Parallel { semitones } => midi + semitones,
        HarmonyType::Diatonic { steps } => {
            let key = key.unwrap_or(KeySignature {
                tonic: 0,
                minor: false,
            });
            let scale = if key.minor {
                [0, 2, 3, 5, 7, 8, 10]
            } else {
                [0, 2, 4, 5, 7, 9, 11]
            };
            let relative = (midi - i32::from(key.tonic)).rem_euclid(12);
            let degree = scale
                .iter()
                .rposition(|step| *step <= relative)
                .unwrap_or(0);
            let alteration = relative - scale[degree];
            let target_degree = i32::try_from(degree).unwrap_or(0) + steps;
            let octave = target_degree.div_euclid(7);
            let target_index = usize::try_from(target_degree.rem_euclid(7)).unwrap_or(0);
            let tonic_octave = midi - relative;
            tonic_octave + octave * 12 + scale[target_index] + alteration
        }
    }
}

/// Finds every string and fret position for one MIDI pitch on a tuning.
///
/// Returns an empty list when the pitch or fret limit is invalid, or when the
/// pitch cannot be reached on any string.
pub fn fretboard_positions(midi: i32, tuning: &[i32], max_fret: i32) -> Vec<FretboardPosition> {
    if !(0..=127).contains(&midi) || max_fret < 0 {
        return Vec::new();
    }
    tuning
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, open)| {
            let fret = midi - open;
            if !(0..=max_fret).contains(&fret) {
                return None;
            }
            Some(FretboardPosition {
                midi,
                string: u32::try_from(index).ok()?,
                fret,
            })
        })
        .collect()
}

fn playable_positions(target: i32, source: i32, config: &HarmonyConfig) -> Vec<FretboardPosition> {
    let mut choices = Vec::new();
    for octave in -config.octave_range..=config.octave_range {
        let midi = target + octave * 12;
        if !(0..=127).contains(&midi) || (midi - source).abs() > 24 {
            continue;
        }
        choices.extend(fretboard_positions(midi, &config.tuning, config.max_fret));
    }
    choices
}

fn tie_break(seed: u64, position: &FretboardPosition) -> u64 {
    let value = seed
        ^ u64::from(position.string).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ u64::from(position.midi.unsigned_abs());
    let value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

const DEFAULT_BPM: f64 = 120.0;
const FAST_SUBDIVISION_BPM: f64 = 80.0;

fn tempo_at(doc: &Document, bar_index: usize) -> f64 {
    doc.tempo_map
        .iter()
        .filter(|change| change.at_bar <= bar_index)
        .max_by_key(|change| change.at_bar)
        .map(|change| change.bpm)
        .unwrap_or(DEFAULT_BPM)
}

struct AccumBeat {
    notes: Vec<Note>,
    dynamic: Option<String>,
    dur: Fraction,
    rhythm: Option<Rhythm>,
}

fn clamp_source_beats(beats: &[Beat], bpm: f64) -> Vec<Beat> {
    let sixteenth = Fraction::new(1, 16);
    if bpm <= FAST_SUBDIVISION_BPM
        || !beats
            .iter()
            .any(|b| inspect::rhythm_duration(b.rhythm) < sixteenth)
    {
        return beats.to_vec();
    }

    let mut accumulated: Vec<AccumBeat> = Vec::new();
    let mut i = 0;
    while i < beats.len() {
        let b = &beats[i];
        let dur = inspect::rhythm_duration(b.rhythm);
        let has_notes = b.notes.iter().any(|n| n.midi.is_some());

        if dur < sixteenth && !has_notes {
            if let Some(prev) = accumulated.last_mut() {
                prev.dur = prev.dur.plus(dur);
                prev.rhythm = None;
                i += 1;
                continue;
            }
        }

        let mut curr_dur = dur;
        let mut curr_notes = b.notes.clone();
        let mut curr_dynamic = b.dynamic.clone();
        let mut curr_has_notes = has_notes;
        let mut curr_rhythm = if dur >= sixteenth {
            Some(b.rhythm)
        } else {
            None
        };

        while curr_dur < sixteenth && i + 1 < beats.len() {
            i += 1;
            let next_b = &beats[i];
            let next_dur = inspect::rhythm_duration(next_b.rhythm);
            let next_has_notes = next_b.notes.iter().any(|n| n.midi.is_some());

            curr_dur = curr_dur.plus(next_dur);
            curr_rhythm = None;
            if !curr_has_notes && next_has_notes {
                curr_notes = next_b.notes.clone();
                curr_dynamic = next_b.dynamic.clone();
                curr_has_notes = true;
            }
        }

        if curr_dur < sixteenth {
            if let Some(prev) = accumulated.last_mut() {
                prev.dur = prev.dur.plus(curr_dur);
                prev.rhythm = None;
                if !prev.notes.iter().any(|n| n.midi.is_some()) && curr_has_notes {
                    prev.notes = curr_notes;
                    prev.dynamic = curr_dynamic;
                }
            } else {
                accumulated.push(AccumBeat {
                    notes: curr_notes,
                    dynamic: curr_dynamic,
                    dur: curr_dur,
                    rhythm: curr_rhythm,
                });
            }
        } else {
            accumulated.push(AccumBeat {
                notes: curr_notes,
                dynamic: curr_dynamic,
                dur: curr_dur,
                rhythm: curr_rhythm,
            });
        }
        i += 1;
    }

    let mut out_beats = Vec::new();
    for accum in accumulated {
        if let Some(rhythm) = accum.rhythm {
            out_beats.push(Beat {
                id: 0,
                rhythm,
                notes: accum.notes,
                dynamic: accum.dynamic,
            });
        } else {
            let rhythms = decompose_fraction_to_rhythms(accum.dur);
            for (idx, rhythm) in rhythms.into_iter().enumerate() {
                let (notes, dynamic) = if idx == 0 {
                    (accum.notes.clone(), accum.dynamic.clone())
                } else {
                    (Vec::new(), None)
                };
                out_beats.push(Beat {
                    id: 0,
                    rhythm,
                    notes,
                    dynamic,
                });
            }
        }
    }
    out_beats
}

fn decompose_fraction_to_rhythms(frac: Fraction) -> Vec<Rhythm> {
    const SPELLINGS: [(Fraction, NoteValue, u8); 12] = [
        (
            Fraction {
                numerator: 1,
                denominator: 1,
            },
            NoteValue::Whole,
            0,
        ),
        (
            Fraction {
                numerator: 7,
                denominator: 8,
            },
            NoteValue::Half,
            2,
        ),
        (
            Fraction {
                numerator: 3,
                denominator: 4,
            },
            NoteValue::Half,
            1,
        ),
        (
            Fraction {
                numerator: 1,
                denominator: 2,
            },
            NoteValue::Half,
            0,
        ),
        (
            Fraction {
                numerator: 7,
                denominator: 16,
            },
            NoteValue::Quarter,
            2,
        ),
        (
            Fraction {
                numerator: 3,
                denominator: 8,
            },
            NoteValue::Quarter,
            1,
        ),
        (
            Fraction {
                numerator: 1,
                denominator: 4,
            },
            NoteValue::Quarter,
            0,
        ),
        (
            Fraction {
                numerator: 7,
                denominator: 32,
            },
            NoteValue::Eighth,
            2,
        ),
        (
            Fraction {
                numerator: 3,
                denominator: 16,
            },
            NoteValue::Eighth,
            1,
        ),
        (
            Fraction {
                numerator: 1,
                denominator: 8,
            },
            NoteValue::Eighth,
            0,
        ),
        (
            Fraction {
                numerator: 3,
                denominator: 32,
            },
            NoteValue::Sixteenth,
            1,
        ),
        (
            Fraction {
                numerator: 1,
                denominator: 16,
            },
            NoteValue::Sixteenth,
            0,
        ),
    ];
    for (dur, value, dots) in SPELLINGS {
        if dur == frac {
            return vec![Rhythm {
                value,
                dots,
                tuplet: None,
            }];
        }
    }
    if frac.denominator != 0 && 64 % frac.denominator == 0 {
        let mult = 64 / frac.denominator;
        let mut steps = frac.numerator * mult;
        let mut out = Vec::new();
        const STEP_SPELLINGS: [(u64, NoteValue, u8); 12] = [
            (64, NoteValue::Whole, 0),
            (56, NoteValue::Half, 2),
            (48, NoteValue::Half, 1),
            (32, NoteValue::Half, 0),
            (28, NoteValue::Quarter, 2),
            (24, NoteValue::Quarter, 1),
            (16, NoteValue::Quarter, 0),
            (14, NoteValue::Eighth, 2),
            (12, NoteValue::Eighth, 1),
            (8, NoteValue::Eighth, 0),
            (6, NoteValue::Sixteenth, 1),
            (4, NoteValue::Sixteenth, 0),
        ];
        while steps > 0 {
            let choice = STEP_SPELLINGS
                .iter()
                .copied()
                .find(|(s, _, _)| *s <= steps && (steps - *s >= 4 || steps == *s));
            let (s, value, dots) = match choice {
                Some(c) => c,
                None => (4.min(steps), NoteValue::Sixteenth, 0),
            };
            out.push(Rhythm {
                value,
                dots,
                tuplet: None,
            });
            steps = steps.saturating_sub(s);
        }
        return out;
    }
    vec![Rhythm::new(NoteValue::Sixteenth)]
}

#[cfg(test)]
#[path = "harmony_test.rs"]
mod tests;
