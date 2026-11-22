use super::*;

#[test]
fn bar_duration_uses_meter_and_the_exact_effective_tempo() {
    let three_four_at_120 = bar_duration((3, 4), 120.0).unwrap();
    let seven_eight_at_120 = bar_duration((7, 8), 120.0).unwrap();
    assert_eq!(three_four_at_120, ExactDuration::new(3, 2));
    assert_eq!(seven_eight_at_120, ExactDuration::new(7, 4));

    let mut document = crate::fixtures::power_chords();
    document.master_bars[0].time = (3, 4);
    document.master_bars[1].time = (3, 4);
    document.tempo_map = vec![
        TempoChange {
            at_bar: 0,
            bpm: 120.0,
            linear: false,
        },
        TempoChange {
            at_bar: 1,
            bpm: 60.0,
            linear: false,
        },
    ];
    assert_eq!(
        document_bar_duration(&document, 0).unwrap(),
        three_four_at_120
    );
    assert_eq!(
        document_bar_duration(&document, 1).unwrap(),
        ExactDuration::new(3, 1),
        "a tempo-map change must alter the exact bar duration"
    );
}

#[test]
fn bars_for_minutes_repeats_the_playback_pattern_without_exceeding_the_budget() {
    let mut document = crate::fixtures::power_chords();
    document.master_bars.truncate(1);
    document.master_bars[0].time = (7, 8);
    document.tempo_map = vec![TempoChange {
        at_bar: 0,
        bpm: 120.0,
        linear: false,
    }];

    let result = bars_for_minutes(&document, 1).unwrap();
    assert_eq!(result.bars, 34);
    assert_eq!(result.achieved, ExactDuration::new(119, 2));
    assert!(result.achieved.as_seconds() <= 60.0);
}

#[test]
fn repeat_resolution_reports_the_nearest_unreachable_duration() {
    let mut document = crate::fixtures::power_chords();
    document.master_bars.truncate(1);
    document.master_bars[0].time = (7, 8);
    document.tempo_map = vec![TempoChange {
        at_bar: 0,
        bpm: 120.0,
        linear: false,
    }];

    let result = resolve_repeats(&document, 1).unwrap();
    assert_eq!(result.repeats, 34);
    assert_eq!(result.achieved, ExactDuration::new(119, 2));
    assert!(!result.exact, "34 bars are 59.5 seconds, not one minute");
}

#[test]
fn arrangement_search_is_seeded_ranked_and_reports_metrics_and_violations() {
    let source = crate::fixtures::power_chords();
    let motifs = (0..source.master_bars.len())
        .map(|bar| Motif {
            start_bar: bar,
            end_bar: bar,
            fingerprint: arrangement::fingerprint_bar(&source, bar).unwrap(),
        })
        .collect::<Vec<_>>();
    let plan = ArrangementPlan::new(
        0,
        vec![crate::generation::arrangement::SectionRequest {
            label: "Section".into(),
            bars: 1,
            candidates: (0..motifs.len()).collect(),
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    );
    let request = ArrangementSearchRequest {
        candidate_count: 3,
        seed: 42,
        target_minutes: Some(1),
        weights: ArrangementSearchWeights {
            metrics: BTreeMap::from([(Metric::Playability, 2.0)]),
            hard_violation_penalty: 100.0,
            duration_penalty_per_second: 0.01,
        },
    };

    let first = search_arrangements(&source, &motifs, &plan, &request).unwrap();
    let repeated = search_arrangements(&source, &motifs, &plan, &request).unwrap();
    assert_eq!(
        first, repeated,
        "same seed and inputs must preserve ranking"
    );
    assert_eq!(first.len(), 3);
    assert!(first.iter().all(|candidate| {
        candidate.metrics.metrics.contains_key(&Metric::Playability)
            && candidate.plan.sections[0].candidates.len() == 1
    }));
    assert!(first.windows(2).all(|pair| pair[0].score >= pair[1].score));
    assert!(first
        .iter()
        .all(|candidate| candidate.violations.is_empty()));
}

#[test]
fn search_refuses_to_invent_more_unique_candidates_than_the_inputs_allow() {
    let source = crate::fixtures::power_chords();
    let motif = Motif {
        start_bar: 0,
        end_bar: 0,
        fingerprint: arrangement::fingerprint_bar(&source, 0).unwrap(),
    };
    let plan = ArrangementPlan::new(
        0,
        vec![crate::generation::arrangement::SectionRequest {
            label: "Only".into(),
            bars: 1,
            candidates: vec![0],
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    );
    let error = search_arrangements(
        &source,
        &[motif],
        &plan,
        &ArrangementSearchRequest {
            candidate_count: 2,
            seed: 0,
            target_minutes: None,
            weights: ArrangementSearchWeights::default(),
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ArrangementSearchError::InsufficientCandidates {
            requested: 2,
            available: 1
        }
    ));
}
