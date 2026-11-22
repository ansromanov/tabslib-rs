//! Read, inspect, edit, render and write a score.
//!
//! ```text
//! cargo run --example quickstart              # uses a generated score
//! cargo run --example quickstart -- song.gp   # or a file of your own
//! ```

use tabslib::format::{ascii, gp::Gp};
use tabslib::{facade, pitch, ReadFormat, WriteFormat};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load a file if one is given, otherwise build a score in code.
    let mut doc = match std::env::args().nth(1) {
        Some(path) => tabslib::load(&std::fs::read(path)?)?,
        None => tabslib::fixtures::power_chords(),
    };

    // Inspect.
    let stats = facade::inspect::statistics(&doc);
    println!(
        "{} tracks, {} bars, {} notes",
        doc.tracks.len(),
        doc.master_bars.len(),
        stats.notes
    );

    // Edit: up a whole tone, keeping every note playable.
    pitch::transpose(&mut doc, 2)?;

    // Render the first four bars of the first track as ASCII tablature.
    let last = doc.master_bars.len().min(4).saturating_sub(1);
    println!("{}", ascii::render_track(&doc, 0, 0, last)?);

    // Write GP bytes and read them back; the round trip is lossless.
    let bytes = Gp::write(&doc)?;
    let reread = Gp::read(&bytes)?;
    assert_eq!(reread.note_count(), doc.note_count());
    Ok(())
}
