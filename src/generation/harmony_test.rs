use super::*;

#[test]
fn diatonic_third_moves_scale_degrees_and_preserves_chromatic_alterations() {
    let key = KeySignature {
        tonic: 0,
        minor: false,
    };
    assert_eq!(
        target_pitch(60, HarmonyType::Diatonic { steps: 2 }, Some(key)),
        64
    );
    assert_eq!(
        target_pitch(61, HarmonyType::Diatonic { steps: 2 }, Some(key)),
        65
    );
    assert_eq!(
        target_pitch(71, HarmonyType::Diatonic { steps: 2 }, Some(key)),
        74
    );
}

#[test]
fn parallel_harmony_adds_a_chromatic_interval() {
    assert_eq!(
        target_pitch(60, HarmonyType::Parallel { semitones: 7 }, None),
        67
    );
}

#[test]
fn playable_positions_obey_string_and_fret_limits() {
    let config = HarmonyConfig {
        source_track: 0,
        key: None,
        harmony: HarmonyType::Parallel { semitones: 7 },
        tuning: vec![40, 45, 50],
        max_fret: 5,
        max_leap: 12,
        octave_range: 1,
        seed: 0,
        track_name: "Guitar 2".into(),
    };
    let positions = playable_positions(59, 59, &config);
    assert!(!positions.is_empty());
    assert!(positions
        .iter()
        .all(|position| { position.fret >= 0 && position.fret <= 5 && position.string < 3 }));
}

#[test]
fn clamp_source_beats_floors_fast_subdivisions_above_tempo_threshold() {
    let fast_beat = Beat {
        id: 0,
        rhythm: Rhythm::new(NoteValue::ThirtySecond),
        notes: vec![Note {
            id: 0,
            midi: Some(60),
            string: Some(0),
            fret: Some(0),
            articulation: None,
            techniques: Vec::new(),
        }],
        dynamic: None,
    };
    let beats = vec![fast_beat.clone(), fast_beat];
    assert_eq!(clamp_source_beats(&beats, 70.0).len(), 2);

    let clamped = clamp_source_beats(&beats, 120.0);
    assert_eq!(clamped.len(), 1);
    assert_eq!(clamped[0].rhythm, Rhythm::new(NoteValue::Sixteenth));
    assert_eq!(
        inspect::rhythm_duration(clamped[0].rhythm),
        Fraction::new(1, 16)
    );
}

#[test]
fn clamp_source_beats_absorbs_trailing_short_rests_into_preceding_note() {
    let dotted_sixteenth = Beat {
        id: 0,
        rhythm: Rhythm {
            value: NoteValue::Sixteenth,
            dots: 1,
            tuplet: None,
        },
        notes: vec![Note {
            id: 0,
            midi: Some(60),
            string: Some(0),
            fret: Some(0),
            articulation: None,
            techniques: Vec::new(),
        }],
        dynamic: None,
    };
    let thirty_second_rest = Beat {
        id: 0,
        rhythm: Rhythm::new(NoteValue::ThirtySecond),
        notes: Vec::new(),
        dynamic: None,
    };
    let beats = vec![dotted_sixteenth, thirty_second_rest];
    let clamped = clamp_source_beats(&beats, 120.0);
    assert_eq!(clamped.len(), 1);
    assert_eq!(clamped[0].rhythm, Rhythm::new(NoteValue::Eighth));
    assert_eq!(
        inspect::rhythm_duration(clamped[0].rhythm),
        Fraction::new(1, 8)
    );
    assert!(clamped[0].notes.iter().any(|n| n.midi == Some(60)));
}

#[test]
fn generated_harmony_clamps_fast_subdivisions_and_adds_zero_violations() {
    let mut doc = crate::fixtures::repeated_frets();
    let bar_id = doc.master_bars[0].bar_ids[0];
    let bar = doc.bars.iter_mut().find(|b| b.id == bar_id as u32).unwrap();
    let note = Note {
        id: 0,
        midi: Some(50),
        string: Some(2),
        fret: Some(0),
        articulation: None,
        techniques: Vec::new(),
    };
    bar.voices[0].beats = vec![
        Beat {
            id: 0,
            rhythm: Rhythm::new(NoteValue::Quarter),
            notes: vec![note.clone()],
            dynamic: None,
        },
        Beat {
            id: 0,
            rhythm: Rhythm::new(NoteValue::Eighth),
            notes: vec![note.clone()],
            dynamic: None,
        },
        Beat {
            id: 0,
            rhythm: Rhythm::new(NoteValue::ThirtySecond),
            notes: vec![note.clone()],
            dynamic: None,
        },
        Beat {
            id: 0,
            rhythm: Rhythm::new(NoteValue::ThirtySecond),
            notes: vec![note.clone()],
            dynamic: None,
        },
        Beat {
            id: 0,
            rhythm: Rhythm::new(NoteValue::Sixteenth),
            notes: vec![note.clone()],
            dynamic: None,
        },
        Beat {
            id: 0,
            rhythm: Rhythm::new(NoteValue::Half),
            notes: vec![note],
            dynamic: None,
        },
    ];

    #[cfg(feature = "assessment")]
    {
        let source_violations = crate::generation::requirements::check_track(&doc, 0);
        assert!(
            source_violations.iter().any(|v| {
                v.requirement == crate::generation::requirements::Requirement::FastSubdivision
            }),
            "source track must have FastSubdivision violations"
        );
    }

    let config = HarmonyConfig::diatonic_third(0, 7);
    let result = generate_harmony(&mut doc, &config).unwrap();

    let gen_bar_id = doc.master_bars[0].bar_ids[result.track_index];
    let gen_bar = doc.bars.iter().find(|b| b.id == gen_bar_id as u32).unwrap();
    for beat in &gen_bar.voices[0].beats {
        assert!(
            inspect::rhythm_duration(beat.rhythm) >= Fraction::new(1, 16),
            "generated beats must be at least a sixteenth in duration: {:?}",
            beat.rhythm
        );
    }

    #[cfg(feature = "assessment")]
    {
        let gen_violations = crate::generation::requirements::check_track(&doc, result.track_index);
        assert!(
            !gen_violations.iter().any(|v| {
                v.requirement == crate::generation::requirements::Requirement::FastSubdivision
            }),
            "generated harmony track must have zero FastSubdivision violations"
        );
    }
}
