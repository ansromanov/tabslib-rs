//! Standard MIDI file import and export.

use crate::error::{Error, Result};
use crate::format::{ReadFormat, WriteFormat};
use crate::model::{Bar, Beat, Document, MasterBar, Note, Rhythm, Track, Voice};

/// Standard MIDI file adapter using format 1 and 480 ticks per quarter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Midi;

fn varint(mut value: u32) -> Vec<u8> {
    let mut bytes = vec![value as u8 & 0x7f];
    while {
        value >>= 7;
        value != 0
    } {
        bytes.push((value as u8 & 0x7f) | 0x80);
    }
    bytes.reverse();
    bytes
}
fn push_event(track: &mut Vec<u8>, delta: u32, status: u8, a: u8, b: u8) {
    track.extend(varint(delta));
    track.extend([status, a, b]);
}
fn ticks(rhythm: Rhythm) -> u32 {
    let (n, d) = rhythm.as_fraction();
    (n.saturating_mul(1920) / d) as u32
}

/// Exports a deterministic format-1 MIDI file.
pub fn write(doc: &Document) -> Vec<u8> {
    let mut output = b"MThd".to_vec();
    output.extend(6u32.to_be_bytes());
    output.extend(1u16.to_be_bytes());
    output.extend((doc.tracks.len() as u16 + 1).to_be_bytes());
    output.extend(480u16.to_be_bytes());
    let mut tempo = Vec::new();
    tempo.extend([0, 0xff, 0x51, 3, 0x07, 0xa1, 0x20, 0, 0xff, 0x2f, 0]);
    output.extend(b"MTrk");
    output.extend((tempo.len() as u32).to_be_bytes());
    output.extend(tempo);
    for (track_index, _track) in doc.tracks.iter().enumerate() {
        let mut absolute = Vec::new();
        let mut cursor = 0;
        for master_index in crate::inspect::playback_order(doc) {
            let master = &doc.master_bars[master_index];
            let bar_id = *master.bar_ids.get(track_index).unwrap_or(&-1);
            if let Some(bar) = doc.bars.iter().find(|bar| bar.id == bar_id as u32) {
                if let Some(voice) = bar.voices.first() {
                    for beat in &voice.beats {
                        let duration = ticks(beat.rhythm);
                        for note in &beat.notes {
                            let midi = note
                                .articulation
                                .and_then(|raw| {
                                    doc.tracks[track_index]
                                        .percussion_articulations
                                        .iter()
                                        .find(|mapping| mapping.raw_id == raw)
                                        .map(|mapping| mapping.midi)
                                        .or_else(|| crate::percussion::midi_note(raw))
                                })
                                .map(i32::from)
                                .or(note.midi)
                                .filter(|n| (0..=127).contains(n));
                            if let Some(midi) = midi {
                                absolute.push((cursor, 0x90, midi as u8, 100));
                                absolute.push((cursor + duration, 0x80, midi as u8, 0));
                            }
                        }
                        cursor += duration;
                    }
                }
            }
        }
        absolute.sort_by_key(|(at, status, _, _)| (*at, *status));
        let mut events = Vec::new();
        let mut previous = 0;
        for (at, status, pitch, velocity) in absolute {
            push_event(
                &mut events,
                at.saturating_sub(previous),
                status,
                pitch,
                velocity,
            );
            previous = at;
        }
        events.extend([0, 0xff, 0x2f, 0]);
        output.extend(b"MTrk");
        output.extend((events.len() as u32).to_be_bytes());
        output.extend(events);
    }
    output
}

