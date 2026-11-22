use super::*;

fn request() -> TokenSoloRequest {
    TokenSoloRequest {
        start_bar: 0,
        end_bar: 0,
        key: KeySignature {
            tonic: 0,
            minor: false,
        },
        scale: vec![0, 2, 4, 5, 7, 9, 11],
        register: (52, 79),
        tuning: vec![40, 45, 50, 55, 59, 64],
        fret_window: (0, 12),
        candidate_count: 3,
        seed: 5,
        track_name: "Token Lead".into(),
    }
}

fn solo_bar(candidate: &SoloTokenCandidate, bar: usize) -> &Bar {
    let id = candidate.document.master_bars[bar].bar_ids[candidate.track_index] as u32;
    candidate
        .document
        .bars
        .iter()
        .find(|item| item.id == id)
        .unwrap()
}

#[test]
fn token_composition_returns_seeded_assessed_candidates_with_guaranteed_requirements() {
    let document = crate::fixtures::power_chords();
    let source = document.clone();
    let candidates = compose_solo_tokens(
        &document,
        &request(),
        "duration=quarter degree=1 hammer-on duration=quarter degree=3 vibrato duration=quarter pitch=67 bend=50 duration=quarter rest",
    )
    .expect("the tokens fill 4/4 and can be fingered");
    assert_eq!(
        document, source,
        "candidate generation does not edit its input"
    );
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0].seed, 5);
    assert_eq!(candidates[1].seed, 6);
    assert_eq!(candidates[0].note_count, 3);
    for candidate in &candidates {
        let failures = requirements::check(&candidate.document)
            .into_iter()
            .filter(|violation| {
                violation.region.track == Some(candidate.track_index)
                    && matches!(
                        violation.requirement,
                        Requirement::AdjacentStrings
                            | Requirement::HandPositionLeap
                            | Requirement::StringMotionRate
                    )
            })
            .collect::<Vec<_>>();
        assert!(
            failures.is_empty(),
            "solver guarantees H3/H4/H14: {failures:?}"
        );
        assert!(!candidate.assessment.metrics.is_empty());
    }
    let beats = &solo_bar(&candidates[0], 0).voices[0].beats;
    assert!(beats[0].notes[0]
        .techniques
        .contains(&Technique::HammerOrigin));
    assert!(beats[1].notes[0]
        .techniques
        .contains(&Technique::HammerDestination));
    assert!(beats[1].notes[0].techniques.contains(&Technique::Vibrato));
    assert!(matches!(
        beats[2].notes[0].techniques.as_slice(),
        [Technique::Bend {
            origin: 0,
            middle: 50,
            dest: 50
        }]
    ));
    assert!(beats[3].notes.is_empty());
}

#[test]
fn parser_rejects_bad_tokens_and_bend_units_are_payload_values() {
    assert!(matches!(
        parse_tokens("pitch=60 nonsense"),
        Err(TokenSoloError::InvalidToken { .. })
    ));
    assert!(matches!(
        parse_tokens("pitch=60 bend=1"),
        Err(TokenSoloError::InvalidToken { .. })
    ));
    let parsed = parse_tokens("pitch=60 bend=100").unwrap();
    assert_eq!(
        parsed[0].techniques,
        [Technique::Bend {
            origin: 0,
            middle: 100,
            dest: 100,
        }]
    );
}

#[test]
fn fingering_respects_the_fret_window_and_reports_when_no_solution_exists() {
    let document = crate::fixtures::power_chords();
    let mut impossible = request();
    impossible.fret_window = (20, 20);
    assert!(matches!(
        compose_solo_tokens(
            &document,
            &impossible,
            "pitch=127 pitch=127 pitch=127 pitch=127"
        ),
        Err(TokenSoloError::NoFingeringSolution { .. })
    ));

    let result = compose_solo_tokens(
        &document,
        &request(),
        "duration=quarter degree=1 duration=quarter degree=2 duration=quarter degree=3 duration=quarter degree=4",
    )
    .unwrap();
    let bar = solo_bar(&result[0], 0);
    for note in bar.voices[0].beats.iter().flat_map(|beat| &beat.notes) {
        assert!((0..=12).contains(&note.fret.unwrap()));
    }
}

#[test]
fn ranking_occurs_only_when_a_caller_supplies_the_ranking_function() {
    let candidates = compose_solo_tokens(
        &crate::fixtures::power_chords(),
        &request(),
        "degree=1 degree=3 degree=5 degree=7",
    )
    .unwrap();
    let ranked = rank_solo_candidates(candidates, |report| {
        i64::try_from(report.metrics[&assessment::Metric::Playability].numerator).unwrap_or(0)
    });
    assert_eq!(ranked.len(), 3);
}
