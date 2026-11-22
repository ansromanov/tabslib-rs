use super::*;
use crate::fixtures;

#[test]
fn solo_transform_requires_an_explicit_configuration() {
    let mut document = fixtures::repeated_frets();
    let result = transform_score(&mut document, Archetype::Solo, 0, 0, None);
    assert!(result.is_err());
}

#[test]
fn named_articulations_have_distinct_written_effects() {
    let source = crate::fixtures::meters_and_sections();
    let mut gallop = source.clone();
    let mut tremolo = source.clone();
    let mut breakdown = source;
    apply_articulation(&mut gallop, 0, 0, "gallop").unwrap();
    apply_articulation(&mut tremolo, 0, 0, "tremolo").unwrap();
    apply_articulation(&mut breakdown, 0, 0, "breakdown").unwrap();
    let durations = |document: &crate::Document| {
        document.bars[0].voices[0]
            .beats
            .iter()
            .map(|beat| beat.rhythm.as_fraction())
            .collect::<Vec<_>>()
    };
    assert_ne!(durations(&gallop), durations(&tremolo));
    assert_ne!(durations(&gallop), durations(&breakdown));
}

#[test]
fn non_solo_archetypes_and_section_patches_change_written_material() {
    let mut document = fixtures::repeated_frets();
    transform_score(&mut document, Archetype::Bass, 0, 0, None).unwrap();
    assert!(document.technique_count(|technique| matches!(technique, Technique::Staccato)) > 0);

    patch_score_section(&mut document, 0, 1, 0, true).unwrap();
    assert!(document
        .bars
        .iter()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .any(|beat| beat.dynamic.is_some()));
}

#[test]
fn named_articulations_are_applied_without_duplicate_techniques() {
    let mut document = fixtures::repeated_frets();
    apply_articulation(&mut document, 0, 0, "tremolo").unwrap();
    let first = document.technique_count(|technique| matches!(technique, Technique::Staccato));
    apply_articulation(&mut document, 0, 0, "tremolo").unwrap();
    assert_eq!(
        document.technique_count(|technique| matches!(technique, Technique::Staccato)),
        first
    );
}

#[test]
fn rhythm_articulations_keep_every_bar_at_its_written_length() {
    for articulation in ["gallop", "tremolo", "breakdown"] {
        let mut document = crate::fixtures::meters_and_sections();
        let before = voice_lengths(&document);
        let last = document.master_bars.len() - 1;
        apply_articulation(&mut document, 0, last, articulation).unwrap();
        assert_eq!(voice_lengths(&document), before, "{articulation}");
        crate::edits::assert_no_overfull(&document).unwrap();
    }
}

#[test]
fn rhythm_articulations_reattack_the_pitch_sounding_at_each_onset() {
    let mut document = crate::fixtures::meters_and_sections();
    let source = onset_pitches(&document);
    apply_articulation(&mut document, 0, 0, "breakdown").unwrap();
    let rewritten = onset_pitches(&document);
    assert!(!rewritten.is_empty());
    for (onset, pitches) in &rewritten {
        let sounding = source
            .iter()
            .rev()
            .find(|(start, _)| start <= onset)
            .map(|(_, pitches)| pitches)
            .unwrap();
        assert_eq!(pitches, sounding, "onset {onset:?}");
    }
    let first = &document.bars[0].voices[0].beats[0];
    assert_eq!(first.dynamic.as_deref(), Some("FF"));
}

#[test]
fn extended_articulations_have_observable_written_effects() {
    let source = crate::fixtures::power_chords();
    let mut halftime = source.clone();
    let mut doubletime = source.clone();
    apply_articulation(&mut halftime, 0, 0, "halftime").unwrap();
    apply_articulation(&mut doubletime, 0, 0, "doubletime").unwrap();
    let rhythm = |document: &crate::Document| {
        document.bars[0].voices[0]
            .beats
            .iter()
            .map(|beat| beat.rhythm.as_fraction())
            .collect::<Vec<_>>()
    };
    assert_ne!(rhythm(&halftime), rhythm(&doubletime));

    for articulation in [
        "ringing",
        "stabs",
        "arpeggiate",
        "lowStringOnly",
        "pedal",
        "sequence",
        "fragmentation",
        "inversion",
        "retrograde",
        "interpolation",
        "shiftedAccents",
        "straightOpen",
    ] {
        let mut document = source.clone();
        apply_articulation(&mut document, 0, 0, articulation).unwrap();
    }
}

fn voice_lengths(document: &crate::Document) -> Vec<Fraction> {
    document
        .bars
        .iter()
        .flat_map(|bar| &bar.voices)
        .map(|voice| {
            voice.beats.iter().fold(Fraction::new(0, 1), |total, beat| {
                let (numerator, denominator) = beat.rhythm.as_fraction();
                total.plus(Fraction::new(numerator, denominator))
            })
        })
        .collect()
}

fn onset_pitches(document: &crate::Document) -> Vec<(Fraction, Vec<Option<i32>>)> {
    let mut at = Fraction::new(0, 1);
    document.bars[0].voices[0]
        .beats
        .iter()
        .map(|beat| {
            let entry = (at, beat.notes.iter().map(|note| note.midi).collect());
            let (numerator, denominator) = beat.rhythm.as_fraction();
            at = at.plus(Fraction::new(numerator, denominator));
            entry
        })
        .collect()
}
