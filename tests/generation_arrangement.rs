#![cfg(feature = "arrangement")]

use tabslib::edits;
use tabslib::fixtures;
use tabslib::generation::arrangement::{
    apply_seeded_variation, check_join, compose_song, fingerprint_bar, score_compatibility,
    segment_motifs, Motif, SectionRequest,
};
use tabslib::inspect::bar_integrity;
use tabslib::model::{NoteValue, Rhythm};

fn motif(doc: &tabslib::Document, start_bar: usize, end_bar: usize) -> Motif {
    let fingerprints = (start_bar..=end_bar)
        .map(|bar| fingerprint_bar(doc, bar).unwrap())
        .collect::<Vec<_>>();
    let first = fingerprints.first().unwrap().clone();
    let combined = fingerprints.iter().skip(1).fold(first, |mut acc, next| {
        for track in 0..acc.rhythms.len() {
            acc.rhythms[track].extend(next.rhythms[track].iter().copied());
            acc.pitch_classes[track].extend(next.pitch_classes[track].iter().copied());
            acc.intervals[track].extend(next.intervals[track].iter().copied());
            acc.note_counts[track] += next.note_counts[track];
        }
        acc
    });
    Motif {
        start_bar,
        end_bar,
        fingerprint: combined,
    }
}

#[test]
fn fingerprints_distinguish_motifs_and_segment_with_a_bounded_size() {
    let doc = fixtures::repeated_frets();
    let first = fingerprint_bar(&doc, 0).unwrap();
    let second = fingerprint_bar(&doc, 1).unwrap();
    assert_eq!(score_compatibility(&first, &first).score, 1.0);
    assert!(score_compatibility(&first, &second).pitch < 1.0);

    let motifs = segment_motifs(&doc, 2, 0.7).unwrap();
    assert_eq!(motifs.len(), 3);
    assert_eq!((motifs[0].start_bar, motifs[0].end_bar), (0, 1));
    assert!(motifs.iter().all(|item| item.end_bar - item.start_bar < 2));
}

#[test]
fn composition_repeats_and_orders_sections_deterministically() {
    let doc = fixtures::repeated_frets();
    let motifs = vec![motif(&doc, 0, 0), motif(&doc, 1, 1)];
    let sections = vec![
        SectionRequest {
            label: "A".into(),
            bars: 3,
            candidates: vec![0],
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        },
        SectionRequest {
            label: "B".into(),
            bars: 2,
            candidates: vec![1],
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        },
    ];
    let first = compose_song(&doc, &motifs, &sections, 12).unwrap();
    let second = compose_song(&doc, &motifs, &sections, 12).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.master_bars.len(), 5);
    assert_eq!(first.master_bars[0].section.as_deref(), Some("A"));
    assert_eq!(first.master_bars[3].section.as_deref(), Some("B"));
    assert!(first.master_bars[1..3]
        .iter()
        .all(|bar| bar.section.is_none()));
    assert_eq!(first.note_count(), doc.note_count());
    assert!(bar_integrity(&first)
        .iter()
        .all(|bar| bar.duration <= bar.capacity));
    assert!(first
        .bars
        .iter()
        .all(|bar| bar.id > doc.bars.last().unwrap().id));
}

#[test]
fn joins_report_overfull_timing_without_repairing_it() {
    let mut doc = fixtures::repeated_frets();
    let valid = check_join(&doc, 0, 1).unwrap();
    assert!(valid.valid);

    let bar_id = doc.master_bars[1].bar_ids[0] as u32;
    doc.bars
        .iter_mut()
        .find(|bar| bar.id == bar_id)
        .unwrap()
        .voices[0]
        .beats[0]
        .rhythm = Rhythm::new(NoteValue::Whole);
    let report = check_join(&doc, 0, 1).unwrap();
    assert!(!report.valid);
    assert!(!report.problems.is_empty());
    let bar = doc.bars.iter().find(|bar| bar.id == bar_id).unwrap();
    assert_eq!(
        bar.voices[0].beats[0].rhythm,
        Rhythm::new(NoteValue::Whole),
        "join checking must not rewrite the overfull beat"
    );
    let selected = vec![motif(&doc, 1, 1)];
    let sections = vec![SectionRequest {
        label: "Invalid".into(),
        bars: 1,
        candidates: vec![0],
        transpose_semitones: 0,
        energy: None,
        instrumentation: None,
        energy_texture: None,
        climax: false,
    }];
    let composed = compose_song(&doc, &selected, &sections, 1)
        .expect("composing a slot fits copied material to destination meter");
    assert_eq!(composed.master_bars.len(), 1);
    assert!(
        bar_integrity(&composed)
            .iter()
            .all(|item| item.duration <= item.capacity),
        "composed bar must fit its meter"
    );
}

