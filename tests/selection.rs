//! Selection-edit tests over generated fixtures.

use tabslib::fixtures;
use tabslib::model::{NoteValue, Rhythm, Technique};
use tabslib::selection::{self, Selection};

fn all_notes(doc: &tabslib::Document) -> Vec<(Option<u32>, Option<i32>, Vec<Technique>)> {
    doc.bars
        .iter()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .map(|note| (note.string, note.fret, note.techniques.clone()))
        .collect()
}

#[test]
fn technique_selection_is_idempotent_and_filterable() {
    let mut doc = fixtures::repeated_frets();
    let selection = Selection {
        track: Some(0),
        bars: Some((0, 0)),
        string: Some(6),
        pitch_class: None,
    };
    selection::set_technique(&mut doc, selection, Technique::Accent).unwrap();
    selection::set_technique(&mut doc, selection, Technique::Accent).unwrap();
    assert_eq!(
        doc.bars[0].voices[0]
            .beats
            .iter()
            .filter(|beat| beat.notes[0].techniques.contains(&Technique::Accent))
            .count(),
        8
    );
    selection::clear_technique(&mut doc, selection, &Technique::Accent).unwrap();
    assert!(!doc.bars[0].voices[0].beats[0].notes[0]
        .techniques
        .contains(&Technique::Accent));
}

#[test]
fn refinger_preserves_pitch_and_inverse_restores_positions() {
    let mut doc = fixtures::repeated_frets();
    let before = all_notes(&doc);
    let selection = Selection {
        track: Some(0),
        bars: Some((0, 3)),
        ..Selection::default()
    };
    selection::refinger(&mut doc, selection, 5).unwrap();
    assert_eq!(doc.bars[0].voices[0].beats[0].notes[0].midi, Some(45));
    selection::refinger(&mut doc, selection, 6).unwrap();
    assert_eq!(all_notes(&doc), before);
}

#[test]
fn dynamics_and_voice_split_merge_are_reversible() {
    let mut doc = fixtures::repeated_frets();
    let selection = Selection {
        track: Some(0),
        bars: Some((0, 0)),
        ..Selection::default()
    };
    selection::set_dynamic(&mut doc, selection, Some("F".into())).unwrap();
    assert_eq!(doc.bars[0].voices[0].beats[0].dynamic.as_deref(), Some("F"));
    selection::split_voice(&mut doc, 0, 0, 0, 4).unwrap();
    assert_eq!(doc.bars[0].voices.len(), 2);
    selection::merge_voices(&mut doc, 0, 0, 0, 1).unwrap();
    assert_eq!(doc.bars[0].voices.len(), 1);
    assert_eq!(doc.bars[0].voices[0].beats.len(), 8);
    assert_eq!(
        doc.bars[0].voices[0].beats[0].rhythm,
        Rhythm::new(NoteValue::Eighth)
    );
}
