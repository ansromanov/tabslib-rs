#![cfg(all(feature = "gp", feature = "musicxml"))]

use std::io::{Cursor, Read, Write};

use tabslib::fixtures;
use tabslib::format::gp::Gp;
use tabslib::format::musicxml::MusicXml;
use tabslib::format::{ReadFormat, WriteFormat};
use tabslib::inspect;
use tabslib::model::Technique;

#[test]
fn retained_container_entries_survive_a_changed_score() {
    let payload = tabslib::format::gp::write_payload(&fixtures::repeated_frets());
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::FileOptions::<()>::default();
    archive.start_file("Content/score.gpif", options).unwrap();
    archive.write_all(payload.as_bytes()).unwrap();
    archive
        .start_file("Content/unknown-settings.bin", options)
        .unwrap();
    archive.write_all(b"preserve me").unwrap();
    let bytes = archive.finish().unwrap().into_inner();

    let mut doc = <Gp as ReadFormat>::read(&bytes).unwrap();
    doc.bars[0].voices[0].beats[0].notes[0].fret = Some(9);
    let saved = <Gp as WriteFormat>::write(&doc).unwrap();
    let mut output = zip::ZipArchive::new(Cursor::new(saved)).unwrap();
    let mut extra = output.by_name("Content/unknown-settings.bin").unwrap();
    let mut data = Vec::new();
    extra.read_to_end(&mut data).unwrap();
    assert_eq!(data, b"preserve me");
}

#[test]
fn mixer_parameters_round_trip_through_gpif() {
    let mut doc = fixtures::repeated_frets();
    doc.tracks[0].pan = Some(-20);
    doc.tracks[0].volume = Some(90);
    doc.tracks[0].mute = true;
    doc.tracks[0].solo = true;
    let xml = tabslib::format::gp::write_payload(&doc);
    let loaded = tabslib::format::gp::parse_payload(&xml).unwrap();
    let track = &loaded.tracks[0];
    assert_eq!(track.pan, Some(-20));
    assert_eq!(track.volume, Some(90));
    assert!(track.mute && track.solo);
}

#[test]
fn ties_and_repeats_are_modelled_and_serialized() {
    let mut doc = fixtures::repeated_frets();
    doc.bars[0].voices[0].beats[0].notes[0]
        .techniques
        .push(Technique::TieOrigin);
    doc.bars[0].voices[0].beats[1].notes[0]
        .techniques
        .push(Technique::TieDestination);
    doc.master_bars[0].repeat_start = true;
    let last = doc.master_bars.len() - 1;
    doc.master_bars[last].repeat_end = Some(2);
    let xml = tabslib::format::gp::write_payload(&doc);
    let loaded = tabslib::format::gp::parse_payload(&xml).unwrap();
    assert_eq!(
        inspect::playback_order(&loaded).len(),
        doc.master_bars.len() * 2
    );
    assert!(loaded.bars[0].voices[0].beats[0].notes[0]
        .techniques
        .contains(&Technique::TieOrigin));
}

#[test]
fn percussion_mapping_and_musicxml_import_are_available() {
    assert_eq!(
        tabslib::percussion::lookup(36).unwrap().1,
        tabslib::percussion::DrumRole::Kick
    );
    assert!(tabslib::percussion::lookup(1).is_none());
    let source = fixtures::repeated_frets();
    let xml = <MusicXml as WriteFormat>::write(&source).unwrap();
    let imported = <MusicXml as ReadFormat>::read(&xml).unwrap();
    assert_eq!(imported.note_count(), source.note_count());
}

#[test]
fn repeated_playback_contributes_to_summary_duration() {
    let mut doc = fixtures::repeated_frets();
    doc.master_bars[0].repeat_start = true;
    let last = doc.master_bars.len() - 1;
    doc.master_bars[last].repeat_end = Some(2);
    assert_eq!(
        inspect::summary(&doc).duration,
        tabslib::inspect::Fraction::new(10, 1)
    );
}
