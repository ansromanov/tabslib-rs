//! Standalone HTML tablature rendering.
//!
//! This write-only adapter produces a self-contained document with semantic
//! headings, responsive tablature tables, tuning labels, rhythm markers and
//! bar boundaries. It owns no score data and does not read HTML back.

use crate::error::Result;
use crate::format::tab::{self, TabScore, TabTrack};
use crate::format::WriteFormat;
use crate::model::Document;

/// The standalone HTML tablature rendering adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Html;

impl WriteFormat for Html {
    const NAME: &'static str = "html";

    fn write(doc: &Document) -> Result<Vec<u8>> {
        render_score(doc).map(String::into_bytes)
    }
}

/// Renders every track and bar as a standalone HTML document.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if a master bar has no entry for a track or refers to a
/// bar that does not exist.
pub fn render_score(doc: &Document) -> Result<String> {
    render(&tab::all(doc)?)
}

/// Renders every track in an inclusive master-bar range.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `start > end` or `end` is past the last master bar, or
/// if a bar in the range is missing for a track or refers to a bar that does not exist.
pub fn render_score_range(doc: &Document, start: usize, end: usize) -> Result<String> {
    render(&tab::range(doc, None, start, end)?)
}

/// Renders one track and every bar as a standalone HTML document.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `track` is not a track of `doc`, or if a master bar has
/// no entry for the track or refers to a bar that does not exist.
pub fn render_track(doc: &Document, track: usize) -> Result<String> {
    render(&tab::track(doc, track)?)
}

/// Renders one track in an inclusive master-bar range.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `track` is not a track of `doc`, if `start > end` or
/// `end` is past the last master bar, or if a bar in the range is missing for the track or
/// refers to a bar that does not exist.
pub fn render_track_range(
    doc: &Document,
    track: usize,
    start: usize,
    end: usize,
) -> Result<String> {
    render(&tab::range(doc, Some(track), start, end)?)
}

fn render(score: &TabScore) -> Result<String> {
    const STYLE: &str = concat!(
        "<style>",
        ":root{color-scheme:light dark}",
        "body{font:16px system-ui,sans-serif;line-height:1.35;margin:2rem;",
        "color:#222;background:#fff}",
        "main{max-width:1200px;margin:auto}",
        "h1{font-size:1.8rem}",
        "h2{margin-top:2rem;border-bottom:2px solid currentColor}",
        ".tab-wrap{overflow-x:auto}",
        "table{border-collapse:collapse;min-width:max-content;margin:1rem 0 2rem}",
        "caption{text-align:left;font-weight:600;padding:.5rem 0}",
        "th,td{border-bottom:1px solid #bbb;padding:.25rem .45rem;text-align:center;",
        "white-space:nowrap}",
        "th[scope=row]{text-align:right;position:sticky;left:0;background:#fff}",
        ".rhythm{font-size:.75rem;color:#555}",
        "td.bar-end{border-right:2px solid currentColor}",
        "@media print{body{margin:1cm;color:#000;background:#fff}",
        "h2{break-after:avoid}.tab-wrap{overflow:visible}table{break-inside:avoid}}",
        "@media(prefers-color-scheme:dark){body{color:#eee;background:#111}",
        "th[scope=row]{background:#111}.rhythm{color:#bbb}}",
        "</style>",
    );
    let mut html = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" ",
    );
    html.push_str("content=\"width=device-width, initial-scale=1\"><title>");
    html.push_str(&escape(&score.title));
    html.push_str("</title>");
    html.push_str(STYLE);
    html.push_str("</head><body><main><h1>");
    html.push_str(&escape(&score.title));
    html.push_str("</h1>");
    for track in &score.tracks {
        render_track_html(&mut html, track);
    }
    html.push_str("</main></body></html>");
    Ok(html)
}

fn render_track_html(html: &mut String, track: &TabTrack) {
    html.push_str("<section><h2>");
    html.push_str(&escape(&track.name));
    html.push_str("</h2><div class=\"tab-wrap\"><table><caption>");
    if track.percussion {
        html.push_str("Percussion");
    } else {
        html.push_str("Tuning: ");
        html.push_str(&escape(&track.tuning.join(" ")));
    }
    html.push_str("</caption><tbody><tr class=\"bar-label\"><th scope=\"row\">bar</th>");
    for bar in &track.bars {
        html.push_str("<th scope=\"col\" colspan=\"");
        html.push_str(&bar.beats.len().to_string());
        html.push_str("\">Bar ");
        html.push_str(&(bar.index + 1).to_string());
        html.push(' ');
        html.push_str(&bar.time.0.to_string());
        html.push('/');
        html.push_str(&bar.time.1.to_string());
        html.push_str("</th>");
    }
    html.push_str("</tr>");
    let rows = if track.percussion {
        1
    } else {
        track.tuning.len()
    };
    for row in 0..rows {
        html.push_str("<tr><th scope=\"row\">");
        html.push_str(if track.percussion {
            "dr"
        } else {
            track.tuning.get(row).map(String::as_str).unwrap_or("?")
        });
        html.push_str("</th>");
        for bar in &track.bars {
            for beat in &bar.beats {
                html.push_str("<td");
                if beat_is_last(bar, beat) {
                    html.push_str(" class=\"bar-end\"");
                }
                html.push('>');
                html.push_str(&escape(
                    beat.notes
                        .get(row)
                        .and_then(Option::as_deref)
                        .unwrap_or("-"),
                ));
                html.push_str("</td>");
            }
        }
        html.push_str("</tr>");
    }
    html.push_str("<tr class=\"rhythm\"><th scope=\"row\">rhythm</th>");
    for bar in &track.bars {
        for beat in &bar.beats {
            html.push_str("<td");
            if beat_is_last(bar, beat) {
                html.push_str(" class=\"bar-end\"");
            }
            html.push('>');
            html.push_str(&escape(&beat.rhythm));
            html.push_str("</td>");
        }
    }
    html.push_str("</tr></tbody></table></div></section>");
}

fn beat_is_last(bar: &crate::format::tab::TabBar, beat: &crate::format::tab::TabBeat) -> bool {
    bar.beats
        .last()
        .map(|last| std::ptr::eq(last, beat))
        .unwrap_or(true)
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
#[path = "html_test.rs"]
mod tests;
