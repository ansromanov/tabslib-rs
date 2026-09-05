//! Inspection tests over generic in-memory fixtures.

use tabslib::fixtures;
use tabslib::inspect::{self, BarFeel, Fraction, TrackKind};
use tabslib::model::Technique;

#[test]
fn summary_reports_fixture_shape() {
    let summary = inspect::summary(&fixtures::meters_and_sections());
    assert_eq!(summary.bars, 6);
    assert_eq!(
        summary.meters,
        vec![(4, 4), (3, 4), (6, 8), (7, 8), (5, 4), (12, 8)]
    );
    assert_eq!(summary.duration, Fraction::new(49, 8));
    assert_eq!(summary.tracks, vec![("gtr".into(), TrackKind::Other)]);
}

#[test]
fn classification_and_census_expose_structure() {
    assert_eq!(
        inspect::classify_track(&fixtures::drum_patterns(), 0),
        Some((TrackKind::Percussion, 5))
    );
    assert_eq!(
        inspect::classify_track(&fixtures::bass_line(), 0),
        Some((TrackKind::Bass, 4))
    );
    let census = inspect::note_census(&fixtures::every_technique());
    assert_eq!(census.techniques["PalmMute"], 8);
    assert!(inspect::note_census(&fixtures::drum_patterns())
        .articulations
        .contains_key(&fixtures::drum::KICK));
}

#[test]
fn integrity_and_positions_are_exact() {
    let mut doc = fixtures::repeated_frets();
    doc.bars[0].voices[0].beats.pop();
    let extra = doc.bars[1].voices[0].beats[0].clone();
    doc.bars[1].voices[0].beats.push(extra);
    assert_eq!(inspect::bar_integrity(&doc).len(), 2);
    let positions = inspect::beat_positions(&fixtures::repeated_frets(), 0, 0, 0);
    assert_eq!(positions[1].onset, Fraction::new(1, 8));
    assert_eq!(
        inspect::detect_bar_feel(&fixtures::repeated_frets(), 0, 0, 0),
        BarFeel::Straight
    );
}

#[test]
fn key_and_tuning_parsers_are_normalized() {
    assert_eq!(inspect::parse_key("C# minor").unwrap().tonic, 1);
    assert!(inspect::parse_key("H major").is_none());
    assert_eq!(
        inspect::parse_tuning("E2 A2 D3 G3 B3 E4").unwrap(),
        fixtures::TUNING_E_STANDARD
    );
    assert_eq!(
        inspect::parse_tuning("Drop D").unwrap(),
        fixtures::TUNING_DROP_D
    );
}

#[test]
fn tied_pitch_inspection_catches_changed_endpoints() {
    let mut doc = fixtures::repeated_frets();
    doc.bars[0].voices[0].beats[0].notes[0]
        .techniques
        .push(Technique::TieOrigin);
    doc.bars[0].voices[0].beats[1].notes[0]
        .techniques
        .push(Technique::TieDestination);
    assert!(inspect::tied_pitch_mismatches(&doc).is_empty());
    doc.bars[0].voices[0].beats[1].notes[0].midi = Some(99);
    assert!(!inspect::tied_pitch_mismatches(&doc).is_empty());
}

#[test]
fn statistics_report_exact_score_shape() {
    let statistics = inspect::statistics(&fixtures::repeated_frets());
    assert_eq!(statistics.tracks, 1);
    assert_eq!(statistics.bars, 5);
    assert_eq!(statistics.voices, 5);
    assert_eq!(statistics.beats, 40);
    assert_eq!(statistics.notes, 40);
    assert_eq!(statistics.pitched_notes, 40);
    assert_eq!(statistics.rests, 0);
    assert_eq!(statistics.notes_per_bar, vec![8; 5]);
    assert_eq!(statistics.durations[&Fraction::new(1, 8)], 40);
    assert_eq!(statistics.pitch_range, Some((40, 52)));
    assert_eq!(statistics.pitch_classes.values().sum::<usize>(), 40);
    assert_eq!(statistics.pitch_intervals.values().sum::<usize>(), 39);
}

#[test]
fn key_estimation_uses_chord_root_evidence() {
    let mut doc = fixtures::repeated_frets();
    for bar in &mut doc.bars {
        for beat in &mut bar.voices[0].beats {
            beat.notes[0].midi = Some(60);
            for (offset, midi) in [64, 67].into_iter().enumerate() {
                let mut note = beat.notes[0].clone();
                note.id += offset as u32 + 1;
                note.midi = Some(midi);
                beat.notes.push(note);
            }
        }
    }
    let estimate = inspect::determine_key(&doc).unwrap();
    assert_eq!(estimate.key.tonic, 0);
    assert!(!estimate.key.minor);
    assert_eq!(estimate.root_support, 40);
    assert_eq!(estimate.pitched_notes, 120);
    assert!(estimate.confidence > Fraction::new(1, 24));
}
