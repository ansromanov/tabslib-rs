//! Seeded solo-track generation over selected document bars.
//!
//! [`generate_solo`] uses caller-selected scale, register, rhythm pattern and
//! phrase length, and voices every generated pitch with the shared fretboard
//! playability primitives in [`crate::generation::harmony`].

use crate::edits;
use crate::generation::harmony::fretboard_positions;
use crate::inspect::{self, Fraction};
use crate::model::{Bar, Beat, Document, KeySignature, Note, Rhythm, Track, Voice};
use crate::{Error, Result};
use std::cmp::Ordering;

/// Constraints for generating a style-neutral melodic lead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoloConfig {
    /// Inclusive first score bar to fill.
    pub start_bar: usize,
    /// Inclusive last score bar to fill.
    pub end_bar: usize,
    /// Tonal center for the supplied scale intervals.
    pub key: KeySignature,
    /// Pitch-class intervals from the tonic, in semitones.
    pub scale: Vec<u8>,
    /// Inclusive MIDI register `(low, high)` for generated notes.
    pub register: (i32, i32),
    /// Allowed rhythmic values, repeated in order through each selected bar.
    /// Their sequence must fit the bar's meter exactly.
    pub rhythm_pattern: Vec<Rhythm>,
    /// Number of bars in a phrase. Phrase boundaries reset voice leading.
    pub phrase_bars: usize,
    /// Number of final note events in each phrase selected for seeded variation.
    /// Zero disables phrase-tail variation.
    pub phrase_tail_notes: usize,
    /// Open-string MIDI pitches, lowest string first.
    pub tuning: Vec<i32>,
    /// Highest permitted fret.
    pub max_fret: i32,
    /// Largest permitted interval between consecutive generated notes.
    pub max_leap: i32,
    /// Seed for reproducible pitch and phrase-tail choices.
    pub seed: u64,
    /// Display name for the generated track.
    pub track_name: String,
}

impl SoloConfig {
    /// Creates a C-major solo configuration with four-bar phrases and a standard guitar register.
    pub fn c_major(start_bar: usize, end_bar: usize, seed: u64) -> Self {
        Self {
            start_bar,
            end_bar,
            key: KeySignature {
                tonic: 0,
                minor: false,
            },
            scale: vec![0, 2, 4, 5, 7, 9, 11],
            register: (52, 79),
            rhythm_pattern: vec![Rhythm::new(crate::model::NoteValue::Eighth)],
            phrase_bars: 4,
            phrase_tail_notes: 2,
            tuning: vec![40, 45, 50, 55, 59, 64],
            max_fret: 24,
            max_leap: 12,
            seed,
            track_name: "Lead Guitar".into(),
        }
    }
}

/// Rejections encountered while solving generated solo pitches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoloDiagnostics {
    /// Rhythmic slots omitted because no playable pitch met the leap constraint.
    pub voice_leading_rejections: usize,
}

/// Result of adding a solo track to a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedSolo {
    /// Index of the appended track.
    pub track_index: usize,
    /// Number of generated notes.
    pub note_count: usize,
    /// Master-bar indexes that begin generated phrases.
    pub phrase_starts: Vec<usize>,
    /// Candidate rejection counts.
    pub diagnostics: SoloDiagnostics,
}

#[derive(Debug, Clone)]
struct BarPlan {
    bar_index: usize,
    rhythms: Vec<Rhythm>,
}

