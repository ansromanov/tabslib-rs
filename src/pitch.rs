//! Deterministic pitch, tuning, capo, and key transformations.

use crate::model::{Document, KeySignature};

/// Failure raised when a requested retuning cannot represent a note on its string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PitchError {
    /// A track or bar selection was outside the document.
    InvalidSelection(String),
    /// A note would require a negative fret or a string not present in the tuning.
    Unplayable { track: usize, bar: usize, note: u32 },
    /// A target tuning is empty or contains an invalid pitch.
    InvalidTuning,
}

fn scale(key: KeySignature) -> [i32; 7] {
    if key.minor {
        [0, 2, 3, 5, 7, 8, 10]
    } else {
        [0, 2, 4, 5, 7, 9, 11]
    }
}

/// Returns the chromatic interval represented by a number of diatonic steps.
pub fn diatonic_interval(key: KeySignature, steps: i32) -> i32 {
    let degrees = scale(key);
    let octave = steps.div_euclid(7);
    let degree = usize::try_from(steps.rem_euclid(7)).unwrap_or(0);
    octave * 12 + degrees[degree]
}

fn note_indices(
    doc: &Document,
    track: Option<usize>,
    start: usize,
    end: usize,
) -> Result<Vec<(usize, usize, usize, usize)>, PitchError> {
    if start > end || end >= doc.master_bars.len() {
        return Err(PitchError::InvalidSelection("bar range".into()));
    }
    let tracks: Vec<usize> = match track {
        Some(index) if index < doc.tracks.len() => vec![index],
        Some(_) => return Err(PitchError::InvalidSelection("track index".into())),
        None => (0..doc.tracks.len()).collect(),
    };
    let mut result = Vec::new();
    for track_index in tracks {
        for bar_index in start..=end {
            let Some(id) = doc.master_bars[bar_index].bar_ids.get(track_index).copied() else {
                continue;
            };
            let Some(bar) = doc.bars.iter().position(|bar| bar.id == id as u32) else {
                continue;
            };
            for voice in 0..doc.bars[bar].voices.len() {
                for beat in 0..doc.bars[bar].voices[voice].beats.len() {
                    result.push((track_index, bar_index, bar, voice * 1_000_000 + beat));
                }
            }
        }
    }
    Ok(result)
}

fn each_note_mut<F>(
    doc: &mut Document,
    track: Option<usize>,
    start: usize,
    end: usize,
    mut f: F,
) -> Result<(), PitchError>
where
    F: FnMut(usize, usize, &mut crate::model::Note),
{
    let indices = note_indices(doc, track, start, end)?;
    for (track_index, bar_index, bar_index_in_doc, packed) in indices {
        let voice_index = packed / 1_000_000;
        let beat_index = packed % 1_000_000;
        for note in &mut doc.bars[bar_index_in_doc].voices[voice_index].beats[beat_index].notes {
            f(track_index, bar_index, note);
        }
    }
    Ok(())
}

fn transpose_selected(
    doc: &mut Document,
    track: Option<usize>,
    start: usize,
    end: usize,
    semitones: i32,
) -> Result<(), PitchError> {
    let tunings = doc
        .tracks
        .iter()
        .map(|track| track.tuning.clone())
        .collect::<Vec<_>>();
    each_note_mut(doc, track, start, end, |track_index, bar_index, note| {
        if let Some(midi) = note.midi {
            note.midi = Some(midi + semitones);
            if let (Some(string), Some(fret)) = (note.string, note.fret) {
                let open = tunings[track_index]
                    .len()
                    .checked_sub(string as usize)
                    .and_then(|i| tunings[track_index].get(i))
                    .copied()
                    .unwrap_or(i32::MIN);
                note.fret = Some(fret + semitones);
                if open == i32::MIN || fret + semitones < 0 {
                    let _ = (bar_index, note.id);
                }
            }
        }
    })
    .and_then(|_| validate_playability(doc, track, start, end))
}

