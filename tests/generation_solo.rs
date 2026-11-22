#![cfg(all(feature = "harmony", feature = "gp"))]

use tabslib::fixtures;
use tabslib::format::{ReadFormat, WriteFormat};
use tabslib::generation::solo::{generate_solo, SoloConfig};
use tabslib::inspect;
use tabslib::model::{Bar, Document};

type GeneratedPitch = (i32, u32, i32);
type GeneratedEvent = ((u64, u64), Option<GeneratedPitch>);

fn bar_for_track(doc: &Document, track_index: usize, bar_index: usize) -> Option<&Bar> {
    let id = *doc.master_bars.get(bar_index)?.bar_ids.get(track_index)?;
    let id = u32::try_from(id).ok()?;
    doc.bars.iter().find(|bar| bar.id == id)
}

fn sequence(doc: &Document, track_index: usize) -> Vec<GeneratedEvent> {
    doc.master_bars
        .iter()
        .enumerate()
        .filter_map(|(bar_index, _)| bar_for_track(doc, track_index, bar_index))
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .map(|beat| {
            let note = beat.notes.first().map(|note| {
                (
                    note.midi.expect("solo pitch"),
                    note.string.expect("solo string"),
                    note.fret.expect("solo fret"),
                )
            });
            (beat.rhythm.as_fraction(), note)
        })
        .collect()
}

#[test]
fn solo_fills_selected_bars_with_scale_notes_and_playable_positions() {
    let mut doc = fixtures::repeated_frets();
    let config = SoloConfig {
        start_bar: 1,
        end_bar: 3,
        ..SoloConfig::c_major(1, 3, 71)
    };
    let generated = generate_solo(&mut doc, &config).unwrap();
    let notes = sequence(&doc, generated.track_index);

    assert_eq!(generated.note_count, 24);
    assert_eq!(notes.len(), 24);
    assert_eq!(doc.master_bars[0].bar_ids[generated.track_index], -1);
    assert_eq!(doc.master_bars[4].bar_ids[generated.track_index], -1);
    let pitches = notes
        .iter()
        .filter_map(|(_, note)| note.map(|(midi, _, _)| midi))
        .collect::<Vec<_>>();
    assert!(pitches
        .windows(2)
        .all(|pair| (pair[1] - pair[0]).abs() <= config.max_leap));
    assert!(
        pitches[..20]
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            > 3
    );
    for (_, note) in notes {
        let (midi, string, fret) = note.expect("the selected rhythmic pattern generates notes");
        assert!((config.register.0..=config.register.1).contains(&midi));
        assert!(config
            .scale
            .contains(&((midi - i32::from(config.key.tonic)).rem_euclid(12) as u8)));
        assert_eq!(config.tuning[string as usize] + fret, midi);
        assert!(fret >= 0 && fret <= config.max_fret);
    }
    assert!(inspect::bar_integrity(&doc).is_empty());
}

#[test]
fn empty_and_sparse_source_bars_do_not_break_solo_generation() {
    let mut doc = fixtures::repeated_frets();
    for bar_index in 1..=3 {
        if let Some(bar) = bar_for_track(&doc, 0, bar_index) {
            let id = bar.id;
            let bar = doc.bars.iter_mut().find(|bar| bar.id == id).unwrap();
            for voice in &mut bar.voices {
                for (beat_index, beat) in voice.beats.iter_mut().enumerate() {
                    if bar_index == 2 || beat_index % 2 == 0 {
                        beat.notes.clear();
                    }
                }
            }
        }
    }
    let config = SoloConfig {
        start_bar: 1,
        end_bar: 3,
        ..SoloConfig::c_major(1, 3, 13)
    };

    let generated = generate_solo(&mut doc, &config).unwrap();
    assert_eq!(generated.note_count, 24);
    assert!(sequence(&doc, generated.track_index)
        .iter()
        .all(|(_, note)| note.is_some()));
    assert!(inspect::bar_integrity(&doc).is_empty());
}

#[test]
fn solo_rejects_an_unplayable_register_without_mutating_the_document() {
    let mut doc = fixtures::repeated_frets();
    let before = doc.clone();
    let config = SoloConfig {
        register: (100, 110),
        tuning: vec![40],
        max_fret: 1,
        ..SoloConfig::c_major(0, 2, 5)
    };

    assert!(generate_solo(&mut doc, &config).is_err());
    assert_eq!(doc, before);
}

#[test]
fn solo_is_seeded_preserves_phrase_boundaries_and_round_trips() {
    let source_bytes =
        <tabslib::format::gp::Gp as WriteFormat>::write(&fixtures::repeated_frets()).unwrap();
    let mut first = <tabslib::format::gp::Gp as ReadFormat>::read(&source_bytes).unwrap();
    let mut second = <tabslib::format::gp::Gp as ReadFormat>::read(&source_bytes).unwrap();
    let config = SoloConfig {
        phrase_bars: 2,
        phrase_tail_notes: 2,
        ..SoloConfig::c_major(0, 4, 1234)
    };
    let generated = generate_solo(&mut first, &config).unwrap();
    let second_result = generate_solo(&mut second, &config).unwrap();

    assert_eq!(generated.phrase_starts, vec![0, 2, 4]);
    assert_eq!(generated, second_result);
    assert_eq!(
        sequence(&first, generated.track_index),
        sequence(&second, second_result.track_index)
    );
    let bytes = <tabslib::format::gp::Gp as WriteFormat>::write(&first).unwrap();
    let reloaded = <tabslib::format::gp::Gp as ReadFormat>::read(&bytes).unwrap();
    assert_eq!(
        sequence(&reloaded, generated.track_index),
        sequence(&first, generated.track_index)
    );
    assert_eq!(reloaded.tracks[generated.track_index].tuning, config.tuning);
    assert_eq!(reloaded.note_count(), first.note_count());
    assert!(inspect::bar_integrity(&reloaded).is_empty());
}

#[test]
fn phrase_tail_variation_uses_the_seed() {
    let mut first = fixtures::repeated_frets();
    let mut second = fixtures::repeated_frets();
    let config = SoloConfig {
        phrase_bars: 2,
        phrase_tail_notes: 2,
        ..SoloConfig::c_major(0, 3, 1)
    };
    let first_result = generate_solo(&mut first, &config).unwrap();
    let second_result = generate_solo(
        &mut second,
        &SoloConfig {
            seed: 2,
            ..config.clone()
        },
    )
    .unwrap();
    let first_sequence = sequence(&first, first_result.track_index);
    let second_sequence = sequence(&second, second_result.track_index);

    let first_tail = first_sequence[14..16]
        .iter()
        .map(|(_, note)| note.map(|(midi, _, _)| midi))
        .collect::<Vec<_>>();
    let second_tail = second_sequence[14..16]
        .iter()
        .map(|(_, note)| note.map(|(midi, _, _)| midi))
        .collect::<Vec<_>>();
    assert_ne!(first_tail, second_tail);
}
