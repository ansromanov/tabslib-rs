//! Deterministic note and articulation edits over explicit selections.

use std::collections::BTreeMap;

use crate::inspect::bar_integrity;
use crate::inspect::Fraction;
use crate::model::{Document, Note, Technique};

/// A note selection. `None` means every value on that axis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    /// Track index, or every track.
    pub track: Option<usize>,
    /// Inclusive master-bar range, or the whole score.
    pub bars: Option<(usize, usize)>,
    /// String number filter.
    pub string: Option<u32>,
    /// MIDI pitch-class filter, normalized modulo twelve.
    pub pitch_class: Option<i32>,
}

/// Error returned when a selection edit cannot be applied safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionError {
    /// A selection index or range is invalid.
    InvalidSelection(String),
    /// Re-fingering would require an unavailable or negative fret.
    Unplayable { track: usize, bar: usize, note: u32 },
    /// An edit produced an over-full voice.
    Overfull(Vec<crate::inspect::BarIntegrity>),
}

type NoteIndex = (usize, usize, usize, usize, usize);
type IntegrityKey = (usize, usize, usize);
type IntegrityState = BTreeMap<IntegrityKey, Fraction>;

fn overfull_state(doc: &Document) -> IntegrityState {
    bar_integrity(doc)
        .into_iter()
        .filter(|item| item.duration > item.capacity)
        .map(|item| {
            (
                (item.track_index, item.bar_index, item.voice_index),
                item.duration,
            )
        })
        .collect()
}

fn selected_indices(
    doc: &Document,
    selection: Selection,
    predicate: Option<fn(&Note) -> bool>,
) -> Result<Vec<NoteIndex>, SelectionError> {
    let (start, end) = selection
        .bars
        .unwrap_or((0, doc.master_bars.len().saturating_sub(1)));
    if doc.master_bars.is_empty() || start > end || end >= doc.master_bars.len() {
        return Err(SelectionError::InvalidSelection("bar range".into()));
    }
    let tracks = match selection.track {
        Some(track) if track < doc.tracks.len() => vec![track],
        Some(_) => return Err(SelectionError::InvalidSelection("track index".into())),
        None => (0..doc.tracks.len()).collect::<Vec<_>>(),
    };
    let mut result = Vec::new();
    for track in tracks {
        for bar in start..=end {
            let Some(id) = doc.master_bars[bar].bar_ids.get(track).copied() else {
                continue;
            };
            let Some(bar_pos) = doc.bars.iter().position(|item| item.id == id as u32) else {
                continue;
            };
            for (voice, value) in doc.bars[bar_pos].voices.iter().enumerate() {
                for (beat, item) in value.beats.iter().enumerate() {
                    for (note, value) in item.notes.iter().enumerate() {
                        let string_ok = selection
                            .string
                            .is_none_or(|string| value.string == Some(string));
                        let pitch_ok = selection.pitch_class.is_none_or(|pitch| {
                            value
                                .midi
                                .is_some_and(|midi| midi.rem_euclid(12) == pitch.rem_euclid(12))
                        });
                        let predicate_ok = predicate.is_none_or(|check| check(value));
                        if string_ok && pitch_ok && predicate_ok {
                            result.push((track, bar, bar_pos, voice, beat * 100_000 + note));
                        }
                    }
                }
            }
        }
    }
    Ok(result)
}

fn check(doc: &Document, before: &IntegrityState) -> Result<(), SelectionError> {
    let failures = bar_integrity(doc)
        .into_iter()
        .filter(|item| item.duration > item.capacity)
        .filter(|item| {
            let key = (item.track_index, item.bar_index, item.voice_index);
            before
                .get(&key)
                .is_none_or(|duration| item.duration > *duration)
        })
        .collect::<Vec<_>>();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(SelectionError::Overfull(failures))
    }
}

/// Applies a technique once to every selected note (idempotent).
pub fn set_technique(
    doc: &mut Document,
    selection: Selection,
    technique: Technique,
) -> Result<(), SelectionError> {
    let before = overfull_state(doc);
    for (_, _, bar, voice, packed) in selected_indices(doc, selection, None)? {
        let beat = packed / 100_000;
        let note = packed % 100_000;
        let target = &mut doc.bars[bar].voices[voice].beats[beat].notes[note];
        if !target.techniques.contains(&technique) {
            target.techniques.push(technique);
        }
    }
    check(doc, &before)
}

