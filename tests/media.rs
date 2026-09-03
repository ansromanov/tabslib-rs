use std::fs;

use tabslib::audio;
use tabslib::fixtures;
use tabslib::format::midi::Midi;
use tabslib::format::musicxml::MusicXml;
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
fn pcm_render_requires_external_soundfont_and_writes_wav() {
    let path = std::env::temp_dir().join(format!("tabslib-test-{}.sf2", std::process::id()));
    fs::write(&path, b"test soundfont placeholder").unwrap();
    let wav = audio::render_pcm(&fixtures::repeated_frets(), &path).unwrap();
    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    fs::remove_file(path).unwrap();
}
