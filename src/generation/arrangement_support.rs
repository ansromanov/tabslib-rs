//! Internal deterministic transforms and identity allocation for arrangements.

use std::collections::BTreeSet;

use crate::inspect::{rhythm_duration, Fraction};
use crate::model::{Bar, Document, Note, NoteValue, Rhythm, Technique};

use super::arrangement::{ArrangementError, MotifFingerprint};

pub(super) fn transpose_fingerprint(
    mut fingerprint: MotifFingerprint,
    semitones: i32,
) -> MotifFingerprint {
    for pitch in fingerprint.pitch_classes.iter_mut().flatten() {
        *pitch = u8::try_from((i32::from(*pitch) + semitones).rem_euclid(12)).unwrap_or_default();
    }
    fingerprint
}

pub(super) fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

pub(super) fn seeded_index(seed: u64, len: usize) -> usize {
    let Ok(len) = u64::try_from(len) else {
        return 0;
    };
    usize::try_from(splitmix64(seed) % len.max(1)).unwrap_or_default()
}

#[derive(Default)]
pub(super) struct IdAllocator {
    bar: u32,
    voice: u32,
    beat: u32,
    note: u32,
}

impl IdAllocator {
    pub(super) fn from_document(doc: &Document) -> Self {
        let mut ids = Self::default();
        for bar in &doc.bars {
            ids.bar = ids.bar.max(bar.id);
            for voice in &bar.voices {
                ids.voice = ids.voice.max(voice.id);
                for beat in &voice.beats {
                    ids.beat = ids.beat.max(beat.id);
                    for note in &beat.notes {
                        ids.note = ids.note.max(note.id);
                    }
                }
            }
        }
        ids
    }
}

pub(super) fn reassign_ids(bar: &mut Bar, ids: &mut IdAllocator) -> Result<(), ArrangementError> {
    ids.bar = ids
        .bar
        .checked_add(1)
        .ok_or_else(|| ArrangementError::InvalidInput("bar ids exhausted".into()))?;
    bar.id = ids.bar;
    for voice in &mut bar.voices {
        ids.voice = ids
            .voice
            .checked_add(1)
            .ok_or_else(|| ArrangementError::InvalidInput("voice ids exhausted".into()))?;
        voice.id = ids.voice;
        for beat in &mut voice.beats {
            ids.beat = ids
                .beat
                .checked_add(1)
                .ok_or_else(|| ArrangementError::InvalidInput("beat ids exhausted".into()))?;
            beat.id = ids.beat;
            for note in &mut beat.notes {
                ids.note = ids
                    .note
                    .checked_add(1)
                    .ok_or_else(|| ArrangementError::InvalidInput("note ids exhausted".into()))?;
                note.id = ids.note;
            }
        }
    }
    Ok(())
}

pub(super) fn u32_to_bar_ref(id: u32) -> Result<i32, ArrangementError> {
    i32::try_from(id)
        .map_err(|_| ArrangementError::InvalidInput("bar id exceeds reference range".into()))
}

pub(super) fn transpose_bar(bar: &mut Bar, semitones: i32) -> Result<(), ArrangementError> {
    for note in bar
        .voices
        .iter_mut()
        .flat_map(|voice| voice.beats.iter_mut())
        .flat_map(|beat| beat.notes.iter_mut())
    {
        transpose_note(note, semitones)?;
    }
    Ok(())
}

fn transpose_note(note: &mut Note, semitones: i32) -> Result<(), ArrangementError> {
    if let Some(midi) = note.midi {
        let Some(transposed) = midi
            .checked_add(semitones)
            .filter(|pitch| (0..=127).contains(pitch))
        else {
            return Err(ArrangementError::InvalidTransposition {
                note_id: note.id,
                semitones,
            });
        };
        note.midi = Some(transposed);
    }
    if let Some(fret) = note.fret {
        let Some(transposed) = fret.checked_add(semitones).filter(|fret| *fret >= 0) else {
            return Err(ArrangementError::InvalidTransposition {
                note_id: note.id,
                semitones,
            });
        };
        note.fret = Some(transposed);
    }
    Ok(())
}

pub(super) fn range_can_transpose(
    doc: &Document,
    start_bar: usize,
    end_bar: usize,
    semitones: i32,
) -> bool {
    doc.master_bars[start_bar..=end_bar]
        .iter()
        .flat_map(|master| master.bar_ids.iter())
        .filter(|id| **id >= 0)
        .all(|id| {
            doc.bars
                .iter()
                .find(|bar| i32::try_from(bar.id).ok() == Some(*id))
                .is_some_and(|bar| {
                    bar.voices
                        .iter()
                        .flat_map(|voice| voice.beats.iter())
                        .flat_map(|beat| beat.notes.iter())
                        .all(|note| {
                            note.midi.is_none_or(|midi| {
                                midi.checked_add(semitones)
                                    .is_some_and(|value| (0..=127).contains(&value))
                            }) && note.fret.is_none_or(|fret| {
                                fret.checked_add(semitones).is_some_and(|value| value >= 0)
                            })
                        })
                })
        })
}