fn validate_playability(
    doc: &Document,
    track: Option<usize>,
    start: usize,
    end: usize,
) -> Result<(), PitchError> {
    for (track_index, bar_index, bar, packed) in note_indices(doc, track, start, end)? {
        let voice = packed / 1_000_000;
        let beat = packed % 1_000_000;
        for note in &doc.bars[bar].voices[voice].beats[beat].notes {
            if let (Some(string), Some(fret)) = (note.string, note.fret) {
                if fret < 0
                    || usize::try_from(string)
                        .ok()
                        .and_then(|s| doc.tracks[track_index].tuning.len().checked_sub(s))
                        .and_then(|i| doc.tracks[track_index].tuning.get(i))
                        .is_none()
                {
                    return Err(PitchError::Unplayable {
                        track: track_index,
                        bar: bar_index,
                        note: note.id,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Transposes every pitched note chromatically, preserving its string where possible.
pub fn transpose(doc: &mut Document, semitones: i32) -> Result<(), PitchError> {
    let end = doc
        .master_bars
        .len()
        .checked_sub(1)
        .ok_or_else(|| PitchError::InvalidSelection("empty document".into()))?;
    transpose_selected(doc, None, 0, end, semitones)
}
/// Transposes one track over an inclusive bar range.
pub fn transpose_track(
    doc: &mut Document,
    track: usize,
    start: usize,
    end: usize,
    semitones: i32,
) -> Result<(), PitchError> {
    transpose_selected(doc, Some(track), start, end, semitones)
}
/// Transposes one track by a key-aware diatonic interval.
pub fn transpose_diatonic(
    doc: &mut Document,
    track: usize,
    start: usize,
    end: usize,
    key: KeySignature,
    steps: i32,
) -> Result<(), PitchError> {
    let interval = diatonic_interval(key, steps);
    transpose_track(doc, track, start, end, interval)
}
/// Applies a capo offset to one track while keeping fretting unchanged.
pub fn apply_capo(
    doc: &mut Document,
    track: usize,
    start: usize,
    end: usize,
    semitones: i32,
) -> Result<(), PitchError> {
    each_note_mut(doc, Some(track), start, end, |_, _, note| {
        if let Some(midi) = note.midi {
            note.midi = Some(midi + semitones);
        }
    })
}
/// Maps a pitch class from one tonic to another, preserving its octave and scale degree.
pub fn map_pitch_class(pitch_class: i32, from: KeySignature, to: KeySignature) -> i32 {
    (pitch_class + to.tonic as i32 - from.tonic as i32).rem_euclid(12)
}
/// Retunes a track while preserving sounding pitches and string assignments.
pub fn retune_preserve_pitch(
    doc: &mut Document,
    track: usize,
    tuning: &[i32],
) -> Result<(), PitchError> {
    if tuning.is_empty() || tuning.iter().any(|pitch| !(*pitch >= 0 && *pitch <= 127)) {
        return Err(PitchError::InvalidTuning);
    }
    let end = doc
        .master_bars
        .len()
        .checked_sub(1)
        .ok_or_else(|| PitchError::InvalidSelection("empty document".into()))?;
    let old = doc
        .tracks
        .get(track)
        .ok_or_else(|| PitchError::InvalidSelection("track index".into()))?
        .tuning
        .clone();
    each_note_mut(doc, Some(track), 0, end, |_, bar, note| {
        if let (Some(string), Some(midi)) = (note.string, note.midi) {
            if let Some(open) = tuning.get(old.len().saturating_sub(string as usize)) {
                note.fret = Some(midi - *open);
            } else {
                note.fret = Some(-1);
            }
        }
        let _ = bar;
    })
    .and_then(|_| {
        doc.tracks[track].tuning = tuning.to_vec();
        validate_playability(doc, Some(track), 0, end)
    })
}
/// Retunes a track while preserving the written string and fret fingering.
pub fn retune_preserve_fingering(
    doc: &mut Document,
    track: usize,
    tuning: &[i32],
) -> Result<(), PitchError> {
    if tuning.is_empty() || tuning.iter().any(|pitch| !(*pitch >= 0 && *pitch <= 127)) {
        return Err(PitchError::InvalidTuning);
    }
    let end = doc
        .master_bars
        .len()
        .checked_sub(1)
        .ok_or_else(|| PitchError::InvalidSelection("empty document".into()))?;
    let old = doc
        .tracks
        .get(track)
        .ok_or_else(|| PitchError::InvalidSelection("track index".into()))?
        .tuning
        .clone();
    each_note_mut(doc, Some(track), 0, end, |_, _, note| {
        if let (Some(string), Some(fret)) = (note.string, note.fret) {
            if let (Some(old_open), Some(new_open)) = (
                old.get(old.len().saturating_sub(string as usize)),
                tuning.get(tuning.len().saturating_sub(string as usize)),
            ) {
                note.midi = Some(new_open + fret);
                let _ = old_open;
            }
        }
    })
    .and_then(|_| {
        doc.tracks[track].tuning = tuning.to_vec();
        validate_playability(doc, Some(track), 0, end)
    })
}
