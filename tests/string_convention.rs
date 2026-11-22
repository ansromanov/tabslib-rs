//! The string-numbering convention, asserted rather than assumed.
//!
//! Strings are 0-based and counted from the lowest, which is what the file
//! format stores. Three formulas were plausible; only this one is right, and
//! the wrong ones were in use across four modules while every test passed —
//! because the fixtures were built with the same wrong formula and therefore
//! agreed with it.
//!
//! These assertions are written against pitch, which is externally true, rather
//! than against a string number, which is a convention the library could get
//! wrong in both places at once.

use tabslib::fixtures;
use tabslib::model::{open_pitch, sounding_pitch};

/// The invariant every other string operation rests on.
#[test]
fn a_note_sounds_the_pitch_its_string_and_fret_imply() {
    for (name, doc) in fixtures::all() {
        for (index, track) in doc.tracks.iter().enumerate() {
            if track.tuning.is_empty() {
                continue;
            }
            for bar in &doc.bars {
                for voice in &bar.voices {
                    for beat in &voice.beats {
                        for note in &beat.notes {
                            let (Some(string), Some(fret), Some(midi)) =
                                (note.string, note.fret, note.midi)
                            else {
                                continue;
                            };
                            assert_eq!(
                                sounding_pitch(&track.tuning, string, fret),
                                Some(midi),
                                "{name}: track {index}, string {string} fret {fret} \
                                 should sound {midi}"
                            );
                        }
                    }
                }
            }
        }
    }
}

/// String 0 is the lowest string, not the highest, and not out of range.
#[test]
fn string_zero_is_the_lowest_string() {
    let tuning = fixtures::TUNING_E_STANDARD;
    assert_eq!(open_pitch(&tuning, 0), Some(40), "string 0 is low E");
    assert_eq!(open_pitch(&tuning, 5), Some(64), "string 5 is high E");
    assert_eq!(open_pitch(&tuning, 6), None, "there is no string 6");
    assert!(
        open_pitch(&tuning, 0) < open_pitch(&tuning, 5),
        "string numbers ascend in pitch"
    );
}

/// Tablature is read with the highest string on top. Asserted by pitch, so it
/// stays correct even if the numbering convention were ever to change.
#[test]
fn tablature_puts_the_highest_string_on_the_top_line() {
    use tabslib::{format::ascii::Ascii, WriteFormat};

    let doc = fixtures::bass_line();
    let tuning = &doc.tracks[0].tuning;
    let rendered = String::from_utf8(Ascii::write(&doc).expect("render")).expect("utf-8");

    let order: Vec<u32> = rendered
        .lines()
        .filter_map(|line| line.strip_prefix('s'))
        .filter_map(|rest| rest.split('|').next())
        .filter_map(|n| n.trim().parse().ok())
        .collect();
    assert!(
        order.len() >= 2,
        "expected several string lines, got {order:?}"
    );

    let pitches: Vec<i32> = order
        .iter()
        .map(|s| open_pitch(tuning, *s).expect("string within the tuning"))
        .collect();
    let mut descending = pitches.clone();
    descending.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(
        pitches, descending,
        "top line must be the highest-pitched string; got pitches {pitches:?}"
    );
}
