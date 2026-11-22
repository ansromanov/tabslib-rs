//! Shared tablature extraction for the HTML and PDF rendering adapters.
//!
//! The types in this module are private to the format layer. They turn the
//! document's id-based bar references into a small, deterministic view that
//! each output adapter can lay out in its own medium.

use crate::error::{Error, Result};
use crate::model::{Document, Note, NoteValue, Track};

/// A score view ready for a rendering adapter.
#[derive(Debug, Clone)]
pub(crate) struct TabScore {
    pub(crate) title: String,
    pub(crate) tracks: Vec<TabTrack>,
}

/// One selected track in a score view.
#[derive(Debug, Clone)]
pub(crate) struct TabTrack {
    pub(crate) name: String,
    pub(crate) tuning: Vec<String>,
    pub(crate) percussion: bool,
    pub(crate) bars: Vec<TabBar>,
}

/// One bar in a score view.
#[derive(Debug, Clone)]
pub(crate) struct TabBar {
    pub(crate) index: usize,
    pub(crate) time: (u32, u32),
    pub(crate) double_bar: bool,
    pub(crate) beats: Vec<TabBeat>,
}

/// One rhythmic position in a bar.
#[derive(Debug, Clone)]
pub(crate) struct TabBeat {
    pub(crate) rhythm: String,
    pub(crate) notes: Vec<Option<String>>,
}

pub(crate) fn all(doc: &Document) -> Result<TabScore> {
    if doc.master_bars.is_empty() {
        return Ok(TabScore {
            title: doc.title.clone(),
            tracks: Vec::new(),
        });
    }
    range(doc, None, 0, doc.master_bars.len() - 1)
}

pub(crate) fn track(doc: &Document, track: usize) -> Result<TabScore> {
    if doc.master_bars.is_empty() {
        if doc.tracks.get(track).is_none() {
            return Err(invalid(format!("track index {track} outside document")));
        }
        return Ok(TabScore {
            title: doc.title.clone(),
            tracks: vec![empty_track(&doc.tracks[track])],
        });
    }
    range(doc, Some(track), 0, doc.master_bars.len() - 1)
}

pub(crate) fn range(
    doc: &Document,
    selected_track: Option<usize>,
    start: usize,
    end: usize,
) -> Result<TabScore> {
    if start > end || end >= doc.master_bars.len() {
        return Err(invalid(format!(
            "bar range {start}..={end} outside document"
        )));
    }
    let track_indices = match selected_track {
        Some(index) if index < doc.tracks.len() => vec![index],
        Some(index) => return Err(invalid(format!("track index {index} outside document"))),
        None => (0..doc.tracks.len()).collect(),
    };
    let tracks = track_indices
        .into_iter()
        .map(|track| extract_track(doc, track, start, end))
        .collect::<Result<Vec<_>>>()?;
    Ok(TabScore {
        title: doc.title.clone(),
        tracks,
    })
}

fn extract_track(doc: &Document, track_index: usize, start: usize, end: usize) -> Result<TabTrack> {
    let track = doc
        .tracks
        .get(track_index)
        .ok_or_else(|| invalid(format!("track index {track_index} outside document")))?;
    let percussion = track.tuning.is_empty();
    let bars = (start..=end)
        .map(|index| {
            let master = doc
                .master_bars
                .get(index)
                .ok_or_else(|| invalid(format!("bar index {index} outside document")))?;
            let bar_id = master
                .bar_ids
                .get(track_index)
                .copied()
                .ok_or_else(|| invalid(format!("bar {index} has no track {track_index}")))?;
            let bar = doc
                .bars
                .iter()
                .find(|candidate| candidate.id == u32::try_from(bar_id).unwrap_or(u32::MAX))
                .ok_or_else(|| invalid(format!("bar {index} references missing bar {bar_id}")))?;
            let beats = bar
                .voices
                .first()
                .map(|voice| {
                    voice
                        .beats
                        .iter()
                        .map(|beat| TabBeat {
                            rhythm: rhythm_label(
                                beat.rhythm.value,
                                beat.rhythm.dots,
                                beat.rhythm.tuplet,
                            ),
                            notes: if percussion {
                                vec![if beat.notes.is_empty() {
                                    None
                                } else {
                                    Some(
                                        beat.notes
                                            .iter()
                                            .map(percussion_note)
                                            .collect::<Vec<_>>()
                                            .join("/"),
                                    )
                                }]
                            } else {
                                (0..track.tuning.len())
                                    .rev()
                                    .map(|string| {
                                        note_at(
                                            &beat.notes,
                                            u32::try_from(string).unwrap_or(u32::MAX),
                                        )
                                        .map(fretted_note)
                                    })
                                    .collect()
                            },
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(TabBar {
                index,
                time: master.time,
                double_bar: master.double_bar,
                beats,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(TabTrack {
        name: track.name.clone(),
        tuning: track
            .tuning
            .iter()
            .rev()
            .map(|pitch| pitch_name(*pitch))
            .collect(),
        percussion,
        bars,
    })
}

fn empty_track(track: &Track) -> TabTrack {
    TabTrack {
        name: track.name.clone(),
        tuning: track
            .tuning
            .iter()
            .rev()
            .map(|pitch| pitch_name(*pitch))
            .collect(),
        percussion: track.tuning.is_empty(),
        bars: Vec::new(),
    }
}

fn note_at(notes: &[Note], string: u32) -> Option<&Note> {
    notes.iter().find(|note| note.string == Some(string))
}

fn fretted_note(note: &Note) -> String {
    note.fret
        .map(|fret| fret.to_string())
        .unwrap_or_else(|| "x".to_string())
}

fn percussion_note(note: &Note) -> String {
    note.articulation
        .or(note.midi)
        .map(|pitch| pitch.to_string())
        .unwrap_or_else(|| "x".to_string())
}

fn rhythm_label(value: NoteValue, dots: u8, tuplet: Option<(u32, u32)>) -> String {
    let base = match value {
        NoteValue::Whole => "w",
        NoteValue::Half => "h",
        NoteValue::Quarter => "q",
        NoteValue::Eighth => "e",
        NoteValue::Sixteenth => "s",
        NoteValue::ThirtySecond => "t",
        NoteValue::SixtyFourth => "x",
    };
    let mut label = base.to_string();
    label.push_str(&".".repeat(usize::from(dots)));
    if let Some((numerator, denominator)) = tuplet {
        label.push_str(&format!("[{numerator}:{denominator}]"));
    }
    label
}

fn pitch_name(pitch: i32) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let pitch_class = usize::try_from(pitch.rem_euclid(12)).unwrap_or(0);
    format!("{}{}", NAMES[pitch_class], pitch.div_euclid(12) - 1)
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Malformed(message.into())
}
