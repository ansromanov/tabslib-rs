//! MusicXML score export.

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::error::{Error, Result};
use crate::format::ReadFormat;
use crate::format::WriteFormat;
use crate::model::{
    Bar, Beat, Document, KeySignature, MasterBar, Note, NoteValue, Rhythm, Track, Voice,
};

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
                                if let (Some(string), Some(fret)) = (note.string, note.fret) {
                                    xml.push_str(&format!(
                                        "<notations><technical><string>{string}</string><fret>{fret}</fret></technical></notations>"
                                    ));
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

fn rhythm_from_ticks(ticks: u32) -> Rhythm {
    let value = match ticks {
        1920 => NoteValue::Whole,
        960 => NoteValue::Half,
        480 => NoteValue::Quarter,
        240 => NoteValue::Eighth,
        120 => NoteValue::Sixteenth,
        60 => NoteValue::ThirtySecond,
        30 => NoteValue::SixtyFourth,
        _ => NoteValue::Quarter,
    };
    Rhythm::new(value)
}

fn tonic_from_fifths(fifths: i32) -> i8 {
    (fifths * 7).rem_euclid(12) as i8
}

#[derive(Default)]
struct XmlNote {
    midi: Option<i32>,
    string: Option<u32>,
    fret: Option<i32>,
    rest: bool,
}

/// Imports the common score-partwise subset emitted by [`MusicXml::write`].
///
/// ```
/// use tabslib::format::musicxml::MusicXml;
/// use tabslib::format::ReadFormat;
///
/// let xml = br#"<?xml version="1.0"?><score-partwise><part-list><score-part id="P1"><part-name>Guitar</part-name></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>480</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes><note><rest/><duration>480</duration><voice>1</voice></note></measure></part></score-partwise>"#;
/// let score = MusicXml::read(xml).unwrap();
/// assert_eq!(score.tracks.len(), 1);
/// ```
pub fn read(bytes: &[u8]) -> Result<Document> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut names = Vec::new();
    let mut parts: Vec<Vec<Bar>> = Vec::new();
    let mut current_part = None;
    let mut current_bar = None;
    let mut current_note: Option<XmlNote> = None;
    let mut current_duration = 480;
    let mut current_chord = false;
    let mut tag = String::new();
    let mut next_id = 0u32;
    let mut time = (4, 4);
    let mut divisions = 480u32;
    let mut key_fifths = None;
    let mut key_minor = false;
    let mut clef_sign = "G".to_string();
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| Error::format("musicxml", e))?
        {
            Event::Start(event) => {
                tag = String::from_utf8_lossy(event.name().as_ref()).into_owned();
                match tag.as_str() {
                    "part" => {
                        current_part = Some(parts.len());
                        parts.push(Vec::new());
                    }
                    "measure" => {
                        current_bar = Some(Bar {
                            id: next_id,
                            clef: Some("G2".into()),
                            voices: vec![Voice {
                                id: next_id,
                                beats: Vec::new(),
                            }],
                        });
                        next_id += 1;
                    }
                    "note" => {
                        current_note = Some(XmlNote::default());
                        current_duration = 480;
                        current_chord = false;
                    }
                    _ => {}
                }
            }
            Event::Empty(event) => match event.name().as_ref() {
                b"rest" => {
                    current_note = Some(XmlNote {
                        rest: true,
                        ..Default::default()
                    });
                }
                b"chord" => current_chord = true,
                _ => {}
            },
            Event::Text(text) => {
                let value = text
                    .unescape()
                    .map_err(|e| Error::format("musicxml", e))?
                    .into_owned();
                match tag.as_str() {
                    "part-name" => names.push(value),
                    "beats" => time.0 = value.parse().unwrap_or(4),
                    "beat-type" => time.1 = value.parse().unwrap_or(4),
                    "divisions" => divisions = value.parse().unwrap_or(480),
                    "fifths" => key_fifths = value.parse().ok(),
                    "mode" => key_minor = value == "minor",
                    "sign" => clef_sign = value,
                    "line" => {
                        if let Some(bar) = current_bar.as_mut() {
                            bar.clef = Some(format!("{clef_sign}{value}"));
                        }
                    }
                    "step" => {
                        if let Some(note) = current_note.as_mut() {
                            note.midi = Some(match value.as_str() {
                                "C" => 0,
                                "D" => 2,
                                "E" => 4,
                                "F" => 5,
                                "G" => 7,
                                "A" => 9,
                                "B" => 11,
                                _ => 0,
                            });
                        }
                    }
                    "alter" => {
                        if let Some(note) = current_note.as_mut() {
                            if let Some(midi) = note.midi.as_mut() {
                                *midi += value.parse::<i32>().unwrap_or(0);
                            }
                        }
                    }
                    "octave" => {
                        if let Some(note) = current_note.as_mut() {
                            note.midi = Some(
                                note.midi.unwrap_or(0)
                                    + value.parse::<i32>().unwrap_or(4) * 12
                                    + 12,
                            );
                        }
                    }
                    "duration" => {
                        let raw = value.parse::<u32>().unwrap_or(divisions);
                        current_duration = raw.saturating_mul(480) / divisions.max(1);
                    }
                    "string" => {
                        if let Some(note) = current_note.as_mut() {
                            note.string = value.parse().ok();
                        }
                    }
                    "fret" => {
                        if let Some(note) = current_note.as_mut() {
                            note.fret = value.parse().ok();
                        }
                    }
                    _ => {}
                }
            }
            Event::End(event) => {
                let ended = String::from_utf8_lossy(event.name().as_ref()).into_owned();
                if ended == "note" {
                    if let Some(note) = current_note.take() {
                        if let Some(bar) = current_bar.as_mut() {
                            let beat = Beat {
                                id: next_id,
                                rhythm: rhythm_from_ticks(current_duration),
                                notes: if note.rest {
                                    Vec::new()
                                } else {
                                    vec![Note {
                                        id: next_id,
                                        midi: note.midi,
                                        string: note.string,
                                        fret: note.fret,
                                        articulation: None,
                                        techniques: Vec::new(),
                                    }]
                                },
                                dynamic: None,
                            };
                            next_id += 1;
                            if current_chord {
                                if let Some(previous) = bar.voices[0].beats.last_mut() {
                                    previous.notes.extend(beat.notes);
                                } else {
                                    bar.voices[0].beats.push(beat);
                                }
                            } else {
                                bar.voices[0].beats.push(beat);
                            }
                        }
                    }
                } else if ended == "measure" {
                    if let (Some(part), Some(bar)) = (current_part, current_bar.take()) {
                        parts[part].push(bar);
                    }
                }
                tag.clear();
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if parts.is_empty() {
        return Err(Error::Malformed("MusicXML contains no parts".into()));
    }
    let count = parts.iter().map(Vec::len).max().unwrap_or(0);
    let tracks = parts
        .iter()
        .enumerate()
        .map(|(i, _)| Track {
            id: i as u32,
            name: names
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("Part {}", i + 1)),
            color: None,
            tuning: vec![40, 45, 50, 55, 59, 64],
            midi_program: None,
            pan: None,
            volume: None,
            mute: false,
            solo: false,
            percussion_articulations: Vec::new(),
        })
        .collect();
    let master_bars = (0..count)
        .map(|i| MasterBar {
            index: i,
            time,
            section: None,
            double_bar: false,
            bar_ids: parts
                .iter()
                .map(|part| part.get(i).map(|bar| bar.id as i32).unwrap_or(-1))
                .collect(),
            repeat_start: false,
            repeat_end: None,
            alternate_ending: 0,
            direction: None,
        })
        .collect();
    let bars = parts.into_iter().flatten().collect();
    Ok(Document {
        title: String::new(),
        artist: String::new(),
        tracks,
        master_bars,
        bars,
        tempo_map: Vec::new(),
        key: key_fifths.map(|fifths| KeySignature {
            tonic: tonic_from_fifths(fifths),
            minor: key_minor,
        }),
        source: None,
    })
}

impl ReadFormat for MusicXml {
    const NAME: &'static str = "musicxml";
    fn detect(bytes: &[u8]) -> bool {
        bytes.windows(15).any(|window| window == b"score-partwise")
    }
    fn read(bytes: &[u8]) -> Result<Document> {
        read(bytes)
    }
}
