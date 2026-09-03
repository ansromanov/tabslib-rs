//! Pitch and tuning transformation tests over generic fixtures.

use tabslib::fixtures;
use tabslib::pitch;

fn pitches(doc: &tabslib::Document) -> Vec<(Option<i32>, Option<i32>, Option<u32>)> {
    doc.bars
        .iter()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .map(|note| (note.midi, note.fret, note.string))
        .collect()
}

#[test]
fn chromatic_transpose_and_inverse_restore_every_fixture() {
    for (_, original) in fixtures::all() {
        let before = pitches(&original);
        let mut changed = original.clone();
        pitch::transpose(&mut changed, 5).unwrap();
        pitch::transpose(&mut changed, -5).unwrap();
        assert_eq!(pitches(&changed), before);
    }
}

#[test]
fn retune_modes_are_distinct_and_pitch_mode_is_reversible() {
    let original = fixtures::repeated_frets();
    let before = pitches(&original);
    let mut pitch_kept = original.clone();
    pitch::retune_preserve_pitch(&mut pitch_kept, 0, &fixtures::TUNING_DROP_D).unwrap();
    assert_eq!(
        pitches(&pitch_kept).iter().map(|x| x.0).collect::<Vec<_>>(),
        before.iter().map(|x| x.0).collect::<Vec<_>>()
    );
    pitch::retune_preserve_pitch(&mut pitch_kept, 0, &fixtures::TUNING_E_STANDARD).unwrap();
    assert_eq!(pitches(&pitch_kept), before);
    let mut fingering_kept = original;
    pitch::retune_preserve_fingering(&mut fingering_kept, 0, &fixtures::TUNING_DROP_D).unwrap();
    assert_ne!(
        pitches(&fingering_kept)
            .iter()
            .map(|x| x.0)
            .collect::<Vec<_>>(),
        before.iter().map(|x| x.0).collect::<Vec<_>>()
    );
    assert_eq!(
        pitches(&fingering_kept)
            .iter()
            .map(|x| (x.1, x.2))
            .collect::<Vec<_>>(),
        before.iter().map(|x| (x.1, x.2)).collect::<Vec<_>>()
    );
}

#[test]
fn key_mapping_diatonic_interval_and_capo_are_deterministic() {
    let c = tabslib::model::KeySignature {
        tonic: 0,
        minor: false,
    };
    let g = tabslib::model::KeySignature {
        tonic: 7,
        minor: false,
    };
    assert_eq!(pitch::map_pitch_class(0, c, g), 7);
    assert_eq!(pitch::diatonic_interval(c, 7), 12);
    let mut doc = fixtures::repeated_frets();
    let before = pitches(&doc);
    pitch::apply_capo(&mut doc, 0, 0, 4, 2).unwrap();
    assert_eq!(
        pitches(&doc).iter().map(|x| x.1).collect::<Vec<_>>(),
        before.iter().map(|x| x.1).collect::<Vec<_>>()
    );
    assert_eq!(
        pitches(&doc)
            .iter()
            .map(|x| x.0.unwrap() - 2)
            .collect::<Vec<_>>(),
        before.iter().map(|x| x.0.unwrap()).collect::<Vec<_>>()
    );
}
