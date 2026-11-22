#![cfg(all(feature = "harmony", feature = "gp"))]

use std::collections::BTreeSet;
use tabslib::fixtures;
use tabslib::format::{ReadFormat, WriteFormat};
use tabslib::generation::harmony::{generate_harmony, HarmonyConfig, HarmonyType};
use tabslib::inspect;
use tabslib::model::KeySignature;

fn generated_notes(doc: &tabslib::Document, track_index: usize) -> Vec<(i32, u32, i32)> {
    doc.master_bars
        .iter()
        .filter_map(|master| master.bar_ids.get(track_index).copied())
        .filter(|id| *id >= 0)
        .filter_map(|id| doc.bars.iter().find(|bar| bar.id == id as u32))
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .map(|note| {
            (
                note.midi.expect("generated pitch"),
                note.string.expect("generated string"),
                note.fret.expect("generated fret"),
            )
        })
        .collect()
}

#[test]
fn harmony_workflow_adds_playable_meter_preserving_track() {
    let mut doc = fixtures::repeated_frets();
    let config = HarmonyConfig {
        source_track: 0,
        key: Some(KeySignature {
            tonic: 0,
            minor: false,
        }),
        harmony: HarmonyType::Diatonic { steps: 2 },
        seed: 41,
        ..HarmonyConfig::diatonic_third(0, 41)
    };
    let result = generate_harmony(&mut doc, &config).unwrap();

    assert!(result.note_count > 0);
    assert_eq!(result.diagnostics.unplayable_notes, 0);
    assert_eq!(doc.tracks[result.track_index].tuning, config.tuning);
    assert_eq!(doc.master_bars.len(), 5);
    assert!(inspect::bar_integrity(&doc).is_empty());
    let notes = generated_notes(&doc, result.track_index);
    assert!(notes
        .windows(2)
        .all(|pair| (pair[1].0 - pair[0].0).abs() <= config.max_leap));
    assert!(notes.iter().all(|(midi, string, fret)| {
        *fret >= 0
            && *fret <= config.max_fret
            && *string < config.tuning.len() as u32
            && config.tuning[*string as usize] + *fret == *midi
    }));
}

#[test]
fn harmony_workflow_omits_notes_that_violate_the_leap_limit() {
    let mut doc = fixtures::repeated_frets();
    let config = HarmonyConfig {
        max_leap: 0,
        ..HarmonyConfig::diatonic_third(0, 5)
    };
    let result = generate_harmony(&mut doc, &config).unwrap();
    let notes = generated_notes(&doc, result.track_index);

    assert!(result.diagnostics.voice_leading_rejected_notes > 0);
    assert!(notes.windows(2).all(|pair| pair[0].0 == pair[1].0));
}

#[test]
fn harmony_generation_is_seeded_and_round_trips_through_gp() {
    let source_bytes =
        <tabslib::format::gp::Gp as WriteFormat>::write(&fixtures::repeated_frets()).unwrap();
    let mut first = <tabslib::format::gp::Gp as ReadFormat>::read(&source_bytes).unwrap();
    let mut second = <tabslib::format::gp::Gp as ReadFormat>::read(&source_bytes).unwrap();
    let config = HarmonyConfig::diatonic_third(0, 17);
    let generated = generate_harmony(&mut first, &config).unwrap();
    generate_harmony(&mut second, &config).unwrap();
    assert_eq!(first, second);

    let bytes = <tabslib::format::gp::Gp as WriteFormat>::write(&first).unwrap();
    let reloaded = <tabslib::format::gp::Gp as ReadFormat>::read(&bytes).unwrap();
    assert_eq!(reloaded.tracks[generated.track_index].tuning, config.tuning);
    assert_eq!(
        generated_notes(&reloaded, generated.track_index),
        generated_notes(&first, generated.track_index)
    );
    assert_eq!(reloaded.note_count(), first.note_count());
    assert!(inspect::bar_integrity(&reloaded).is_empty());
}

