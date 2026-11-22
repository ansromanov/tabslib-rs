use super::*;

#[test]
fn tempo_at_uses_the_latest_change_at_or_before_the_bar() {
    let document = crate::fixtures::tempo_changes();
    // Changes: bar 0 -> 105, bar 2 -> 72, bar 4 -> 160.
    let tempi: Vec<Option<f64>> = (0..6).map(|bar| document.tempo_at(bar)).collect();
    assert_eq!(
        tempi,
        vec![
            Some(105.0),
            Some(105.0),
            Some(72.0),
            Some(72.0),
            Some(160.0),
            Some(160.0)
        ]
    );
}

#[test]
fn tempo_at_is_unknown_before_the_first_change() {
    let mut document = crate::fixtures::tempo_changes();
    document.tempo_map.remove(0);
    assert_eq!(document.tempo_at(1), None);
    assert_eq!(document.tempo_at(2), Some(72.0));
}

#[test]
fn string_zero_is_the_lowest_string() {
    // The doc on `Note::string` promises this; `open_pitch` implements it.
    let tuning = [40, 45, 50, 55, 59, 64];
    assert_eq!(open_pitch(&tuning, 0), Some(40));
    assert_eq!(open_pitch(&tuning, 5), Some(64));
}