/// Clears an exact technique from every selected note.
pub fn clear_technique(
    doc: &mut Document,
    selection: Selection,
    technique: &Technique,
) -> Result<(), SelectionError> {
    let before = overfull_state(doc);
    for (_, _, bar, voice, packed) in selected_indices(doc, selection, None)? {
        let beat = packed / 100_000;
        let note = packed % 100_000;
        doc.bars[bar].voices[voice].beats[beat].notes[note]
            .techniques
            .retain(|item| item != technique);
    }
    check(doc, &before)
}

/// Sets the accent technique on every selected note.
pub fn set_accent(doc: &mut Document, selection: Selection) -> Result<(), SelectionError> {
    set_technique(doc, selection, Technique::Accent)
}
/// Sets or clears a written dynamic on beats containing selected notes.
pub fn set_dynamic(
    doc: &mut Document,
    selection: Selection,
    dynamic: Option<String>,
) -> Result<(), SelectionError> {
    let before = overfull_state(doc);
    let indices = selected_indices(doc, selection, None)?;
    for (_, _, bar, voice, packed) in indices {
        let beat = packed / 100_000;
        doc.bars[bar].voices[voice].beats[beat].dynamic = dynamic.clone();
    }
    check(doc, &before)
}

/// Re-fingers selected notes onto one target string without changing MIDI pitch.
pub fn refinger(
    doc: &mut Document,
    selection: Selection,
    target_string: u32,
) -> Result<(), SelectionError> {
    let before = overfull_state(doc);
    let indices = selected_indices(doc, selection, None)?;
    for (track, bar_index, bar, voice, packed) in indices {
        let beat = packed / 100_000;
        let note_index = packed % 100_000;
        let tuning = &doc.tracks[track].tuning;
        let Some(open) = target_string
            .checked_sub(1)
            .and_then(|string| tuning.get(tuning.len().saturating_sub(string as usize + 1)))
        else {
            return Err(SelectionError::Unplayable {
                track,
                bar: bar_index,
                note: doc.bars[bar].voices[voice].beats[beat].notes[note_index].id,
            });
        };
        let note = &mut doc.bars[bar].voices[voice].beats[beat].notes[note_index];
        let Some(midi) = note.midi else { continue };
        let fret = midi - *open;
        if fret < 0 {
            return Err(SelectionError::Unplayable {
                track,
                bar: bar_index,
                note: note.id,
            });
        }
        note.string = Some(target_string);
        note.fret = Some(fret);
    }
    check(doc, &before)
}

/// Splits a voice at a beat index, preserving beat order and durations.
pub fn split_voice(
    doc: &mut Document,
    track: usize,
    bar: usize,
    voice: usize,
    at_beat: usize,
) -> Result<(), SelectionError> {
    let before = overfull_state(doc);
    let id = *doc
        .master_bars
        .get(bar)
        .and_then(|master| master.bar_ids.get(track))
        .ok_or_else(|| SelectionError::InvalidSelection("track or bar".into()))?;
    let target = doc
        .bars
        .iter_mut()
        .find(|item| item.id == id as u32)
        .and_then(|item| item.voices.get_mut(voice))
        .ok_or_else(|| SelectionError::InvalidSelection("voice".into()))?;
    if at_beat > target.beats.len() {
        return Err(SelectionError::InvalidSelection("beat index".into()));
    }
    let beats = target.beats.split_off(at_beat);
    let new_id = doc
        .bars
        .iter()
        .flat_map(|bar| bar.voices.iter())
        .map(|voice| voice.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let bar = doc
        .bars
        .iter_mut()
        .find(|item| item.id == id as u32)
        .ok_or_else(|| SelectionError::InvalidSelection("bar".into()))?;
    bar.voices.push(crate::model::Voice { id: new_id, beats });
    check(doc, &before)
}

/// Merges a source voice into a target voice and removes the source voice.
pub fn merge_voices(
    doc: &mut Document,
    track: usize,
    bar: usize,
    target: usize,
    source: usize,
) -> Result<(), SelectionError> {
    let before = overfull_state(doc);
    if target == source {
        return Err(SelectionError::InvalidSelection(
            "voices must differ".into(),
        ));
    }
    let id = *doc
        .master_bars
        .get(bar)
        .and_then(|master| master.bar_ids.get(track))
        .ok_or_else(|| SelectionError::InvalidSelection("track or bar".into()))?;
    let score_bar = doc
        .bars
        .iter_mut()
        .find(|item| item.id == id as u32)
        .ok_or_else(|| SelectionError::InvalidSelection("bar".into()))?;
    if target >= score_bar.voices.len() || source >= score_bar.voices.len() {
        return Err(SelectionError::InvalidSelection("voice".into()));
    }
    let moved = score_bar.voices.remove(source).beats;
    let target = if source < target { target - 1 } else { target };
    score_bar.voices[target].beats.extend(moved);
    check(doc, &before)
}