fn read_u32(bytes: &[u8], at: &mut usize) -> Option<u32> {
    let v = bytes.get(*at..*at + 4)?.try_into().ok()?;
    *at += 4;
    Some(u32::from_be_bytes(v))
}
fn read_var(bytes: &[u8], at: &mut usize) -> Option<u32> {
    let mut v = 0;
    for _ in 0..4 {
        let b = *bytes.get(*at)?;
        *at += 1;
        v = (v << 7) | u32::from(b & 0x7f);
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

/// Imports note-on events from a standard MIDI file into one 4/4 track.
pub fn read(bytes: &[u8]) -> Result<Document> {
    if !bytes.starts_with(b"MThd") {
        return Err(Error::Malformed("missing MIDI header".into()));
    }
    let mut at = 4;
    let header_len = read_u32(bytes, &mut at)
        .ok_or_else(|| Error::Malformed("truncated MIDI header".into()))?
        as usize;
    if header_len < 6 || bytes.len() < at + header_len {
        return Err(Error::Malformed("invalid MIDI header".into()));
    }
    let _format = u16::from_be_bytes([bytes[at], bytes[at + 1]]);
    let tracks = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
    let division = u16::from_be_bytes([bytes[at + 4], bytes[at + 5]]) as u32;
    at += header_len;
    let mut notes = Vec::new();
    let mut id = 0;
    let mut max_tick = 0;
    for _ in 0..tracks {
        if bytes.get(at..at + 4) != Some(b"MTrk") {
            break;
        }
        at += 4;
        let len = read_u32(bytes, &mut at)
            .ok_or_else(|| Error::Malformed("truncated MIDI track".into()))?
            as usize;
        let end = at
            .checked_add(len)
            .ok_or_else(|| Error::Malformed("MIDI track too large".into()))?;
        if end > bytes.len() {
            return Err(Error::Malformed("truncated MIDI track".into()));
        }
        let mut tick = 0;
        let mut running = 0;
        while at < end {
            tick += read_var(bytes, &mut at)
                .ok_or_else(|| Error::Malformed("invalid MIDI delta".into()))?;
            let mut status = *bytes
                .get(at)
                .ok_or_else(|| Error::Malformed("truncated MIDI event".into()))?;
            if status & 0x80 != 0 {
                at += 1;
                running = status;
            } else {
                status = running;
            }
            if status == 0xff {
                let _kind = *bytes
                    .get(at)
                    .ok_or_else(|| Error::Malformed("truncated MIDI meta".into()))?;
                at += 1;
                let n = read_var(bytes, &mut at)
                    .ok_or_else(|| Error::Malformed("invalid MIDI meta".into()))?
                    as usize;
                at = at.saturating_add(n);
            } else if status & 0xf0 == 0x90 || status & 0xf0 == 0x80 {
                let pitch = *bytes
                    .get(at)
                    .ok_or_else(|| Error::Malformed("truncated MIDI note".into()))?;
                let velocity = *bytes
                    .get(at + 1)
                    .ok_or_else(|| Error::Malformed("truncated MIDI note".into()))?;
                at += 2;
                if status & 0xf0 == 0x90 && velocity > 0 {
                    notes.push((tick, pitch, id));
                    id += 1;
                    max_tick = max_tick.max(tick);
                }
            } else {
                at = at.saturating_add(if status & 0xe0 == 0xc0 { 1 } else { 2 });
            }
        }
        at = end;
    }
    let scale = 1920u32.max(division.saturating_mul(4));
    let bars = ((max_tick / scale) + 1) as usize;
    let mut bar_values = Vec::new();
    let mut bar_ids = Vec::new();
    for b in 0..bars {
        let note_list = notes
            .iter()
            .filter(|(t, _, _)| *t / scale == b as u32)
            .map(|(_, p, id)| Note {
                id: *id,
                midi: Some(i32::from(*p)),
                string: None,
                fret: None,
                articulation: None,
                techniques: Vec::new(),
            })
            .collect::<Vec<_>>();
        let beats = note_list
            .into_iter()
            .map(|n| Beat {
                id: n.id,
                rhythm: Rhythm::new(crate::model::NoteValue::Quarter),
                notes: vec![n],
                dynamic: None,
            })
            .collect::<Vec<_>>();
        bar_ids.push(b as i32);
        bar_values.push(Bar {
            id: b as u32,
            clef: Some("G2".into()),
            voices: vec![Voice {
                id: b as u32,
                beats,
            }],
        });
    }
    Ok(Document {
        title: "MIDI import".into(),
        artist: String::new(),
        tracks: vec![Track {
            id: 0,
            name: "MIDI".into(),
            color: None,
            tuning: vec![40, 45, 50, 55, 59, 64],
            midi_program: None,
            pan: None,
            volume: None,
            mute: false,
            solo: false,
            percussion_articulations: Vec::new(),
        }],
        master_bars: (0..bars)
            .map(|i| MasterBar {
                index: i,
                time: (4, 4),
                section: None,
                double_bar: false,
                bar_ids: vec![bar_ids[i]],
                repeat_start: false,
                repeat_end: None,
                alternate_ending: 0,
                direction: None,
            })
            .collect(),
        bars: bar_values,
        tempo_map: Vec::new(),
        key: None,
        source: None,
    })
}

impl WriteFormat for Midi {
    const NAME: &'static str = "midi";
    fn write(doc: &Document) -> Result<Vec<u8>> {
        Ok(write(doc))
    }
}
impl ReadFormat for Midi {
    const NAME: &'static str = "midi";
    fn detect(bytes: &[u8]) -> bool {
        bytes.starts_with(b"MThd")
    }
    fn read(bytes: &[u8]) -> Result<Document> {
        read(bytes)
    }
}
