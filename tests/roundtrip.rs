//! Round-trip invariants over generated fixtures.
//!
//! No file is read and no recording is involved: every document here is built
//! in code from generic instrumental material. The crate is fully testable with
//! no corpus of any kind.
//!
//! Each assertion is a quantity that *changes* when the feature under test is
//! silently dropped. Note count, title and track count all stay correct while
//! rhythm and articulation are being destroyed, so none of them is sufficient
//! alone -- the duration histogram and the technique census are the assertions
//! that carry weight.

use std::collections::BTreeMap;
use tabslib::fixtures;
use tabslib::model::{NoteValue, Rhythm, Technique};
use tabslib::Document;

fn durations(doc: &Document) -> BTreeMap<(u64, u64), usize> {
    let mut h = BTreeMap::new();
    for bar in &doc.bars {
        for v in &bar.voices {
            for b in &v.beats {
                *h.entry(b.rhythm.as_fraction()).or_insert(0) += 1;
            }
        }
    }
    h
}

fn techniques(doc: &Document) -> BTreeMap<String, usize> {
    let mut c = BTreeMap::new();
    for bar in &doc.bars {
        for v in &bar.voices {
            for b in &v.beats {
                for n in &b.notes {
                    for t in &n.techniques {
                        *c.entry(format!("{t:?}")).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    c
}

fn positions(doc: &Document) -> Vec<(Option<u32>, Option<i32>, Option<i32>)> {
    doc.bars
        .iter()
        .flat_map(|b| &b.voices)
        .flat_map(|v| &v.beats)
        .flat_map(|b| &b.notes)
        .map(|n| (n.string, n.fret, n.articulation))
        .collect()
}

#[test]
fn round_trip_changes_nothing_measurable() {
    for (name, before) in fixtures::all() {
        let bytes = tabslib::save(&before).unwrap_or_else(|e| panic!("{name}: save: {e}"));
        let after = tabslib::load(&bytes).unwrap_or_else(|e| panic!("{name}: reload: {e}"));

        assert_eq!(
            after.note_count(),
            before.note_count(),
            "{name}: note count"
        );
        assert_eq!(
            after.tracks.len(),
            before.tracks.len(),
            "{name}: track count"
        );
        assert_eq!(after.bars.len(), before.bars.len(), "{name}: bar count");
        assert_eq!(
            after.section_count(),
            before.section_count(),
            "{name}: sections"
        );
        assert_eq!(
            durations(&after),
            durations(&before),
            "{name}: duration histogram"
        );
        assert_eq!(
            techniques(&after),
            techniques(&before),
            "{name}: technique census"
        );
        assert_eq!(
            positions(&after),
            positions(&before),
            "{name}: string/fret/articulation"
        );
    }
}

#[test]
fn every_note_value_survives_a_round_trip() {
    let before = fixtures::all_note_values();
    let after = tabslib::load(&tabslib::save(&before).expect("save")).expect("reload");
    let (a, b) = (durations(&before), durations(&after));
    assert_eq!(a, b, "durations changed\n before: {a:?}\n after:  {b:?}");
    // the fixture is built to contain tuplets; if it does not, it is not testing them
    assert!(
        a.keys().any(|(_, d)| d % 3 == 0),
        "fixture has no triplets to test"
    );
    assert!(
        a.keys().any(|(_, d)| d % 5 == 0),
        "fixture has no quintuplets to test"
    );
}

#[test]
fn tunings_and_colours_survive() {
    for (name, before) in fixtures::all() {
        let after = tabslib::load(&tabslib::save(&before).expect("save")).expect("reload");
        for (x, y) in before.tracks.iter().zip(after.tracks.iter()) {
            assert_eq!(y.tuning, x.tuning, "{name}: tuning on {}", x.name);
            assert_eq!(y.color, x.color, "{name}: colour on {}", x.name);
            assert_eq!(y.name, x.name, "{name}: track name");
        }
    }
}

#[test]
fn meters_and_section_names_survive() {
    let before = fixtures::meters_and_sections();
    let after = tabslib::load(&tabslib::save(&before).expect("save")).expect("reload");
    let m = |d: &Document| {
        d.master_bars
            .iter()
            .map(|m| (m.time, m.section.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(m(&after), m(&before), "meters or section names changed");
}

#[test]
fn save_is_deterministic() {
    for (name, doc) in fixtures::all() {
        assert_eq!(
            tabslib::save(&doc).expect("a"),
            tabslib::save(&doc).expect("b"),
            "{name}: save is not byte-deterministic"
        );
    }
}

#[test]
fn rhythm_fractions_are_exact() {
    assert_eq!(Rhythm::new(NoteValue::Quarter).as_fraction(), (1, 4));
    assert_eq!(
        Rhythm {
            value: NoteValue::Half,
            dots: 1,
            tuplet: None
        }
        .as_fraction(),
        (3, 4)
    );
    assert_eq!(
        Rhythm {
            value: NoteValue::Eighth,
            dots: 0,
            tuplet: Some((3, 2))
        }
        .as_fraction(),
        (1, 12)
    );
    assert_eq!(
        Rhythm {
            value: NoteValue::Quarter,
            dots: 2,
            tuplet: None
        }
        .as_fraction(),
        (7, 16)
    );
}

#[test]
fn note_values_use_the_spelling_guitar_pro_writes() {
    assert_eq!(NoteValue::Sixteenth.as_gpif(), "16th");
    assert_eq!(NoteValue::ThirtySecond.as_gpif(), "32nd");
    assert_eq!(NoteValue::Quarter.as_gpif(), "Quarter");
    assert_eq!(NoteValue::parse("Sixteenth"), Some(NoteValue::Sixteenth));
    assert_eq!(NoteValue::parse("16th"), Some(NoteValue::Sixteenth));
}

#[test]
fn palm_mute_is_read_from_the_property_attribute() {
    // the technique is written as <Property name="PalmMuted">, not an element
    // named PalmMute; a reader matching element names sees none of them
    let doc = fixtures::repeated_frets();
    let expected = doc.technique_count(|t| matches!(t, Technique::PalmMute));
    assert!(expected > 0, "fixture has no palm mutes to test");
    let after = tabslib::load(&tabslib::save(&doc).expect("save")).expect("reload");
    assert_eq!(
        after.technique_count(|t| matches!(t, Technique::PalmMute)),
        expected
    );
}
