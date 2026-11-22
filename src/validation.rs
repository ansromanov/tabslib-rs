//! Structural validation for [`crate::Document`] values.

use crate::model::Document;
use std::collections::HashSet;

/// One non-fatal document validation issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Stable path to the affected document value.
    pub path: String,
    /// Human-readable explanation of the invariant violation.
    pub message: String,
}

/// Validates references, identifiers, ranges, and bar integrity.
pub fn validate(document: &Document) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut track_ids = HashSet::new();
    for (index, track) in document.tracks.iter().enumerate() {
        if !track_ids.insert(track.id) {
            issues.push(issue(format!("tracks[{index}].id"), "duplicate track id"));
        }
        for (string, pitch) in track.tuning.iter().enumerate() {
            if !(0..=127).contains(pitch) {
                issues.push(issue(
                    format!("tracks[{index}].tuning[{string}]"),
                    "open pitch must be between 0 and 127",
                ));
            }
        }
    }
    let mut bar_ids = HashSet::new();
    for (index, bar) in document.bars.iter().enumerate() {
        if !bar_ids.insert(bar.id) {
            issues.push(issue(format!("bars[{index}].id"), "duplicate bar id"));
        }
        validate_nested_ids(bar, index, &mut issues);
    }
    for (index, master) in document.master_bars.iter().enumerate() {
        if master.index != index {
            issues.push(issue(
                format!("masterBars[{index}].index"),
                "index must match position",
            ));
        }
        if master.time.0 == 0 || master.time.1 == 0 {
            issues.push(issue(
                format!("masterBars[{index}].time"),
                "time signature must be non-zero",
            ));
        }
        for (track_index, bar_id) in master.bar_ids.iter().enumerate() {
            let Some(bar_id) = u32::try_from(*bar_id).ok() else {
                issues.push(issue(
                    format!("masterBars[{index}].barIds[{track_index}]"),
                    "bar reference does not resolve",
                ));
                continue;
            };
            let Some(bar) = document.bars.iter().find(|bar| bar.id == bar_id) else {
                issues.push(issue(
                    format!("masterBars[{index}].barIds[{track_index}]"),
                    "bar reference does not resolve",
                ));
                continue;
            };
            let Some(track) = document.tracks.get(track_index) else {
                issues.push(issue(
                    format!("masterBars[{index}].barIds[{track_index}]"),
                    "bar reference has no track",
                ));
                continue;
            };
            for (voice_index, voice) in bar.voices.iter().enumerate() {
                for (beat_index, beat) in voice.beats.iter().enumerate() {
                    for (note_index, note) in beat.notes.iter().enumerate() {
                        if let Some(string) = note.string {
                            let in_range = usize::try_from(string)
                                .ok()
                                .is_some_and(|s| s < track.tuning.len());
                            if !in_range {
                                issues.push(issue(format!("masterBars[{index}].tracks[{track_index}].voices[{voice_index}].beats[{beat_index}].notes[{note_index}].string"), "string is outside the track tuning"));
                            }
                        }
                        if let Some(fret) = note.fret {
                            if fret < 0 {
                                issues.push(issue(format!("masterBars[{index}].tracks[{track_index}].voices[{voice_index}].beats[{beat_index}].notes[{note_index}].fret"), "fret must be non-negative"));
                            }
                        }
                        if let Some(midi) = note.midi {
                            if !(0..=127).contains(&midi) {
                                issues.push(issue(format!("masterBars[{index}].tracks[{track_index}].voices[{voice_index}].beats[{beat_index}].notes[{note_index}].midi"), "MIDI pitch must be between 0 and 127"));
                            }
                        }
                    }
                }
            }
        }
        if master.bar_ids.len() > document.tracks.len() {
            issues.push(issue(
                format!("masterBars[{index}].barIds"),
                "more bar references than tracks",
            ));
        }
    }
    issues
}

fn validate_nested_ids(bar: &crate::model::Bar, bar_index: usize, issues: &mut Vec<Issue>) {
    let mut voice_ids = HashSet::new();
    for (voice_index, voice) in bar.voices.iter().enumerate() {
        if !voice_ids.insert(voice.id) {
            issues.push(issue(
                format!("bars[{bar_index}].voices[{voice_index}].id"),
                "duplicate voice id",
            ));
        }
        let mut beat_ids = HashSet::new();
        for (beat_index, beat) in voice.beats.iter().enumerate() {
            if !beat_ids.insert(beat.id) {
                issues.push(issue(
                    format!("bars[{bar_index}].voices[{voice_index}].beats[{beat_index}].id"),
                    "duplicate beat id",
                ));
            }
            let mut note_ids = HashSet::new();
            for (note_index, note) in beat.notes.iter().enumerate() {
                if !note_ids.insert(note.id) {
                    issues.push(issue(format!("bars[{bar_index}].voices[{voice_index}].beats[{beat_index}].notes[{note_index}].id"), "duplicate note id"));
                }
            }
        }
    }
}

fn issue(path: String, message: &str) -> Issue {
    Issue {
        path,
        message: message.to_string(),
    }
}

#[cfg(test)]
#[path = "validation_test.rs"]
mod tests;
