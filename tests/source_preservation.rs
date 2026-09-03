#![cfg(feature = "gp")]

use tabslib::fixtures;

#[test]
fn an_unedited_gp_container_is_byte_identical_after_load_and_save() {
    let original = fixtures::repeated_frets();
    let bytes = tabslib::save(&original).unwrap();
    let loaded = tabslib::load(&bytes).unwrap();
    assert_eq!(tabslib::save(&loaded).unwrap(), bytes);
}

#[test]
fn an_edited_note_is_written_and_remains_readable() {
    let original = fixtures::repeated_frets();
    let bytes = tabslib::save(&original).unwrap();
    let mut edited = tabslib::load(&bytes).unwrap();
    edited.bars[0].voices[0].beats[0].notes[0].fret = Some(9);
    edited.bars[0].voices[0].beats[0].notes[0].midi = Some(44);
    let saved = tabslib::save(&edited).unwrap();
    let reloaded = tabslib::load(&saved).unwrap();
    assert_eq!(reloaded.bars[0].voices[0].beats[0].notes[0].fret, Some(9));
    assert_eq!(reloaded.bars[0].voices[0].beats[0].notes[0].midi, Some(44));
}
