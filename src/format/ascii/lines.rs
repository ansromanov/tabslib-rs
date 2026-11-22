//! The horizontal lines of a tablature staff: one per string, or one per drum.
//!
//! Owns [`Line`] and the rules that order and label lines. A fretted track gets
//! one line per string, highest-pitched first. A percussion track gets one lane
//! per instrument its notes sound, ordered as drum notation stacks them --
//! cymbals, hi-hat and ride on top, snare, toms from high to low, the rest, and
//! the bass drum at the bottom -- and labelled with the instrument's name, at
//! most [`MAX_DRUM_LABEL`] characters wide.

use crate::model::{Document, Note, Track};
use crate::percussion::{self, DrumRole};

/// One horizontal line of a tablature staff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    /// A string of a fretted instrument.
    String {
        /// String number, 0-based from the lowest string, as on [`Note::string`].
        string: u32,
        /// Open-string MIDI pitch.
        open_pitch: i32,
    },
    /// One percussion instrument.
    Drum {
        /// General MIDI percussion key, or `None` for hits whose instrument the
        /// document does not identify.
        midi: Option<u8>,
        /// Broad kit role, when the key is a standard percussion sound.
        role: Option<DrumRole>,
    },
}

/// Whether a track is a percussion staff: no tuning, or an all-zero one.
pub(crate) fn is_percussion(track: &Track) -> bool {
    track.tuning.is_empty() || track.tuning.iter().all(|pitch| *pitch == 0)
}

/// Resolved percussion key of a hit, the same way playback resolves it: the
/// track's articulation mapping, then the note's own pitch, then the raw
/// articulation when it is itself a standard percussion key.
pub(crate) fn drum_key(track: &Track, note: &Note) -> Option<u8> {
    let midi = note
        .articulation
        .and_then(|raw| {
            track
                .percussion_articulations
                .iter()
                .find(|mapping| mapping.raw_id == raw)
                .map(|mapping| i32::from(mapping.midi))
        })
        .or(note.midi)
        .or_else(|| {
            note.articulation
                .filter(|raw| percussion::midi_note(*raw).is_some())
        })?;
    u8::try_from(midi).ok().filter(|key| *key <= 127)
}

/// Widest drum lane label. Wide enough for "Tambourine" and "Side Stick",
/// narrow enough that the labels cost one short word of a terminal line.
pub const MAX_DRUM_LABEL: usize = 10;

/// Name of a General MIDI percussion key, at most [`MAX_DRUM_LABEL`] wide.
///
/// Keys that play the same instrument -- the two kicks, the two snares, the
/// two crashes -- share a name; [`track_lines`] numbers them apart when a
/// score sounds both. Toms and wood blocks, which a kit routinely sounds
/// together, are named apart instead, because a numbered lane says less than
/// "Low Floor" next to "Floor Tom". A key outside the standard kit keeps its number, which
/// is the only thing known about it.
fn drum_name(midi: u8) -> String {
    let label = match midi {
        35 | 36 => "Kick",
        37 => "Side Stick",
        38 | 40 => "Snare",
        39 => "Hand Clap",
        41 => "Low Floor",
        43 => "Floor Tom",
        42 => "Hi-Hat",
        44 => "Pedal Hat",
        45 => "Low Tom",
        46 => "Open Hat",
        47 => "Lo-Mid Tom",
        48 => "Hi-Mid Tom",
        50 => "High Tom",
        49 | 57 => "Crash",
        51 | 59 => "Ride",
        52 => "China",
        53 => "Ride Bell",
        54 => "Tambourine",
        55 => "Splash",
        56 => "Cowbell",
        58 => "Vibraslap",
        60 => "High Bongo",
        61 => "Low Bongo",
        62 => "Mute Conga",
        63 => "High Conga",
        64 => "Low Conga",
        65 => "Hi Timbale",
        66 => "Lo Timbale",
        67 => "High Agogo",
        68 => "Low Agogo",
        69 => "Cabasa",
        70 => "Maracas",
        71 | 72 => "Whistle",
        73 | 74 => "Guiro",
        75 => "Claves",
        76 => "Hi Woodblk",
        77 => "Lo Woodblk",
        78 => "Mute Cuica",
        79 => "Open Cuica",
        80 => "Mute Tri",
        81 => "Open Tri",
        _ => return midi.to_string(),
    };
    label.to_string()
}

/// A label for a lane whose name is already taken, still within
/// [`MAX_DRUM_LABEL`]: "Crash" and "Crash 2" for a score that sounds both
/// crashes.
fn numbered_label(base: &str, suffix: u32) -> String {
    let tail = format!(" {suffix}");
    let room = MAX_DRUM_LABEL.saturating_sub(tail.chars().count());
    let head: String = base.chars().take(room).collect();
    format!("{}{tail}", head.trim_end())
}

/// Stacking rank of a drum lane, top of the staff first.
fn lane_rank(midi: Option<u8>) -> (u8, i16) {
    let Some(key) = midi else {
        return (6, 0);
    };
    let key_order = i16::from(key);
    match percussion::role(key) {
        Some(DrumRole::Cymbal) => (0, key_order),
        Some(DrumRole::HiHat) => (1, key_order),
        Some(DrumRole::Ride) => (2, key_order),
        Some(DrumRole::Snare) => (3, key_order),
        // Higher toms sit higher on the staff.
        Some(DrumRole::Tom) => (4, -key_order),
        Some(DrumRole::Other) | None => (5, key_order),
        Some(DrumRole::Kick) => (7, key_order),
    }
}

/// Lines and their labels for a track, top line first.
///
/// Drum lanes cover every instrument the track sounds anywhere in the score,
/// so lanes stay put while a viewer scrolls.
pub(crate) fn track_lines(
    doc: &Document,
    track_index: usize,
    track: &Track,
) -> Vec<(Line, String)> {
    if !is_percussion(track) {
        let labels = crate::pitch::string_labels(&track.tuning);
        return track
            .tuning
            .iter()
            .enumerate()
            .rev()
            .zip(labels)
            .filter_map(|((string, open_pitch), label)| {
                let string = u32::try_from(string).ok()?;
                Some((
                    Line::String {
                        string,
                        open_pitch: *open_pitch,
                    },
                    label,
                ))
            })
            .collect();
    }
    let mut keys: Vec<Option<u8>> = Vec::new();
    for master in &doc.master_bars {
        let Some(bar) = super::layout::bar_for(doc, master.index, track_index) else {
            continue;
        };
        for note in bar
            .voices
            .iter()
            .flat_map(|voice| voice.beats.iter())
            .flat_map(|beat| beat.notes.iter())
        {
            let key = drum_key(track, note);
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys.sort_by_key(|key| lane_rank(*key));
    let mut used: Vec<String> = Vec::new();
    keys.into_iter()
        .map(|key| {
            let base = key.map_or_else(|| "Unknown".to_string(), drum_name);
            let mut label = base.clone();
            let mut suffix = 2;
            while used.contains(&label) {
                label = numbered_label(&base, suffix);
                suffix += 1;
            }
            used.push(label.clone());
            (
                Line::Drum {
                    midi: key,
                    role: key.and_then(percussion::role),
                },
                label,
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "lines_test.rs"]
mod tests;
