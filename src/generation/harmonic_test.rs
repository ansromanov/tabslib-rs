use super::*;

#[test]
fn parses_labels_and_builds_tones() {
    let chord = parse_chord_label("F#m7").unwrap();
    assert_eq!(chord.root, 6);
    assert_eq!(build_chord_tones(chord), vec![6, 9, 1, 4]);
    assert!(is_chord_tone(49, chord));
}

#[test]
fn writes_progression_with_distinct_bar_and_pitch_data() {
    let progression = sample_progression(&["C", "Am"]).unwrap();
    let document = write_harmonic_riff(&progression, &HarmonicRiffConfig::default()).unwrap();
    assert_eq!(document.master_bars.len(), 2);
    assert_eq!(document.note_count(), 12);
    let first = document.bars[0].voices[0].beats[0]
        .notes
        .iter()
        .filter_map(|note| note.midi)
        .map(|pitch| pitch.rem_euclid(12))
        .collect::<Vec<_>>();
    let second = document.bars[1].voices[0].beats[0]
        .notes
        .iter()
        .filter_map(|note| note.midi)
        .map(|pitch| pitch.rem_euclid(12))
        .collect::<Vec<_>>();
    assert_ne!(first, second);
}

#[test]
fn voicing_search_enumerates_shapes_and_open_bias_selects_between_them() {
    let chord = parse_chord_label("C").unwrap();
    let options = VoicingOptions {
        min_fret: 0,
        max_fret: 5,
        max_candidates: 128,
        open_string_bias: true,
    };
    let voicings = find_voicings_with_options(chord, &[40, 45, 50, 55, 59, 64], options).unwrap();
    assert!(voicings.len() > 1);
    let open = pick_best_voicing_with_options(&voicings, options).unwrap();
    let closed = pick_best_voicing_with_options(
        &voicings,
        VoicingOptions {
            open_string_bias: false,
            ..options
        },
    )
    .unwrap();
    assert_ne!(open.frets, closed.frets);
}