/// Adds a seeded, playable solo track across an inclusive score-bar range.
///
/// The generated bars retain the selected meters and leave bars outside the
/// range empty. Rhythmic patterns must exactly fill each selected bar. Phrase
/// tails choose among playable, scale-correct candidates using the supplied
/// seed, so identical inputs always produce identical pitch and fingering data.
///
/// ```
/// use tabslib::fixtures;
/// use tabslib::generation::solo::{generate_solo, SoloConfig};
///
/// let mut score = fixtures::repeated_frets();
/// let request = SoloConfig::c_major(1, 3, 42);
/// let result = generate_solo(&mut score, &request).unwrap();
/// assert!(result.note_count > 0);
/// ```
pub fn generate_solo(doc: &mut Document, config: &SoloConfig) -> Result<GeneratedSolo> {
    validate_config(doc, config)?;
    let plans = (config.start_bar..=config.end_bar)
        .map(|bar_index| {
            let meter = doc.master_bars[bar_index].time;
            let rhythms =
                tile_rhythm_pattern(&config.rhythm_pattern, inspect::bar_capacity(meter))?;
            Ok(BarPlan { bar_index, rhythms })
        })
        .collect::<Result<Vec<_>>>()?;

    let positions = scale_positions(config);
    if positions.is_empty() {
        return Err(Error::Malformed(
            "solo scale has no playable pitches inside the requested register".into(),
        ));
    }

    let mut generated_bars = Vec::with_capacity(plans.len());
    let mut phrase_starts = Vec::new();
    let mut diagnostics = SoloDiagnostics::default();
    let mut note_count = 0;
    for (phrase_index, phrase_start) in (0..plans.len()).step_by(config.phrase_bars).enumerate() {
        let phrase_end = (phrase_start + config.phrase_bars).min(plans.len());
        let phrase_plans = &plans[phrase_start..phrase_end];
        let event_count = phrase_plans
            .iter()
            .map(|plan| plan.rhythms.len())
            .sum::<usize>();
        let tail_start = event_count.saturating_sub(config.phrase_tail_notes);
        let first_bar = phrase_plans
            .first()
            .ok_or_else(|| Error::Malformed("solo phrase has no bars".into()))?
            .bar_index;
        phrase_starts.push(first_bar);
        let mut event_index = 0;
        let mut previous_pitch: Option<i32> = None;
        let mut melodic_direction = 1;
        let phrase_seed = mix(config.seed
            ^ u64::try_from(phrase_index)
                .unwrap_or(u64::MAX)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15));

        for plan in phrase_plans {
            let mut beats = Vec::with_capacity(plan.rhythms.len());
            for rhythm in &plan.rhythms {
                let phrase_tail = config.phrase_tail_notes > 0 && event_index >= tail_start;
                let candidates = positions
                    .iter()
                    .copied()
                    .filter(|position| {
                        previous_pitch.is_none_or(|previous| {
                            (position.midi - previous).abs() <= config.max_leap
                        })
                    })
                    .collect::<Vec<_>>();
                let moving_candidates = previous_pitch.map_or_else(
                    || candidates.clone(),
                    |previous| {
                        let moving = candidates
                            .iter()
                            .copied()
                            .filter(|position| position.midi != previous)
                            .collect::<Vec<_>>();
                        if moving.is_empty() {
                            candidates.clone()
                        } else {
                            moving
                        }
                    },
                );
                let note = if candidates.is_empty() {
                    diagnostics.voice_leading_rejections += 1;
                    None
                } else {
                    let chosen = choose_position(
                        &moving_candidates,
                        previous_pitch,
                        melodic_direction,
                        phrase_seed,
                        event_index,
                        phrase_tail,
                    );
                    if let Some(previous) = previous_pitch {
                        if chosen.midi != previous {
                            melodic_direction = (chosen.midi - previous).signum();
                        }
                    }
                    previous_pitch = Some(chosen.midi);
                    note_count += 1;
                    Some(Note {
                        id: 0,
                        midi: Some(chosen.midi),
                        string: Some(chosen.string),
                        fret: Some(chosen.fret),
                        articulation: None,
                        techniques: Vec::new(),
                    })
                };
                beats.push(Beat {
                    id: 0,
                    rhythm: *rhythm,
                    notes: note.into_iter().collect(),
                    dynamic: None,
                });
                event_index += 1;
            }
            generated_bars.push((
                plan.bar_index,
                Bar {
                    id: 0,
                    clef: Some("G2".into()),
                    voices: vec![Voice { id: 0, beats }],
                },
            ));
        }
    }

    append_solo_track(doc, config, generated_bars)?;
    Ok(GeneratedSolo {
        track_index: doc.tracks.len() - 1,
        note_count,
        phrase_starts,
        diagnostics,
    })
}

fn validate_config(doc: &Document, config: &SoloConfig) -> Result<()> {
    if doc.master_bars.is_empty() || config.start_bar > config.end_bar {
        return Err(Error::Malformed("invalid or empty solo bar range".into()));
    }
    if config.end_bar >= doc.master_bars.len() {
        return Err(Error::Malformed(
            "solo bar range exceeds the document".into(),
        ));
    }
    if !(0..12).contains(&config.key.tonic)
        || config.scale.is_empty()
        || config.scale.iter().any(|step| *step > 11)
        || config
            .scale
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != config.scale.len()
    {
        return Err(Error::Malformed("invalid solo key or scale".into()));
    }
    if config.register.0 < 0
        || config.register.1 > 127
        || config.register.0 > config.register.1
        || config.tuning.is_empty()
        || config.tuning.iter().any(|pitch| !(0..=127).contains(pitch))
        || config.max_fret < 0
        || config.max_leap < 0
        || config.phrase_bars == 0
        || config.phrase_tail_notes > 1024
        || config.rhythm_pattern.is_empty()
        || config.rhythm_pattern.iter().any(|rhythm| {
            rhythm.dots > 4 || rhythm.tuplet.is_some_and(|(num, den)| num == 0 || den == 0)
        })
    {
        return Err(Error::Malformed(
            "invalid solo register, tuning, or constraints".into(),
        ));
    }
    Ok(())
}

