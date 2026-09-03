//! Regression tests for inspection and structural-edit bugs.

use tabslib::edits;
use tabslib::fixtures;
use tabslib::inspect::{self, BarFeel, Fraction, TrackKind};
use tabslib::model::{Beat, Rhythm};

#[test]
fn zero_denominators_and_empty_tunings_are_safe() {
    assert_eq!(inspect::bar_capacity((4, 0)), Fraction::new(0, 1));
    assert!(inspect::parse_tuning(" ").is_none());
}

#[test]
fn piano_is_not_classified_as_percussion() {
    let mut doc = fixtures::repeated_frets();
    doc.tracks[0].name = "piano".into();
    doc.tracks[0].midi_program = Some(0);
    assert_eq!(
        inspect::classify_track(&doc, 0),
        Some((TrackKind::Other, 5))
    );
}

#[test]
fn track_classification_uses_tuning_and_sound() {
    let mut doc = fixtures::repeated_frets();
    doc.tracks[0].name = "bass".into();
    doc.tracks[0].tuning.truncate(4);
    doc.tracks[0].midi_program = Some(33);
    assert_eq!(
        inspect::classify_track(&doc, 0).map(|x| x.0),
        Some(TrackKind::Bass)
    );

    let mut empty = fixtures::repeated_frets();
    empty.tracks[0].name = "unused".into();
    empty.tracks[0].midi_program = Some(0);
    for bar in &mut empty.bars {
        for voice in &mut bar.voices {
            for beat in &mut voice.beats {
                beat.notes.clear();
            }
        }
    }
    assert_eq!(
        inspect::classify_track(&empty, 0).map(|x| x.0),
        Some(TrackKind::Empty)
    );

    let drums = fixtures::drum_patterns();
    assert_eq!(
        inspect::classify_track(&drums, 0).map(|x| x.0),
        Some(TrackKind::Percussion)
    );
}

#[test]
fn census_collapses_payload_bearing_techniques() {
    let census = inspect::note_census(&fixtures::every_technique());
    assert_eq!(census.techniques.get("Bend"), Some(&4));
    assert_eq!(census.techniques.get("Harmonic"), Some(&8));
}

#[test]
fn remove_and_splice_clean_references_and_indices() {
    let mut doc = fixtures::repeated_frets();
    let original_bars = doc.bars.len();
    let clone = edits::clone_track(&mut doc, 0).unwrap();
    assert!(doc.bars.len() > original_bars);
    edits::remove_track(&mut doc, clone).unwrap();
    assert_eq!(doc.bars.len(), original_bars);

    let source = fixtures::meters_and_sections();
    let mut destination = source.clone();
    edits::splice(&mut destination, 1, 1, &source, 0).unwrap();
    assert!(destination
        .master_bars
        .iter()
        .enumerate()
        .all(|(i, bar)| bar.index == i));
}

#[test]
fn swung_and_unsupported_feel_are_distinguishable() {
    let mut doc = fixtures::repeated_frets();
    doc.master_bars[0].time = (4, 4);
    let note = doc.bars[0].voices[0].beats[0].notes[0].clone();
    let bar = &mut doc.bars[0];
    bar.voices[0].beats = (0..8)
        .map(|i| Beat {
            id: 0,
            rhythm: if i % 2 == 0 {
                Rhythm {
                    value: tabslib::model::NoteValue::Eighth,
                    dots: 1,
                    tuplet: None,
                }
            } else {
                Rhythm::new(tabslib::model::NoteValue::Sixteenth)
            },
            notes: vec![note.clone()],
            dynamic: None,
        })
        .collect();
    assert_eq!(inspect::detect_bar_feel(&doc, 0, 0, 0), BarFeel::Swung);
    doc.master_bars[0].time = (7, 8);
    doc.bars[0].voices[0].beats = (0..7)
        .map(|i| Beat {
            id: i,
            rhythm: Rhythm {
                value: tabslib::model::NoteValue::Eighth,
                dots: 0,
                tuplet: Some((3, 2)),
            },
            notes: vec![note.clone()],
            dynamic: None,
        })
        .collect();
    assert_eq!(inspect::detect_bar_feel(&doc, 0, 0, 0), BarFeel::Triplet);
    doc.master_bars[0].time = (4, 3);
    assert_eq!(
        inspect::detect_bar_feel(&doc, 0, 0, 0),
        BarFeel::Unsupported
    );
}

#[test]
fn new_bar_uses_a_document_unique_voice_id() {
    let mut doc = fixtures::repeated_frets();
    let bar = edits::new_bar(&doc);
    doc.bars.push(bar.clone());
    let second = edits::new_bar(&doc);
    assert!(bar.voices[0].id > 0);
    assert_ne!(bar.voices[0].id, second.voices[0].id);
    assert_eq!(inspect::bar_capacity((7, 8)), Fraction::new(7, 8));
}
