//! Export one pitched track of a score as JSON notes in MIDI ticks (480 per quarter note) plus the tempo map,
//! in playback order (repeats expanded), for doubling a guitar part with a synth in a DAW.
//!
//! ```text
//! cargo run --release --example export_track -- song.gp            # list the tracks
//! cargo run --release --example export_track -- song.gp out.json 1
//! ```
//!
//! Notes keep their written length; a tied note extends the note it continues. Dead notes are left out,
//! palm-muted ones are flagged. Output:
//! `{title, track, tuning, ppq, total_ticks, tempos: [{tick, bpm}], bars: [{tick, num, den, section}],
//!   notes: [{tick, length, pitch, velocity, palm_mute, string, fret}]}`.

use serde_json::json;
use tabslib::model::{sounding_pitch, Technique};

const TICKS_PER_WHOLE: u64 = 1920;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let usage = "usage: export_track <score.gp> [<out.json> <track>]";
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or(usage)?;
    let document = tabslib::load(&std::fs::read(&input)?)?;
    let (Some(output), Some(track_index)) = (args.next(), args.next()) else {
        for (index, track) in document.tracks.iter().enumerate() {
            println!(
                "{index}: {:?} tuning {:?} program {:?}",
                track.name, track.tuning, track.midi_program
            );
        }
        return Ok(());
    };
    let track_index: usize = track_index.parse()?;
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
    let mut notes: Vec<serde_json::Value> = Vec::new();
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
                        if note.techniques.contains(&Technique::Dead) {
                            continue;
                        }
                        // Pitch from the track tuning and the fret first: the per-note `midi` of some files (the Rained
                        // Steel tabs) is 1 or 3 semitones off the tuning the player means (C#), `midi` only fills in
                        // notes without a string and fret.
                        let pitch = match (note.string, note.fret) {
                            (Some(string), Some(fret)) => {
                                sounding_pitch(&track.tuning, string, fret)
                            }
                            _ => None,
                        }
                        .or(note.midi);
                        let Some(pitch) = pitch.filter(|pitch| (0..=127).contains(pitch)) else {
                            continue;
                        };
                        if note.techniques.contains(&Technique::TieDestination) {
                            if let Some(previous) =
                                notes.iter_mut().rev().find(|n| n["pitch"] == pitch)
                            {
                                let start = previous["tick"].as_u64().unwrap_or(0);
                                previous["length"] = json!(cursor + length - start);
                                continue;
                            }
                        }
                        notes.push(json!({
                            "tick": cursor, "length": length, "pitch": pitch, "velocity": 100,
                            "palm_mute": note.techniques.contains(&Technique::PalmMute),
                            "string": note.string, "fret": note.fret,
                        }));
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
        "title": document.title, "track": track.name, "track_index": track_index, "tuning": track.tuning,
        "ppq": 480, "total_ticks": cursor,
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
