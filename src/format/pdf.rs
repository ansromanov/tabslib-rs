//! Deterministic vector PDF tablature rendering.
//!
//! The adapter writes a small PDF using the standard Helvetica base font and
//! line/text drawing operators. Keeping the writer here avoids a dependency
//! on a PDF runtime while still producing paginated, printable vector output.

use crate::error::Result;
use crate::format::tab::{self, TabScore, TabTrack};
use crate::format::WriteFormat;
use crate::model::Document;

const PAGE_WIDTH: f32 = 595.0;
const PAGE_HEIGHT: f32 = 842.0;
const MARGIN: f32 = 42.0;
const LINE_HEIGHT: f32 = 13.0;
const TRACK_GAP: f32 = 18.0;
const MAX_CELLS: usize = 18;

/// The print-ready PDF tablature rendering adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pdf;

impl WriteFormat for Pdf {
    const NAME: &'static str = "pdf";

    fn write(doc: &Document) -> Result<Vec<u8>> {
        render_score(doc)
    }
}

/// Renders every track and bar as deterministic PDF bytes.
pub fn render_score(doc: &Document) -> Result<Vec<u8>> {
    render(&tab::all(doc)?)
}

/// Renders every track in an inclusive master-bar range as PDF bytes.
pub fn render_score_range(doc: &Document, start: usize, end: usize) -> Result<Vec<u8>> {
    render(&tab::range(doc, None, start, end)?)
}

/// Renders one track and every bar as PDF bytes.
pub fn render_track(doc: &Document, track: usize) -> Result<Vec<u8>> {
    render(&tab::track(doc, track)?)
}

/// Renders one track in an inclusive master-bar range as PDF bytes.
pub fn render_track_range(
    doc: &Document,
    track: usize,
    start: usize,
    end: usize,
) -> Result<Vec<u8>> {
    render(&tab::range(doc, Some(track), start, end)?)
}

