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

/// Asserted by pitch rather than by string number.
///
/// This previously required `s1` on the top line, which is only correct if
/// string 1 is the highest -- and it is not: strings are 0-based from the
/// lowest. The test agreed with the wrong convention and so could not detect
/// it. Ordering by pitch is true regardless of how strings are numbered.
#[test]
fn ascii_places_highest_pitched_string_first() {
    use tabslib::model::open_pitch;

    let doc = fixtures::bass_line();
    let tuning = &doc.tracks[0].tuning;
    assert!(
        tuning.last().unwrap() > tuning.first().unwrap(),
        "tuning is stored lowest-first"
    );

    let rendered = render_track(&doc, 0, 0, 0).unwrap();
    let pitches: Vec<i32> = rendered
        .lines()
        .filter_map(|line| line.strip_prefix('s'))
        .filter_map(|rest| rest.split('|').next())
        .filter_map(|n| n.trim().parse::<u32>().ok())
        .filter_map(|s| open_pitch(tuning, s))
        .collect();

    assert_eq!(
        pitches.len(),
        tuning.len(),
        "every string should be rendered"
    );
    let mut descending = pitches.clone();
    descending.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(
        pitches, descending,
        "top line must be the highest-pitched string"
    );
}

#[test]
fn gp_rejects_zero_meter_parts_at_the_parse_boundary() {
    let xml = gp::write_payload(&fixtures::repeated_frets())
        .replace("<Time>4/4</Time>", "<Time>4/0</Time>");
    let error = gp::parse_payload(&xml).unwrap_err();
    assert!(error.to_string().contains("time signature"));
}
