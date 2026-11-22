#![cfg(all(feature = "midi", feature = "musicxml", feature = "wav"))]

use tabslib::fixtures;
use tabslib::format::midi::Midi;
use tabslib::format::musicxml::MusicXml;
use tabslib::format::wav;
use tabslib::format::{ReadFormat, WriteFormat};

#[test]
fn midi_export_import_preserves_pitched_note_count() {
    let original = fixtures::repeated_frets();
    let bytes = <Midi as WriteFormat>::write(&original).unwrap();
    assert!(Midi::detect(&bytes));
    let imported = <Midi as ReadFormat>::read(&bytes).unwrap();
    assert_eq!(imported.note_count(), original.note_count());
}

#[test]
fn musicxml_export_is_deterministic_and_contains_score_structure() {
    let doc = fixtures::repeated_frets();
    let first = <MusicXml as WriteFormat>::write(&doc).unwrap();
    let second = <MusicXml as WriteFormat>::write(&doc).unwrap();
    assert_eq!(first, second);
    let xml = String::from_utf8(first).unwrap();
    assert!(xml.contains("<score-partwise"));
    assert!(xml.contains("<part-name>gtr</part-name>"));
    assert!(xml.contains("<pitch>"));
}

#[test]
fn performance_compiler_emits_score_notes_and_duration() {
    let performance = wav::compile_performance_events(&fixtures::repeated_frets()).unwrap();

    assert!(performance.sample_count > 0);
    assert!(performance
        .events
        .iter()
        .any(|event| matches!(event.message, wav::PerformanceMessage::NoteOn { .. })));
    assert!(performance
        .events
        .iter()
        .any(|event| matches!(event.message, wav::PerformanceMessage::NoteOff { .. })));
}