pub(super) fn transpose_selected_bars(
    doc: &mut Document,
    start_bar: usize,
    end_bar: usize,
    semitones: i32,
) -> Result<(), ArrangementError> {
    let selected = doc.master_bars[start_bar..=end_bar]
        .iter()
        .flat_map(|master| master.bar_ids.iter().copied())
        .filter(|id| *id >= 0)
        .collect::<BTreeSet<_>>();
    let mut replacements = Vec::with_capacity(selected.len());
    for id in selected {
        let mut bar = doc
            .bars
            .iter()
            .find(|bar| i32::try_from(bar.id).ok() == Some(id))
            .cloned()
            .ok_or_else(|| ArrangementError::InvalidInput(format!("missing bar id {id}")))?;
        transpose_bar(&mut bar, semitones)?;
        replacements.push(bar);
    }
    for replacement in replacements {
        if let Some(bar) = doc.bars.iter_mut().find(|bar| bar.id == replacement.id) {
            *bar = replacement;
        }
    }
    Ok(())
}

const RHYTHM_CANDIDATES: &[(NoteValue, u8)] = &[
    (NoteValue::Whole, 1),        // 3/2
    (NoteValue::Whole, 0),        // 1/1
    (NoteValue::Half, 2),         // 7/8
    (NoteValue::Half, 1),         // 3/4
    (NoteValue::Half, 0),         // 1/2
    (NoteValue::Quarter, 2),      // 7/16
    (NoteValue::Quarter, 1),      // 3/8
    (NoteValue::Quarter, 0),      // 1/4
    (NoteValue::Eighth, 2),       // 7/32
    (NoteValue::Eighth, 1),       // 3/16
    (NoteValue::Eighth, 0),       // 1/8
    (NoteValue::Sixteenth, 2),    // 7/64
    (NoteValue::Sixteenth, 1),    // 3/32
    (NoteValue::Sixteenth, 0),    // 1/16
    (NoteValue::ThirtySecond, 1), // 3/64
    (NoteValue::ThirtySecond, 0), // 1/32
    (NoteValue::SixtyFourth, 0),  // 1/64
];

pub(super) fn decompose_fraction_into_rhythms(mut fraction: Fraction) -> Vec<Rhythm> {
    if fraction.numerator == 0 {
        return Vec::new();
    }
    for &(value, dots) in RHYTHM_CANDIDATES {
        let r = Rhythm {
            value,
            dots,
            tuplet: None,
        };
        let (n, d) = r.as_fraction();
        if fraction == Fraction::new(n, d) {
            return vec![r];
        }
    }
    let mut result = Vec::new();
    while fraction.numerator > 0 {
        let mut matched = false;
        for &(value, dots) in RHYTHM_CANDIDATES {
            let r = Rhythm {
                value,
                dots,
                tuplet: None,
            };
            let (n, d) = r.as_fraction();
            let cand_frac = Fraction::new(n, d);
            if let Some(diff) = fraction.minus(cand_frac) {
                result.push(r);
                fraction = diff;
                matched = true;
                break;
            }
        }
        if !matched {
            break;
        }
    }
    result
}

pub(super) fn fit_bar_to_capacity(bar: &mut Bar, capacity: Fraction) {
    for voice in &mut bar.voices {
        let total_duration = voice.beats.iter().fold(Fraction::new(0, 1), |sum, beat| {
            sum.plus(rhythm_duration(beat.rhythm))
        });
        if total_duration <= capacity {
            continue;
        }
        let mut accumulated = Fraction::new(0, 1);
        let mut new_beats = Vec::new();
        for beat in &voice.beats {
            if accumulated >= capacity {
                break;
            }
            let beat_dur = rhythm_duration(beat.rhythm);
            if accumulated.plus(beat_dur) <= capacity {
                accumulated = accumulated.plus(beat_dur);
                new_beats.push(beat.clone());
            } else {
                let remaining = capacity.minus(accumulated).unwrap_or(Fraction::new(0, 1));
                if remaining.numerator > 0 {
                    let rhythms = decompose_fraction_into_rhythms(remaining);
                    for (i, rhythm) in rhythms.into_iter().enumerate() {
                        let mut split_beat = beat.clone();
                        split_beat.rhythm = rhythm;
                        if i > 0 && !split_beat.notes.is_empty() {
                            for note in &mut split_beat.notes {
                                if !note
                                    .techniques
                                    .iter()
                                    .any(|t| matches!(t, Technique::TieDestination))
                                {
                                    note.techniques.push(Technique::TieDestination);
                                }
                            }
                        }
                        new_beats.push(split_beat);
                    }
                }
                break;
            }
        }
        voice.beats = new_beats;
    }
}