fn tile_rhythm_pattern(pattern: &[Rhythm], capacity: Fraction) -> Result<Vec<Rhythm>> {
    let mut rhythms = Vec::new();
    let mut elapsed = Fraction::new(0, 1);
    for index in 0..1024 {
        let rhythm = pattern[index % pattern.len()];
        let next = checked_plus(elapsed, inspect::rhythm_duration(rhythm)).ok_or_else(|| {
            Error::Malformed("solo rhythm pattern exceeds exact timing range".into())
        })?;
        if compare_fraction(next, capacity).is_gt() {
            return Err(Error::Malformed(
                "solo rhythm pattern does not fit the selected bar meter exactly".into(),
            ));
        }
        rhythms.push(rhythm);
        if compare_fraction(next, capacity).is_eq() {
            return Ok(rhythms);
        }
        elapsed = next;
    }
    Err(Error::Malformed(
        "solo rhythm pattern needs more than 1024 events per bar".into(),
    ))
}

fn compare_fraction(left: Fraction, right: Fraction) -> Ordering {
    (u128::from(left.numerator) * u128::from(right.denominator))
        .cmp(&(u128::from(right.numerator) * u128::from(left.denominator)))
}

fn checked_plus(left: Fraction, right: Fraction) -> Option<Fraction> {
    let numerator = u128::from(left.numerator)
        .checked_mul(u128::from(right.denominator))?
        .checked_add(u128::from(right.numerator).checked_mul(u128::from(left.denominator))?)?;
    let denominator = u128::from(left.denominator).checked_mul(u128::from(right.denominator))?;
    let divisor = gcd_u128(numerator, denominator);
    Some(Fraction::new(
        u64::try_from(numerator / divisor).ok()?,
        u64::try_from(denominator / divisor).ok()?,
    ))
}

fn gcd_u128(a: u128, b: u128) -> u128 {
    if b == 0 {
        a.max(1)
    } else {
        gcd_u128(b, a % b)
    }
}

fn scale_positions(config: &SoloConfig) -> Vec<crate::generation::harmony::FretboardPosition> {
    let mut positions = Vec::new();
    for midi in config.register.0..=config.register.1 {
        let interval = (midi - i32::from(config.key.tonic)).rem_euclid(12);
        let Ok(interval) = u8::try_from(interval) else {
            continue;
        };
        if config.scale.contains(&interval) {
            positions.extend(fretboard_positions(midi, &config.tuning, config.max_fret));
        }
    }
    positions.sort_by_key(|position| (position.midi, position.string, position.fret));
    positions
}

fn choose_position(
    candidates: &[crate::generation::harmony::FretboardPosition],
    previous: Option<i32>,
    direction: i32,
    seed: u64,
    event: usize,
    phrase_tail: bool,
) -> crate::generation::harmony::FretboardPosition {
    if phrase_tail {
        let mut pitches = candidates
            .iter()
            .map(|position| position.midi)
            .collect::<Vec<_>>();
        pitches.dedup();
        let pitch = pitches[seeded_index(seed, event, pitches.len())];
        let same_pitch = candidates
            .iter()
            .copied()
            .filter(|position| position.midi == pitch)
            .collect::<Vec<_>>();
        return same_pitch[seeded_index(mix(seed), event, same_pitch.len())];
    }
    candidates
        .iter()
        .copied()
        .min_by_key(|position| {
            let target = previous.map_or(position.midi, |pitch| pitch + direction * 2);
            let distance = (position.midi - target).abs();
            (
                distance * 32 + position.fret,
                mix(seed
                    ^ event_seed(event).wrapping_mul(0x9e37_79b9_7f4a_7c15)
                    ^ u64::from(position.string)
                    ^ u64::from(position.midi.unsigned_abs())),
            )
        })
        .unwrap_or(candidates[0])
}

