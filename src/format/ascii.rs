//! Lossy, deterministic ASCII tablature rendering, and the structured layout
//! terminal renderers draw from.
//!
//! Two surfaces live here:
//!
//! - [`layout_track`] and its types ([`TrackLayout`], [`BarLayout`],
//!   [`BeatLayout`], [`Cell`], [`Line`]) place every bar, beat and note on a
//!   character grid without drawing it, so a UI can colour notes, highlight the
//!   playing beat ([`beat_at`]) and stack systems ([`wrap`]). [`render_layout`]
//!   draws one as plain text.
//! - [`render_track`], [`render_compare`] and the [`Ascii`] writer keep the
//!   compact numbered-string format (`s0` is the lowest string) that prompts
//!   and golden tests refer to by string number. It shows the first voice only,
//!   one fixed-width cell per beat.

#[path = "ascii/cell.rs"]
mod cell;
#[path = "ascii/layout.rs"]
mod layout;
#[path = "ascii/lines.rs"]
mod lines;
#[path = "ascii/text.rs"]
mod text;

pub use cell::{Annotation, Cell, Connector};
pub use layout::{beat_at, layout_track, wrap, BarLayout, BeatLayout, LayoutOptions, TrackLayout};
pub use lines::{Line, MAX_DRUM_LABEL};
pub use text::render_layout;

use crate::error::{Error, Result};
use crate::format::WriteFormat;
use crate::model::{Document, Note, Track};

/// The ASCII tablature rendering adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ascii;

impl WriteFormat for Ascii {
    const NAME: &'static str = "ascii";

    fn write(doc: &Document) -> Result<Vec<u8>> {
        if doc.master_bars.is_empty() {
            return Ok(Vec::new());
        }
        let mut output = String::new();
        for track in 0..doc.tracks.len() {
            if track > 0 {
                output.push('\n');
            }
            output.push_str(&render_track(doc, track, 0, doc.master_bars.len() - 1)?);
        }
        Ok(output.into_bytes())
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Malformed(message.into())
}

fn selected_track(doc: &Document, track: usize) -> Result<&Track> {
    doc.tracks
        .get(track)
        .ok_or_else(|| invalid(format!("track index {track} outside document")))
}

fn check_range(doc: &Document, start: usize, end: usize) -> Result<()> {
    if start > end || end >= doc.master_bars.len() {
        return Err(invalid(format!(
            "bar range {start}..={end} outside document"
        )));
    }
    Ok(())
}

fn bar_for(doc: &Document, bar: usize, track: usize) -> Option<&crate::model::Bar> {
    let id = *doc.master_bars[bar].bar_ids.get(track)?;
    doc.bars.iter().find(|candidate| candidate.id == id as u32)
}

fn note_at(notes: &[Note], string: u32) -> Option<&Note> {
    notes.iter().find(|note| note.string == Some(string))
}

fn note_text(note: Option<&Note>) -> String {
    match note.and_then(|note| note.fret) {
        Some(fret) => fret.to_string(),
        None if note.is_some() => "x".to_string(),
        None => "-".to_string(),
    }
}

fn render_lines(doc: &Document, track: usize, start: usize, end: usize) -> Result<Vec<String>> {
    let track_data = selected_track(doc, track)?;
    check_range(doc, start, end)?;
    // Highest-pitched string on the top line, as written tablature is read.
    // String numbers are 0-based from the *lowest* string, so the display order
    // is their reverse -- ordering by string number puts the tab upside down.
    let strings = if track_data.tuning.is_empty() {
        vec![None]
    } else {
        (0..track_data.tuning.len() as u32)
            .rev()
            .map(Some)
            .collect::<Vec<_>>()
    };
    let mut lines = vec![format!("{} [{}..={}]", track_data.name, start + 1, end + 1)];
    for string in strings {
        let label = match string {
            Some(number) => format!("s{number}"),
            None => "dr".to_string(),
        };
        let mut line = format!("{label:<2}|");
        for bar_index in start..=end {
            let bar = bar_for(doc, bar_index, track);
            let beats = bar
                .map(|bar| {
                    bar.voices
                        .first()
                        .map(|voice| voice.beats.as_slice())
                        .unwrap_or(&[])
                })
                .unwrap_or(&[]);
            for beat in beats {
                let note = string.and_then(|number| note_at(&beat.notes, number));
                let text = if string.is_none() {
                    if beat.notes.is_empty() {
                        "-".to_string()
                    } else {
                        "x".to_string()
                    }
                } else {
                    note_text(note)
                };
                line.push_str(&format!("{text:^4}"));
            }
            line.push('|');
        }
        lines.push(line);
    }
    Ok(lines)
}

/// Renders one track and an inclusive master-bar range as ASCII tablature.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `track` is not a track of `doc`, or if `start > end` or
/// `end` is past the last master bar.
pub fn render_track(doc: &Document, track: usize, start: usize, end: usize) -> Result<String> {
    render_lines(doc, track, start, end).map(|lines| lines.join("\n"))
}

/// Renders two inclusive bar windows side by side for visual comparison.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `track` is not a track of `doc`, or if either window has
/// `start > end` or an `end` past the last master bar.
pub fn render_compare(
    doc: &Document,
    track: usize,
    left: (usize, usize),
    right: (usize, usize),
) -> Result<String> {
    let left_lines = render_lines(doc, track, left.0, left.1)?;
    let right_lines = render_lines(doc, track, right.0, right.1)?;
    let width = left_lines.iter().map(String::len).max().unwrap_or(0);
    let mut output = Vec::new();
    for index in 0..left_lines.len().max(right_lines.len()) {
        let left_line = left_lines.get(index).map(String::as_str).unwrap_or("");
        let right_line = right_lines.get(index).map(String::as_str).unwrap_or("");
        output.push(format!("{left_line:<width$} | {right_line}"));
    }
    Ok(output.join("\n"))
}
