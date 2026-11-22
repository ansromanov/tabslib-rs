#![cfg(feature = "ascii")]

use tabslib::edits;
use tabslib::fixtures;
use tabslib::format::ascii::{render_compare, render_track, Ascii};
use tabslib::format::WriteFormat;

/// The golden encodes the string convention, so it is written out in full:
/// strings are 0-based from the lowest, and tablature is read with the highest
/// on the top line, so `s5` leads and the low-string riff lands on `s0`.
///
/// The previous golden read `s1`..`s6` with the notes on `s6`. It agreed with a
/// wrong convention rather than with the music, which is why it passed while
/// every string number in the library was off.
#[test]
fn fixture_ascii_is_stable_and_golden() {
    let doc = fixtures::repeated_frets();
    let rendered = render_track(&doc, 0, 0, 0).unwrap();
    assert_eq!(
        rendered,
        "gtr [1..=1]\ns5| -   -   -   -   -   -   -   -  |\ns4| -   -   -   -   -   -   -   -  |\ns3| -   -   -   -   -   -   -   -  |\ns2| -   -   -   -   -   -   -   -  |\ns1| -   -   -   -   -   -   -   -  |\ns0| 5   5   5   5   5   5   5   5  |"
    );
    let one_bar = edits::slice(&doc, 0, 0).unwrap();
    assert_eq!(
        rendered,
        String::from_utf8(<Ascii as WriteFormat>::write(&one_bar).unwrap()).unwrap()
    );
}

#[test]
fn ascii_renders_ranges_and_side_by_side_windows() {
    let doc = fixtures::repeated_frets();
    let range = render_track(&doc, 0, 1, 2).unwrap();
    assert!(range.starts_with("gtr [2..=3]"));
    let comparison = render_compare(&doc, 0, (0, 0), (1, 1)).unwrap();
    assert!(comparison.contains(" | gtr [2..=2]"));
}

#[test]
fn ascii_rejects_invalid_selection() {
    let doc = fixtures::repeated_frets();
    assert!(render_track(&doc, 1, 0, 0).is_err());
    assert!(render_track(&doc, 0, 1, 0).is_err());
}
