use tabslib::model::open_pitch;
use tabslib::{fixtures, pitch};

#[test]
fn every_named_preset_is_lowest_string_first_and_matches_open_pitch() {
    let expected_low_strings = [40, 39, 38, 37, 38, 36, 35, 33, 35, 33, 28, 26, 23, 21];
    assert_eq!(pitch::TUNING_PRESETS.len(), expected_low_strings.len());
    for (preset, expected) in pitch::TUNING_PRESETS.iter().zip(expected_low_strings) {
        assert_eq!(
            open_pitch(preset.pitches, 0),
            Some(expected),
            "{} must store string zero as its lowest open string",
            preset.name
        );
        assert_eq!(
            pitch::preset_name_for_tuning(preset.pitches),
            Some(preset.name),
            "{} must reverse-resolve from its exact pitch list",
            preset.name
        );
    }
}

#[test]
fn preset_names_resolve_case_insensitively_and_unknown_names_list_options() {
    let preset = pitch::resolve_tuning_preset("  dRoP d  ").unwrap();
    assert_eq!(preset.name, "Drop D");
    assert_eq!(preset.pitches, [38, 45, 50, 55, 59, 64]);

    let error = pitch::resolve_tuning_preset("Drop Z").unwrap_err();
    let message = error.to_string();
    assert!(message.contains("Drop Z"));
    for preset in pitch::TUNING_PRESETS {
        assert!(message.contains(preset.name), "missing {}", preset.name);
    }
}

#[test]
fn preset_name_retuning_preserves_pitch_or_fingering_as_requested() {
    let original = fixtures::repeated_frets();
    let original_pitches = note_pitches(&original);
    let original_positions = note_positions(&original);

    let mut pitch_kept = original.clone();
    pitch::retune_preserve_pitch(&mut pitch_kept, 0, "Eb Standard").unwrap();
    assert_eq!(pitch_kept.tracks[0].tuning, [39, 44, 49, 54, 58, 63]);
    assert_eq!(note_pitches(&pitch_kept), original_pitches);
    assert_ne!(note_positions(&pitch_kept), original_positions);

    let mut fingering_kept = original;
    pitch::retune_preserve_fingering(&mut fingering_kept, 0, "Drop D").unwrap();
    assert_eq!(fingering_kept.tracks[0].tuning, [38, 45, 50, 55, 59, 64]);
    assert_eq!(note_positions(&fingering_kept), original_positions);
    assert_ne!(note_pitches(&fingering_kept), original_pitches);
}

fn note_pitches(document: &tabslib::Document) -> Vec<Option<i32>> {
    document
        .bars
        .iter()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .map(|note| note.midi)
        .collect()
}

fn note_positions(document: &tabslib::Document) -> Vec<(Option<u32>, Option<i32>)> {
    document
        .bars
        .iter()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .map(|note| (note.string, note.fret))
        .collect()
}
