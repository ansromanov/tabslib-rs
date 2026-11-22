use super::*;

#[test]
fn evaluation_preserves_exact_duration_and_pitch_facts() {
    let score = crate::fixtures::repeated_frets();
    let report = evaluate(&score);
    assert_eq!(report.bars, score.master_bars.len());
    assert!(!report.pitch_classes.is_empty());
    assert!(report.duration.denominator > 0);
}

#[test]
fn content_hash_is_stable() {
    assert_eq!(content_hash(b"tabslib"), content_hash(b"tabslib"));
    assert_ne!(content_hash(b"a"), content_hash(b"b"));
}
