//! Structural edit tests over generated fixtures.

use tabslib::edits;
use tabslib::fixtures;
use tabslib::inspect::{self, TrackKind};
use tabslib::model::Track;

fn extra_track(name: &str) -> Track {
    Track {
        id: 99,
        name: name.into(),
        color: None,
        tuning: fixtures::TUNING_E_STANDARD.to_vec(),
        midi_program: Some(30),
        pan: None,
        volume: None,
        mute: false,
        solo: false,
    }
}

#[test]
fn track_operations_keep_names_and_bar_references_consistent() {
    let mut doc = fixtures::repeated_frets();
    let created = edits::create_track(&mut doc, extra_track("gtr"));
    assert_eq!(doc.tracks[created].name, "gtr 2");
    let cloned = edits::clone_track(&mut doc, 0).unwrap();
    assert_eq!(doc.tracks[cloned].name, "gtr 3");
    edits::rename_track(&mut doc, cloned, "gtr").unwrap();
    assert_eq!(doc.tracks[cloned].name, "gtr 3");
    edits::reorder_track(&mut doc, cloned, 0).unwrap();
    assert_eq!(
        inspect::bar_integrity(&doc)
            .iter()
            .filter(|x| x.duration > x.capacity)
            .count(),
        0
    );
    edits::remove_track(&mut doc, created).unwrap();
    assert_eq!(doc.tracks.len(), 2);
}

#[test]
fn slice_splice_and_append_preserve_whole_bar_integrity() {
    let source = fixtures::meters_and_sections();
    let mut sliced = edits::slice(&source, 1, 3).unwrap();
    assert_eq!(sliced.master_bars.len(), 3);
    assert_eq!(sliced.master_bars[0].index, 0);
    edits::append(&mut sliced, &edits::slice(&source, 4, 5).unwrap()).unwrap();
    assert_eq!(sliced.master_bars.len(), 5);
    let replacement = edits::slice(&source, 0, 0).unwrap();
    edits::splice(&mut sliced, 2, 2, &replacement, 0).unwrap();
    assert!(inspect::bar_integrity(&sliced)
        .iter()
        .all(|x| x.duration <= x.capacity));
}

#[test]
fn sections_silence_empty_tracks_and_clamp_are_guarded() {
    let mut doc = fixtures::repeated_frets();
    edits::set_section(&mut doc, 0, Some("Intro".into())).unwrap();
    edits::patch_section(&mut doc, 0, |old| Some(format!("{} / A", old.unwrap()))).unwrap();
    edits::rename_section(&mut doc, "Intro / A", "Verse");
    edits::silence(&mut doc, 0, 0).unwrap();
    assert_eq!(
        inspect::classify_track(&doc, 0),
        Some((TrackKind::Other, 4))
    );
    let mut overfull = fixtures::repeated_frets();
    let beat = overfull.bars[0].voices[0].beats[0].clone();
    overfull.bars[0].voices[0].beats.push(beat);
    edits::clamp_voice(&mut overfull, 0, 0, 0).unwrap();
    assert!(inspect::bar_integrity(&overfull)
        .iter()
        .all(|x| x.duration <= x.capacity));
}
