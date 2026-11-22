use super::*;

#[test]
fn rhythmic_pattern_must_tile_the_bar_exactly() {
    let eighth = Rhythm::new(crate::model::NoteValue::Eighth);
    let beats = tile_rhythm_pattern(&[eighth], Fraction::new(4, 4)).unwrap();
    assert_eq!(beats.len(), 8);
    assert!(tile_rhythm_pattern(
        &[Rhythm::new(crate::model::NoteValue::Quarter)],
        Fraction::new(7, 8)
    )
    .is_err());
}

#[test]
fn exact_pitch_positions_obey_the_fretboard_limit() {
    let positions = fretboard_positions(47, &[40, 45], 7);
    assert_eq!(
        positions
            .iter()
            .map(|position| position.fret)
            .collect::<Vec<_>>(),
        vec![7, 2]
    );
    assert!(fretboard_positions(47, &[40, 45], 1).is_empty());
}
