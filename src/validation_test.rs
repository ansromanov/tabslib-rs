use super::*;
use crate::fixtures;

#[test]
fn valid_fixture_has_no_structural_issues() {
    assert!(validate(&fixtures::repeated_frets()).is_empty());
}

#[test]
fn validation_reports_dangling_bars_and_bad_ranges() {
    let mut document = fixtures::repeated_frets();
    document.master_bars[0].bar_ids[0] = -1;
    let issues = validate(&document);
    assert!(issues.iter().any(|issue| issue.path.contains("barIds")));

    let mut document = fixtures::repeated_frets();
    document.bars[0].voices[0].beats[0].notes[0].fret = Some(-1);
    let issues = validate(&document);
    assert!(issues.iter().any(|issue| issue.path.ends_with(".fret")));
}

#[cfg(feature = "gp")]
#[test]
fn load_with_diagnostics_keeps_valid_documents_clean() {
    let bytes = crate::save(&fixtures::repeated_frets()).expect("fixture saves");
    let (_, diagnostics) = crate::load_with_diagnostics(&bytes).expect("fixture loads");
    assert!(diagnostics.is_empty());
}
