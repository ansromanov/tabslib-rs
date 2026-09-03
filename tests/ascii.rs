#![cfg(feature = "ascii")]

use tabslib::edits;
use tabslib::fixtures;
use tabslib::format::ascii::{render_compare, render_track, Ascii};
use tabslib::format::WriteFormat;

#[test]
fn fixture_ascii_is_stable_and_golden() {
    let doc = fixtures::repeated_frets();
    let rendered = render_track(&doc, 0, 0, 0).unwrap();
    assert_eq!(
        rendered,
        "gtr [1..=1]\ns1| -   -   -   -   -   -   -   -  |\ns2| -   -   -   -   -   -   -   -  |\ns3| -   -   -   -   -   -   -   -  |\ns4| -   -   -   -   -   -   -   -  |\ns5| -   -   -   -   -   -   -   -  |\ns6| 5   5   5   5   5   5   5   5  |"
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