#[test]
fn composition_keeps_empty_tracks_and_applies_explicit_transposition() {
    let mut doc = fixtures::repeated_frets();
    let empty = doc.tracks[0].clone();
    edits::create_track(&mut doc, empty);
    let selected = vec![motif(&doc, 0, 0)];
    let sections = vec![SectionRequest {
        label: "Lift".into(),
        bars: 1,
        candidates: vec![0],
        transpose_semitones: 2,
        energy: None,
        instrumentation: None,
        energy_texture: None,
        climax: false,
    }];
    let composed = compose_song(&doc, &selected, &sections, 99).unwrap();
    assert_eq!(composed.tracks.len(), 2);
    assert_eq!(composed.master_bars[0].bar_ids[1], -1);
    let output_id = composed.master_bars[0].bar_ids[0] as u32;
    let output_bar = composed
        .bars
        .iter()
        .find(|bar| bar.id == output_id)
        .unwrap();
    let source_id = doc.master_bars[0].bar_ids[0] as u32;
    let source_bar = doc.bars.iter().find(|bar| bar.id == source_id).unwrap();
    assert_eq!(
        output_bar.voices[0].beats[0].notes[0].midi,
        Some(source_bar.voices[0].beats[0].notes[0].midi.unwrap() + 2)
    );
}

#[test]
fn invalid_empty_or_unplayable_compositions_are_rejected() {
    let doc = fixtures::repeated_frets();
    let selected = vec![motif(&doc, 0, 0)];
    assert!(compose_song(&doc, &selected, &[], 0).is_err());
    let sections = vec![SectionRequest {
        label: "Low".into(),
        bars: 1,
        candidates: vec![0],
        transpose_semitones: -100,
        energy: None,
        instrumentation: None,
        energy_texture: None,
        climax: false,
    }];
    assert!(compose_song(&doc, &selected, &sections, 0).is_err());
}

#[test]
fn seeded_variation_is_repeatable_and_preserves_written_rhythm() {
    let original = fixtures::repeated_frets();
    let mut first = original.clone();
    let mut second = original.clone();
    let first_shift = apply_seeded_variation(&mut first, 0, 1, 27).unwrap();
    let second_shift = apply_seeded_variation(&mut second, 0, 1, 27).unwrap();
    assert_ne!(first_shift, 0);
    assert_eq!(first_shift, second_shift);
    assert_eq!(first, second);
    assert_eq!(first.note_count(), original.note_count());
    let durations = |doc: &tabslib::Document| {
        doc.bars[0].voices[0]
            .beats
            .iter()
            .map(|beat| beat.rhythm.as_fraction())
            .collect::<Vec<_>>()
    };
    assert_eq!(durations(&first), durations(&original));
}

/// Energy and instrumentation are plan decisions applied after assembly, so
/// they must reach the written score rather than staying in the plan file.
#[test]
fn section_energy_and_instrumentation_reach_the_assembled_document() {
    use tabslib::generation::arrangement::{compose_plan, ArrangementPlan};

    let source = tabslib::fixtures::power_chords();
    let motifs = segment_motifs(&source, source.master_bars.len(), 0.0).unwrap();
    let mut plan = ArrangementPlan::new(
        11,
        vec![SectionRequest {
            label: "A".into(),
            bars: source.master_bars.len(),
            candidates: vec![0],
            transpose_semitones: 0,
            energy: Some(90),
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    );

    let loud = compose_plan(&source, &motifs, &plan).unwrap();
    let dynamics: Vec<String> = loud
        .bars
        .iter()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .filter(|beat| !beat.notes.is_empty())
        .filter_map(|beat| beat.dynamic.clone())
        .collect();
    assert!(!dynamics.is_empty(), "energy writes a dynamic");
    assert!(
        dynamics.iter().all(|value| value == "FF"),
        "energy 90 writes FF, got {dynamics:?}"
    );

    plan.sections[0].energy = None;
    plan.sections[0].instrumentation = Some(Vec::new());
    let silent = compose_plan(&source, &motifs, &plan).unwrap();
    assert_eq!(
        silent.note_count(),
        0,
        "a section naming no tracks silences every one of them"
    );

    plan.sections[0].instrumentation = Some(vec![99]);
    assert!(
        compose_plan(&source, &motifs, &plan).is_err(),
        "naming a track that does not exist is an error, not a silent no-op"
    );
}
