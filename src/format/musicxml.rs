//! MusicXML score export.

use crate::error::Result;
use crate::format::WriteFormat;
use crate::model::{Document, Note};

/// MusicXML export adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct MusicXml;

fn pitch(note: &Note) -> Option<(String, i32, i32)> {
    let midi = note.midi?;
    let names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let name = names[midi.rem_euclid(12) as usize];
    Some((
        name.trim_end_matches('#').to_string(),
        midi / 12 - 1,
        i32::from(name.ends_with('#')),
    ))
}

/// Serialises a document to a compact, deterministic MusicXML score.
pub fn write(doc: &Document) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<score-partwise version=\"4.0\">",
    );
    xml.push_str("<part-list>");
    for (index, track) in doc.tracks.iter().enumerate() {
        xml.push_str(&format!(
            "<score-part id=\"P{}\"><part-name>{}</part-name></score-part>",
            index + 1,
            escape(&track.name)
        ));
    }
    xml.push_str("</part-list>");
    for (track_index, _track) in doc.tracks.iter().enumerate() {
        xml.push_str(&format!("<part id=\"P{}\">", track_index + 1));
        for (bar_index, master) in doc.master_bars.iter().enumerate() {
            xml.push_str(&format!("<measure number=\"{}\">", bar_index + 1));
            if bar_index == 0 {
                xml.push_str(&format!("<attributes><divisions>480</divisions><time><beats>{}</beats><beat-type>{}</beat-type></time></attributes>", master.time.0, master.time.1));
            }
            let id = master.bar_ids.get(track_index).copied().unwrap_or(-1);
            if let Some(bar) = doc.bars.iter().find(|bar| bar.id == id as u32) {
                if let Some(voice) = bar.voices.first() {
                    for beat in &voice.beats {
                        let (n, d) = beat.rhythm.as_fraction();
                        let duration = n.saturating_mul(1920) / d;
                        if beat.notes.is_empty() {
                            xml.push_str(&format!("<note><rest/><duration>{duration}</duration><voice>1</voice></note>"));
                        } else {
                            for (chord, note) in beat.notes.iter().enumerate() {
                                xml.push_str("<note>");
                                if chord > 0 {
                                    xml.push_str("<chord/>");
                                }
                                if let Some((step, octave, alter)) = pitch(note) {
                                    xml.push_str(&format!("<pitch><step>{step}</step>"));
                                    if alter != 0 {
                                        xml.push_str("<alter>1</alter>");
                                    }
                                    xml.push_str(&format!("<octave>{octave}</octave></pitch>"));
                                } else {
                                    xml.push_str("<rest/>");
                                }
                                xml.push_str(&format!(
                                    "<duration>{duration}</duration><voice>1</voice></note>"
                                ));
                            }
                        }
                    }
                }
            }
            xml.push_str("</measure>");
        }
        xml.push_str("</part>");
    }
    xml.push_str("</score-partwise>");
    xml
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

impl WriteFormat for MusicXml {
    const NAME: &'static str = "musicxml";

    fn write(doc: &Document) -> Result<Vec<u8>> {
        Ok(write(doc).into_bytes())
    }
}