#[test]
fn parallel_harmony_generates_the_requested_pitch_classes() {
    let mut doc = fixtures::repeated_frets();
    let source_classes = doc
        .bars
        .iter()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .filter_map(|note| note.midi)
        .map(|midi| (midi + 7).rem_euclid(12))
        .collect::<BTreeSet<_>>();
    let config = HarmonyConfig {
        harmony: HarmonyType::Parallel { semitones: 7 },
        ..HarmonyConfig::diatonic_third(0, 29)
    };
    let generated = generate_harmony(&mut doc, &config).unwrap();
    let notes = generated_notes(&doc, generated.track_index);

    assert!(generated.note_count > 0);
    assert!(notes
        .iter()
        .all(|(midi, _, _)| source_classes.contains(&midi.rem_euclid(12))));
}

#[test]
fn harmony_workflow_reports_unplayable_source_notes() {
    let mut doc = fixtures::repeated_frets();
    let config = HarmonyConfig {
        harmony: HarmonyType::Parallel { semitones: 7 },
        tuning: vec![127],
        max_fret: 0,
        ..HarmonyConfig::diatonic_third(0, 3)
    };
    let result = generate_harmony(&mut doc, &config).unwrap();
    assert_eq!(result.note_count, 0);
    assert!(result.diagnostics.unplayable_notes > 0);
}

#[test]
fn harmony_generation_clamps_sub_sixteenth_material_without_adding_violations() {
    let mut doc = fixtures::repeated_frets();
    let bar_id = doc.master_bars[0].bar_ids[0];
    let bar = doc.bars.iter_mut().find(|b| b.id == bar_id as u32).unwrap();
    let note = tabslib::model::Note {
        id: 0,
        midi: Some(50),
        string: Some(2),
        fret: Some(0),
        articulation: None,
        techniques: Vec::new(),
    };
    bar.voices[0].beats = vec![
        tabslib::model::Beat {
            id: 0,
            rhythm: tabslib::model::Rhythm::new(tabslib::model::NoteValue::Quarter),
            notes: vec![note.clone()],
            dynamic: None,
        },
        tabslib::model::Beat {
            id: 0,
            rhythm: tabslib::model::Rhythm::new(tabslib::model::NoteValue::Eighth),
            notes: vec![note.clone()],
            dynamic: None,
        },
        tabslib::model::Beat {
            id: 0,
            rhythm: tabslib::model::Rhythm::new(tabslib::model::NoteValue::ThirtySecond),
            notes: vec![note.clone()],
            dynamic: None,
        },
        tabslib::model::Beat {
            id: 0,
            rhythm: tabslib::model::Rhythm::new(tabslib::model::NoteValue::ThirtySecond),
            notes: vec![note.clone()],
            dynamic: None,
        },
        tabslib::model::Beat {
            id: 0,
            rhythm: tabslib::model::Rhythm::new(tabslib::model::NoteValue::Sixteenth),
            notes: vec![note.clone()],
            dynamic: None,
        },
        tabslib::model::Beat {
            id: 0,
            rhythm: tabslib::model::Rhythm::new(tabslib::model::NoteValue::Half),
            notes: vec![note],
            dynamic: None,
        },
    ];

    let config = HarmonyConfig::diatonic_third(0, 7);
    let result = generate_harmony(&mut doc, &config).unwrap();
    assert!(result.note_count > 0);
    assert!(inspect::bar_integrity(&doc).is_empty());

    let gen_bar_id = doc.master_bars[0].bar_ids[result.track_index];
    let gen_bar = doc.bars.iter().find(|b| b.id == gen_bar_id as u32).unwrap();
    let sixteenth = inspect::Fraction::new(1, 16);
    for beat in &gen_bar.voices[0].beats {
        assert!(
            inspect::rhythm_duration(beat.rhythm) >= sixteenth,
            "generated beats must not be shorter than a sixteenth: {:?}",
            beat.rhythm
        );
    }
}