fn seeded_index(seed: u64, event: usize, count: usize) -> usize {
    let mixed = mix(seed ^ event_seed(event).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    let count = u64::try_from(count).unwrap_or(u64::MAX);
    usize::try_from(mixed % count).unwrap_or(0)
}

fn event_seed(event: usize) -> u64 {
    u64::try_from(event).unwrap_or(u64::MAX)
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn append_solo_track(
    doc: &mut Document,
    config: &SoloConfig,
    mut generated: Vec<(usize, Bar)>,
) -> Result<()> {
    let bar_count = generated.len();
    let voice_count = generated.iter().map(|(_, bar)| bar.voices.len()).sum();
    let beat_count = generated
        .iter()
        .flat_map(|(_, bar)| &bar.voices)
        .map(|voice| voice.beats.len())
        .sum();
    let note_count = generated
        .iter()
        .flat_map(|(_, bar)| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .map(|beat| beat.notes.len())
        .sum();
    let bar_id = reserve_ids(doc.bars.iter().map(|bar| bar.id).max(), bar_count, "bar")?;
    let voice_id = reserve_ids(
        doc.bars
            .iter()
            .flat_map(|bar| &bar.voices)
            .map(|voice| voice.id)
            .max(),
        voice_count,
        "voice",
    )?;
    let beat_id = reserve_ids(
        doc.bars
            .iter()
            .flat_map(|bar| &bar.voices)
            .flat_map(|voice| &voice.beats)
            .map(|beat| beat.id)
            .max(),
        beat_count,
        "beat",
    )?;
    let note_id = reserve_ids(
        doc.bars
            .iter()
            .flat_map(|bar| &bar.voices)
            .flat_map(|voice| &voice.beats)
            .flat_map(|beat| &beat.notes)
            .map(|note| note.id)
            .max(),
        note_count,
        "note",
    )?;
    let last_bar_id = bar_id
        .checked_add(
            u32::try_from(bar_count.saturating_sub(1))
                .map_err(|_| Error::Malformed("solo bar count exceeds ID range".into()))?,
        )
        .ok_or_else(|| Error::Malformed("solo bar IDs exceed supported range".into()))?;
    i32::try_from(last_bar_id)
        .map_err(|_| Error::Malformed("solo bar IDs exceed document range".into()))?;

    let track_index = edits::create_track(
        doc,
        Track {
            id: 0,
            name: config.track_name.clone(),
            color: None,
            tuning: config.tuning.clone(),
            midi_program: Some(30),
            midi_channel: Some(0),
            pan: None,
            volume: None,
            mute: false,
            solo: false,
            percussion_articulations: Vec::new(),
            generator_metadata: None,
        },
    );
    let mut bar_ids = bar_id..;
    let mut voice_ids = voice_id..;
    let mut beat_ids = beat_id..;
    let mut note_ids = note_id..;
    for (master_index, mut bar) in generated.drain(..) {
        bar.id = bar_ids
            .next()
            .ok_or_else(|| Error::Malformed("solo bar IDs exceed supported range".into()))?;
        for voice in &mut bar.voices {
            voice.id = voice_ids
                .next()
                .ok_or_else(|| Error::Malformed("solo voice IDs exceed supported range".into()))?;
            for beat in &mut voice.beats {
                beat.id = beat_ids.next().ok_or_else(|| {
                    Error::Malformed("solo beat IDs exceed supported range".into())
                })?;
                for note in &mut beat.notes {
                    note.id = note_ids.next().ok_or_else(|| {
                        Error::Malformed("solo note IDs exceed supported range".into())
                    })?;
                }
            }
        }
        doc.master_bars[master_index].bar_ids[track_index] = i32::try_from(bar.id)
            .map_err(|_| Error::Malformed("solo bar ID exceeds document range".into()))?;
        doc.bars.push(bar);
    }
    Ok(())
}

fn reserve_ids(maximum: Option<u32>, count: usize, kind: &str) -> Result<u32> {
    if count == 0 {
        return Ok(maximum.unwrap_or(0));
    }
    let first = maximum
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| Error::Malformed(format!("solo {kind} IDs exceed supported range")))?;
    let span = u32::try_from(count - 1)
        .map_err(|_| Error::Malformed(format!("solo {kind} count exceeds supported range")))?;
    first
        .checked_add(span)
        .and_then(|last| last.checked_add(1))
        .ok_or_else(|| Error::Malformed(format!("solo {kind} IDs exceed supported range")))?;
    Ok(first)
}

#[cfg(test)]
#[path = "solo_test.rs"]
mod tests;
