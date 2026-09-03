//! Lossy, deterministic ASCII tablature rendering.

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
    let strings = if track_data.tuning.is_empty() {
        vec![None]
    } else {
        (1..=track_data.tuning.len() as u32)
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
pub fn render_track(doc: &Document, track: usize, start: usize, end: usize) -> Result<String> {
    render_lines(doc, track, start, end).map(|lines| lines.join("\n"))
}

/// Renders two inclusive bar windows side by side for visual comparison.
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
