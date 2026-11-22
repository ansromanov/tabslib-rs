use super::*;

#[cfg(feature = "riffs")]
use super::super::riff_spec::{Register, RiffArchetype};

#[test]
fn cells_phrases_and_interval_ngrams_are_stable_and_countable() {
    let document = crate::fixtures::repeated_frets();
    let cells = extract_rhythm_cells(&document, &[0], 4).expect("fixture aligns to sixteenths");
    assert_eq!(cells[0].bar, 0);
    assert!(
        cells[0].encoding.contains('x') || cells[0].encoding.contains('p'),
        "sounding attacks, including palm-muted attacks, must be encoded"
    );
    assert!(cells[0].encoding.contains('-'), "rests must be encoded");

    let phrases = extract_rhythm_phrases(&cells, 2).expect("two-bar phrases");
    assert!(!phrases.is_empty());
    assert!(phrases[0].encoding.contains("0=t0v0:"));

    let ngrams = extract_interval_ngrams(&document, &[0], 4, 1).expect("pitched onsets");
    assert!(!ngrams.is_empty());
    assert!(ngrams.iter().flatten().any(|interval| *interval != 0));
}

#[test]
fn recommended_grid_preserves_thirty_second_written_values() {
    let document = crate::fixtures::repeated_frets();
    let grid = recommended_steps_per_quarter(&document).unwrap();
    assert!(grid > 0);
    let extraction = extract_rhythm_cells_tolerant(&document, &[0], grid).unwrap();
    assert_eq!(extraction.skipped, Vec::new());
    assert!(!extraction.cells.is_empty());
}

#[test]
fn caller_documents_build_blend_and_compare_rhythmic_distributions() {
    let first = crate::fixtures::repeated_frets();
    let second = crate::fixtures::power_chords();
    let roles = vec![
        vec!["A".into(), "B".into(), "A".into()],
        vec!["A".into(), "B".into(), "A".into()],
        vec!["A".into(), "C".into()],
    ];
    let left = build_rhythmic_vocabulary(std::slice::from_ref(&first), &[0], 4, 1, &roles)
        .expect("caller samples build a profile");
    let right = build_rhythmic_vocabulary(&[second], &[0], 4, 1, &roles[..1])
        .expect("second caller sample builds a profile");
    assert!(!left.cells.is_empty());
    assert!(!left.intervals.is_empty());
    assert_eq!(left.forms["A>B>A"], 2.0 / 3.0);
    assert_eq!(cosine_similarity(&left.cells, &left.cells), 1.0);
    assert!(cosine_similarity(&left.cells, &right.cells) < 1.0);

    let blended = blend_vocabularies(&left, &right, 1.0).expect("full left weighting");
    assert_eq!(
        blended, left,
        "weight one must preserve every left distribution"
    );
    let midpoint = blend_vocabularies(&left, &right, 0.5).expect("balanced caller weighting");
    assert!(midpoint.cells.values().all(|value| *value >= 0.0));
}

#[cfg(feature = "riffs")]
#[test]
fn caller_profile_applies_and_reports_only_fields_that_changed() {
    let mut spec = super::super::riff_spec::RiffSpec::minimal(vec![40, 45, 50, 55, 59, 64]);
    let profile = CallerRiffProfile {
        archetype: Some(RiffArchetype::Gallop),
        register: Some(Register::Mid),
        degrees: Some(vec![1, 2, 3]),
        tempo: Some(140.0),
        palm_mute: Some(true),
        ..CallerRiffProfile::default()
    };
    let changes = apply_riff_profile(&mut spec, &profile).expect("profile remains playable");
    assert_eq!(changes.len(), 5);
    assert_eq!(spec.archetype, RiffArchetype::Gallop);
    assert_eq!(spec.register, Register::Mid);
    assert_eq!(spec.degrees, vec![1, 2, 3]);
    assert_eq!(spec.tempo, 140.0);
    assert!(spec.palm_mute);
    let unchanged = apply_riff_profile(&mut spec, &profile).expect("idempotent profile");
    assert!(
        unchanged.is_empty(),
        "an already applied profile changes no fields"
    );
}
