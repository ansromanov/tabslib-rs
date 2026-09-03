use tabslib::fixtures;
use tabslib::format::ascii::render_track;
use tabslib::format::gp;
use tabslib::format::midi::Midi;
use tabslib::format::WriteFormat;

#[test]
fn load_dispatches_standard_midi() {
    let bytes = <Midi as WriteFormat>::write(&fixtures::repeated_frets()).unwrap();
    let loaded = tabslib::load(&bytes).unwrap();
    assert_eq!(loaded.note_count(), fixtures::repeated_frets().note_count());
}

#[test]
fn ascii_places_highest_pitched_string_first() {
    let doc = fixtures::bass_line();
    let lines = render_track(&doc, 0, 0, 0).unwrap();
    let lines = lines.lines().collect::<Vec<_>>();
    assert!(doc.tracks[0].tuning.last().unwrap() > doc.tracks[0].tuning.first().unwrap());
    assert!(lines[1].starts_with("s1|"));
    assert!(lines[4].starts_with("s4|"));
}

#[test]
fn gp_rejects_zero_meter_parts_at_the_parse_boundary() {
    let xml = gp::write_payload(&fixtures::repeated_frets())
        .replace("<Time>4/4</Time>", "<Time>4/0</Time>");
    let error = gp::parse_payload(&xml).unwrap_err();
    assert!(error.to_string().contains("time signature"));
}
