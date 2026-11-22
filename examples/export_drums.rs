//! Export the drum track of a score as JSON: notes in MIDI ticks (480 per quarter note) plus the tempo map,
//! in playback order (repeats expanded), for importing the drums into a DAW.
//!
//! ```text
//! cargo run --release --example export_drums -- song.gp drums.json [track-index]
//! ```
//!
//! Without a track index the first percussion track is used: one with percussion articulations or MIDI
//! channel 9, else one whose name mentions drums. Output:
//! `{title, track, ppq, total_ticks, tempos: [{tick, bpm}], bars: [{tick, num, den, section}],
//!   notes: [{tick, length, pitch, velocity}]}`.

use serde_json::json;
use tabslib::model::Document;

const TICKS_PER_WHOLE: u64 = 1920;

fn is_percussion(document: &Document, index: usize) -> bool {
    let track = &document.tracks[index];
    !track.percussion_articulations.is_empty()
        || track.midi_channel == Some(9)
        || track.name.to_lowercase().contains("drum")
        || track.name.to_lowercase().contains("бараб")
}

/// MIDI velocity for a Guitar Pro dynamic mark (`PPP` .. `FFF`); 100 when the beat carries none.
fn velocity(dynamic: &Option<String>) -> u8 {
    match dynamic.as_deref().map(str::to_lowercase).as_deref() {
        Some("ppp") => 30,
        Some("pp") => 45,
        Some("p") => 60,
        Some("mp") => 75,
        Some("mf") => 90,
        Some("f") => 105,
        Some("ff") => 118,
        Some("fff") => 127,
        _ => 100,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or("usage: export_drums <score.gp> <out.json> [track]")?;
    let output = args
        .next()
        .ok_or("usage: export_drums <score.gp> <out.json> [track]")?;
    let document = tabslib::load(&std::fs::read(&input)?)?;
    let track_index = match args.next() {
        Some(text) => text.parse::<usize>()?,
        None => (0..document.tracks.len())
            .find(|&i| is_percussion(&document, i))
            .ok_or("no percussion track found; pass a track index")?,
    };
    let track = &document.tracks[track_index];

    let tempo_at = |master: usize| -> f64 {
        document
            .tempo_map
            .iter()
            .filter(|change| change.at_bar <= master)
            .max_by_key(|change| change.at_bar)
            .map(|change| change.bpm)
            .or_else(|| document.tempo_map.first().map(|change| change.bpm))
            .unwrap_or(120.0)
    };

    let mut cursor: u64 = 0;
    let mut notes = Vec::new();
    let mut bars = Vec::new();
    let mut tempos: Vec<(u64, f64)> = Vec::new();
    for master_index in tabslib::inspect::playback_order(&document) {
        let master = &document.master_bars[master_index];
        let bpm = tempo_at(master_index);
        if tempos
            .last()
            .map(|(_, last)| (*last - bpm).abs() > 1e-9)
            .unwrap_or(true)
        {
            tempos.push((cursor, bpm));
        }
        bars.push(json!({
            "tick": cursor, "num": master.time.0, "den": master.time.1,
            "section": master.section, "master_bar": master_index,
        }));
        let bar_start = cursor;
        let bar_id = *master.bar_ids.get(track_index).unwrap_or(&-1);
        if let Some(bar) = document
            .bars
            .iter()
            .find(|bar| bar.id as i64 == bar_id as i64)
        {
            if let Some(voice) = bar.voices.first() {
                for beat in &voice.beats {
                    let (n, d) = beat.rhythm.as_fraction();
                    let length = n * TICKS_PER_WHOLE / d;
                    for note in &beat.notes {
                        let pitch = note
                            .articulation
                            .and_then(|raw| {
                                track
                                    .percussion_articulations
                                    .iter()
                                    .find(|mapping| mapping.raw_id == raw)
                                    .map(|mapping| i32::from(mapping.midi))
                                    .or_else(|| tabslib::percussion::midi_note(raw).map(i32::from))
                            })
                            .or(note.midi)
                            .filter(|pitch| (0..=127).contains(pitch));
                        if let Some(pitch) = pitch {
                            notes.push(json!({
                                "tick": cursor, "length": length.min(240), "pitch": pitch,
                                "velocity": velocity(&beat.dynamic),
                            }));
                        }
                    }
                    cursor += length;
                }
            }
        }
        // a bar whose voice is shorter than the meter still takes the whole bar
        let bar_length = master.time.0 as u64 * TICKS_PER_WHOLE / master.time.1 as u64;
        cursor = cursor.max(bar_start + bar_length);
    }
    let document_json = json!({
        "title": document.title, "track": track.name, "track_index": track_index, "ppq": 480,
        "total_ticks": cursor,
        "tempos": tempos.iter().map(|(tick, bpm)| json!({"tick": tick, "bpm": bpm})).collect::<Vec<_>>(),
        "bars": bars, "notes": notes,
    });
    std::fs::write(&output, serde_json::to_string_pretty(&document_json)?)?;
    eprintln!(
        "{} - track {} {:?}: {} notes, {} bars, {} tempo changes, {} ticks",
        document.title,
        track_index,
        track.name,
        notes.len(),
        bars.len(),
        tempos.len(),
        cursor
    );
    Ok(())
}