fn render(score: &TabScore) -> Result<Vec<u8>> {
    let mut pages = vec![String::new()];
    let mut y = PAGE_HEIGHT - MARGIN;
    add_text(&mut pages[0], MARGIN, y, 16.0, &score.title);
    y -= 28.0;
    for track in &score.tracks {
        let block = track_height(track);
        if y - block < MARGIN {
            pages.push(String::new());
            y = PAGE_HEIGHT - MARGIN;
        }
        add_text(current_page(&mut pages), MARGIN, y, 12.0, &track.name);
        y -= 16.0;
        if track.percussion {
            add_text(current_page(&mut pages), MARGIN, y, 8.0, "Percussion");
        } else {
            add_text(
                current_page(&mut pages),
                MARGIN,
                y,
                8.0,
                &format!("Tuning: {}", track.tuning.join(" ")),
            );
        }
        y -= 14.0;
        for bar in &track.bars {
            let chunks = bar.beats.chunks(MAX_CELLS).collect::<Vec<_>>();
            for (chunk_index, chunk) in chunks.iter().enumerate() {
                let rows = if track.percussion {
                    1
                } else {
                    track.tuning.len()
                };
                let needed = (rows as f32 + 2.0) * LINE_HEIGHT + 5.0;
                if y - needed < MARGIN {
                    pages.push(String::new());
                    y = PAGE_HEIGHT - MARGIN;
                }
                let label = format!(
                    "Bar {} {}/{}{}{}",
                    bar.index + 1,
                    bar.time.0,
                    bar.time.1,
                    if chunk_index == 0 { "" } else { " (continued)" },
                    if bar.double_bar { " ||" } else { "" }
                );
                add_text(current_page(&mut pages), MARGIN, y, 8.0, &label);
                y -= LINE_HEIGHT;
                let rhythm = chunk
                    .iter()
                    .map(|beat| beat.rhythm.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                add_text(
                    current_page(&mut pages),
                    MARGIN,
                    y,
                    7.0,
                    &format!("rhythm | {rhythm} |"),
                );
                y -= LINE_HEIGHT;
                for row in 0..rows {
                    let label = if track.percussion {
                        "dr".to_string()
                    } else {
                        track
                            .tuning
                            .get(row)
                            .cloned()
                            .unwrap_or_else(|| "?".to_string())
                    };
                    let values = chunk
                        .iter()
                        .map(|beat| {
                            beat.notes
                                .get(row)
                                .and_then(Option::as_deref)
                                .unwrap_or("-")
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    add_text(
                        current_page(&mut pages),
                        MARGIN,
                        y,
                        9.0,
                        &format!("{label} | {values} |"),
                    );
                    y -= LINE_HEIGHT;
                }
                y -= 5.0;
            }
        }
        y -= TRACK_GAP;
    }
    build_pdf(pages)
}

fn track_height(track: &TabTrack) -> f32 {
    let rows = if track.percussion {
        1
    } else {
        track.tuning.len()
    };
    let bar_chunks = track
        .bars
        .iter()
        .map(|bar| bar.beats.len().max(1).div_ceil(MAX_CELLS))
        .sum::<usize>();
    30.0 + (bar_chunks as f32 * ((rows as f32 + 2.0) * LINE_HEIGHT + 5.0)) + TRACK_GAP
}

fn current_page(pages: &mut [String]) -> &mut String {
    let index = pages.len() - 1;
    &mut pages[index]
}

fn add_text(content: &mut String, x: f32, y: f32, size: f32, text: &str) {
    content.push_str("BT /F1 ");
    content.push_str(&format_number(size));
    content.push_str(" Tf ");
    content.push_str(&format_number(x));
    content.push(' ');
    content.push_str(&format_number(y));
    content.push_str(" Td (");
    content.push_str(&pdf_text(text));
    content.push_str(") Tj ET\n");
}

fn build_pdf(pages: Vec<String>) -> Result<Vec<u8>> {
    let page_count = pages.len().max(1);
    let first_page_id = 3usize;
    let first_content_id = first_page_id + page_count;
    let font_id = first_content_id + page_count;
    let object_count = font_id;
    let mut objects = vec![Vec::new(); object_count];
    objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
    let kids = (0..page_count)
        .map(|index| format!("{} 0 R", first_page_id + index))
        .collect::<Vec<_>>()
        .join(" ");
    objects[1] = format!("<< /Type /Pages /Kids [{kids}] /Count {page_count} >>").into_bytes();
    for (index, page) in pages.iter().enumerate() {
        let page_id = first_page_id + index;
        let content_id = first_content_id + index;
        objects[page_id - 1] = format!(
            concat!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] ",
                "/Resources << /Font << /F1 {} 0 R >> >> ",
                "/Contents {} 0 R >>"
            ),
            PAGE_WIDTH, PAGE_HEIGHT, font_id, content_id,
        )
        .into_bytes();
        let stream = page.as_bytes();
        objects[content_id - 1] = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        objects[content_id - 1].extend_from_slice(stream);
        objects[content_id - 1].extend_from_slice(b"endstream");
    }
    objects[font_id - 1] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec();

    let mut output = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(object_count + 1);
    offsets.push(0usize);
    for (index, object) in objects.iter().enumerate() {
        offsets.push(output.len());
        output.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        output.extend_from_slice(object);
        output.extend_from_slice(b"\nendobj\n");
    }
    let xref = output.len();
    output.extend_from_slice(format!("xref\n0 {}\n", object_count + 1).as_bytes());
    output.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    output.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            object_count + 1
        )
        .as_bytes(),
    );
    Ok(output)
}

fn format_number(value: f32) -> String {
    format!("{value:.2}")
}

fn pdf_text(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '(' => "\\(".to_string(),
            ')' => "\\)".to_string(),
            '\\' => "\\\\".to_string(),
            character if character.is_ascii() && !character.is_ascii_control() => {
                character.to_string()
            }
            _ => "?".to_string(),
        })
        .collect()
}

#[cfg(test)]
#[path = "pdf_test.rs"]
mod tests;
