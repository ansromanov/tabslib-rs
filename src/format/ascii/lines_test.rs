use super::*;

use crate::fixtures;

#[test]
fn string_lines_run_highest_pitch_first_with_note_names() {
    let doc = fixtures::repeated_frets();
    let lines = track_lines(&doc, 0, &doc.tracks[0]);
    let labels: Vec<&str> = lines.iter().map(|(_, label)| label.as_str()).collect();
    assert_eq!(labels, ["e", "B", "G", "D", "A", "E"]);
    let pitches: Vec<i32> = lines
        .iter()
        .filter_map(|(line, _)| match line {
            Line::String { open_pitch, .. } => Some(*open_pitch),
            Line::Drum { .. } => None,
        })
        .collect();
    assert_eq!(pitches, [64, 59, 55, 50, 45, 40]);
}

#[test]
fn drum_lanes_are_one_per_instrument_stacked_like_a_kit() {
    // The fixture sounds closed hat, ride, crash, snare, two toms and kick.
    let doc = fixtures::drum_patterns();
    let labels: Vec<String> = track_lines(&doc, 0, &doc.tracks[0])
        .into_iter()
        .map(|(_, label)| label)
        .collect();
    assert_eq!(labels.first().map(String::as_str), Some("Crash"));
    assert_eq!(labels.last().map(String::as_str), Some("Kick"));
    for expected in ["Hi-Hat", "Ride", "Snare", "Kick"] {
        assert!(
            labels.iter().any(|label| label == expected),
            "missing lane {expected} in {labels:?}"
        );
    }
    let snare = labels.iter().position(|label| label == "Snare");
    let hat = labels.iter().position(|label| label == "Hi-Hat");
    assert!(hat < snare, "cymbals sit above the snare: {labels:?}");
    let mut unique = labels.clone();
    unique.dedup();
    assert_eq!(unique.len(), labels.len());
}

#[test]
fn a_mapped_articulation_resolves_through_the_track_table() {
    let mut track = fixtures::drum_patterns().tracks[0].clone();
    track.percussion_articulations = vec![crate::percussion::PercussionArticulation {
        raw_id: 3,
        midi: 38,
        role: DrumRole::Snare,
    }];
    let note = Note {
        id: 1,
        midi: None,
        string: None,
        fret: None,
        articulation: Some(3),
        techniques: vec![],
    };
    // Raw id 3 is not a percussion key itself; only the table names it.
    assert_eq!(drum_key(&track, &note), Some(38));
}

#[test]
fn colliding_names_get_numbered() {
    let mut doc = fixtures::drum_patterns();
    // Add a second crash (57) next to the fixture's crash (49).
    let bar = doc.bars.last_mut().expect("fixture has bars");
    let mut extra = bar.voices[0].beats[0].notes[0].clone();
    extra.midi = Some(57);
    extra.articulation = Some(57);
    bar.voices[0].beats[0].notes.push(extra);
    let labels: Vec<String> = track_lines(&doc, 0, &doc.tracks[0])
        .into_iter()
        .map(|(_, label)| label)
        .collect();
    assert!(labels.contains(&"Crash".to_string()));
    assert!(labels.contains(&"Crash 2".to_string()), "{labels:?}");
}

#[test]
fn every_kit_lane_is_named_within_the_label_width() {
    // A two-character code says nothing to a reader; a name that overruns the
    // width pushes the staff off a terminal line. Both matter, so assert both
    // for the whole standard kit.
    for key in 35..=81u8 {
        let label = drum_name(key);
        assert!(
            label.chars().count() <= MAX_DRUM_LABEL,
            "{key} is labelled {label:?}, wider than {MAX_DRUM_LABEL}"
        );
        assert!(
            label.chars().next().is_some_and(char::is_alphabetic),
            "{key} is labelled {label:?}, which is not a name"
        );
    }
    assert_eq!(drum_name(36), "Kick");
    assert_eq!(drum_name(42), "Hi-Hat");
    assert_eq!(drum_name(54), "Tambourine");
    // Outside the standard kit nothing is known but the number.
    assert_eq!(drum_name(90), "90");
}

#[test]
fn a_numbered_label_stays_within_the_label_width() {
    assert_eq!(numbered_label("Tambourine", 2), "Tambouri 2");
    assert_eq!(numbered_label("Crash", 3), "Crash 3");
}
