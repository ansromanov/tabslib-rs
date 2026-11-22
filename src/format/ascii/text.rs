//! Plain-text drawing of a [`TrackLayout`].
//!
//! Owns [`render_layout`]. It is a reference renderer for the layout API: every
//! position it uses comes from the layout, so a terminal UI drawing the same
//! layout with colour lines up with it column for column.
//!
//! Each system is a header row (bar number, repeat and ending marks, a changed
//! time signature, a tempo change as `q=120`, the section in brackets), one row
//! per technique present in the system (`PM`, `LR`, `~`...), then the staff.

use std::ops::Range;

use super::cell::Annotation;
use super::layout::{wrap, BarLayout, TrackLayout};
use crate::model::HarmonicKind;

/// Fixed top-to-bottom order of annotation rows.
const ROW_ORDER: [Annotation; 12] = [
    Annotation::PalmMute,
    Annotation::LetRing,
    Annotation::Vibrato,
    Annotation::Tapped,
    Annotation::Harmonic(HarmonicKind::Natural),
    Annotation::Harmonic(HarmonicKind::Artificial),
    Annotation::Harmonic(HarmonicKind::Pinch),
    Annotation::Harmonic(HarmonicKind::Tap),
    Annotation::Harmonic(HarmonicKind::Semi),
    Annotation::Harmonic(HarmonicKind::Feedback),
    Annotation::Accent,
    Annotation::Staccato,
];

fn bar_heading(bar: &BarLayout) -> String {
    let mut parts = vec![(bar.index + 1).to_string()];
    if bar.repeat_start {
        parts.push("|:".to_string());
    }
    if !bar.alternate_endings.is_empty() {
        let endings: String = bar
            .alternate_endings
            .iter()
            .map(|ending| format!("{ending}."))
            .collect();
        parts.push(endings);
    }
    if bar.time_changed {
        parts.push(format!("{}/{}", bar.time.0, bar.time.1));
    }
    if let Some(tempo) = bar.tempo_change {
        parts.push(format!("q={}", tempo.bpm));
    }
    if let Some(section) = &bar.section {
        parts.push(format!("[{section}]"));
    }
    if let Some(direction) = &bar.direction {
        parts.push(direction.clone());
    }
    if let Some(count) = bar.repeat_end {
        parts.push(format!(":|x{count}"));
    }
    if bar.double_bar {
        parts.push("||".to_string());
    }
    parts.join(" ")
}

fn put(row: &mut Vec<char>, column: usize, text: &str) {
    for (offset, character) in text.chars().enumerate() {
        let at = column + offset;
        if at >= row.len() {
            row.resize(at + 1, ' ');
        }
        if let Some(slot) = row.get_mut(at) {
            *slot = character;
        }
    }
}

fn header_row(layout: &TrackLayout, bars: &[BarLayout]) -> String {
    let mut row: Vec<char> = Vec::new();
    let mut column = layout.label_width() + 1;
    let mut cursor = 0;
    for bar in bars {
        let at = column.max(cursor);
        let heading = bar_heading(bar);
        put(&mut row, at, &heading);
        cursor = at + heading.chars().count() + 1;
        column += bar.width + 1;
    }
    row.into_iter().collect::<String>().trim_end().to_string()
}

fn annotation_row(layout: &TrackLayout, bars: &[BarLayout], kind: Annotation) -> String {
    let label_width = layout.label_width();
    let mut row = format!("{:<label_width$} ", kind.label());
    for bar in bars {
        let mut content = vec![' '; bar.width];
        let (mark, fill) = match kind {
            Annotation::Vibrato => ('~', '~'),
            Annotation::PalmMute
            | Annotation::LetRing
            | Annotation::Tapped
            | Annotation::Accent
            | Annotation::Staccato
            | Annotation::Harmonic(_) => ('*', '-'),
        };
        for (index, beat) in bar.beats.iter().enumerate() {
            if !beat.annotations.contains(&kind) {
                continue;
            }
            if let Some(slot) = content.get_mut(beat.column) {
                *slot = mark;
            }
            let continues = bar
                .beats
                .get(index + 1)
                .is_some_and(|next| next.annotations.contains(&kind));
            if continues {
                for slot in content
                    .iter_mut()
                    .skip(beat.column + 1)
                    .take(beat.width.saturating_sub(1))
                {
                    *slot = fill;
                }
            }
        }
        row.extend(content);
        row.push(' ');
    }
    row.trim_end().to_string()
}

fn staff_row(layout: &TrackLayout, bars: &[BarLayout], line: usize) -> String {
    let label_width = layout.label_width();
    let label = layout.labels.get(line).map(String::as_str).unwrap_or("");
    let mut row = format!("{label:<label_width$}|");
    for bar in bars {
        let mut content = vec!['-'; bar.width];
        for beat in &bar.beats {
            let Some(Some(cell)) = beat.cells.get(line) else {
                continue;
            };
            let mut text = cell.text.clone();
            if let Some(connector) = cell.connector {
                if cell.width() < beat.width {
                    text.push(connector.symbol());
                }
            }
            for (offset, character) in text.chars().enumerate() {
                if let Some(slot) = content.get_mut(beat.column + offset) {
                    *slot = character;
                }
            }
        }
        row.extend(content);
        row.push('|');
    }
    row
}

fn render_system(layout: &TrackLayout, range: Range<usize>) -> Vec<String> {
    let bars = layout.bars.get(range).unwrap_or(&[]);
    let mut rows = vec![header_row(layout, bars)];
    for kind in ROW_ORDER {
        if bars
            .iter()
            .flat_map(|bar| bar.beats.iter())
            .any(|beat| beat.annotations.contains(&kind))
        {
            rows.push(annotation_row(layout, bars, kind));
        }
    }
    rows.extend((0..layout.lines.len()).map(|line| staff_row(layout, bars, line)));
    rows
}

/// Draws a layout as plain text, wrapped into systems of at most `width`
/// columns (see [`wrap`]), separated by blank lines.
pub fn render_layout(layout: &TrackLayout, width: usize) -> String {
    wrap(layout, width)
        .into_iter()
        .map(|range| render_system(layout, range).join("\n"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
#[path = "text_test.rs"]
mod tests;
