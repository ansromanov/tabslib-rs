#![cfg(feature = "drums")]

use tabslib::fixtures;
use tabslib::generation::drums::improvement::{assess, improve, DrumImprovementConfig};
use tabslib::generation::drums::performance::{
    add_transition_marker, apply_articulations, apply_fill, generate_seeded_hits, humanize_hits,
    pair_hihat_chokes, ArticulationRequest, DrumProfile, FillPattern, HumanizationConfig,
};
use tabslib::generation::drums::phrase_clock::PhraseClock;
use tabslib::generation::drums::{
    combine_track_onsets, count_limb_violations, extract_bar_onsets, extract_composite_bar_onsets,
    onset_step_mask, solve_limbs, voices, DrumHit, DrumOrnament, DrumPattern, PatternStep,
    PhraseGrid,
};
use tabslib::inspect::Fraction;

#[test]
fn extracts_exact_riff_onsets_and_masks() {
    let doc = fixtures::power_chords();
    let onsets = extract_bar_onsets(&doc, 0, 0);

    assert_eq!(onsets.len(), 4);
    assert_eq!(onsets[0].step, 0);
    assert_eq!(onsets[1].step, 4);
    assert_eq!(onsets[2].step, 8);
    assert_eq!(onsets[3].step, 12);
    assert!(onsets.iter().all(|onset| onset.exact));
    assert!(onsets.iter().all(|onset| onset.palm_mute));
    assert_eq!(onsets[1].onset, Fraction::new(1, 4));
    assert_eq!(
        onset_step_mask(&onsets, 16),
        vec![
            true, false, false, false, true, false, false, false, true, false, false, false, true,
            false, false, false,
        ]
    );
}

#[test]
fn combines_unison_tracks_without_losing_articulation_flags() {
    let doc = fixtures::power_chords();
    let first = extract_bar_onsets(&doc, 0, 0);
    let mut second = first.clone();
    for onset in &mut second {
        onset.track_names = vec!["guitar-two".to_string()];
    }
    let combined = combine_track_onsets(&[first, second]);

    assert_eq!(combined.len(), 4);
    assert!(combined.iter().all(|onset| onset.track_count == 2));
    assert!(combined.iter().all(|onset| onset.palm_mute));
    assert!(combined.iter().all(|onset| onset.accent));
}

#[test]
fn percussion_tracks_are_not_treated_as_pitched_riffs() {
    let doc = fixtures::drum_patterns();

    assert!(extract_bar_onsets(&doc, 0, 0).is_empty());
    assert!(extract_composite_bar_onsets(&doc, &[0], 0).is_empty());
}

#[test]
fn phrase_grid_and_pattern_keep_bar_boundaries_and_articulations() {
    let doc = fixtures::power_chords();
    let first = extract_bar_onsets(&doc, 0, 0);
    let second = extract_bar_onsets(&doc, 0, 1);
    let grid = PhraseGrid::from_onsets(&[first.clone(), second], 16);
    let pattern = DrumPattern::from_onsets(&[first], 16);

    assert_eq!(grid.bars.len(), 2);
    assert_eq!(grid.bars[0], grid.bars[1]);
    assert_eq!(grid.flattened().len(), 32);
    assert_eq!(pattern.bars[0].iter().filter(|step| step.onset).count(), 4);
    assert!(pattern.bars[0]
        .iter()
        .filter(|step| step.onset)
        .all(|step| step.palm_mute));
}

fn hit(midi: u8, step: u32) -> DrumHit {
    DrumHit::new(midi, step)
}

#[test]
fn limb_solver_applies_hand_priority_and_fill_rules() {
    let mut hits = vec![
        hit(voices::ACOUSTIC_SNARE, 0),
        hit(voices::CRASH_CYMBAL_1, 0),
        hit(voices::CLOSED_HI_HAT, 0),
        hit(voices::HIGH_TOM, 0),
    ];
    hits[0].accent = true;

    let solved = solve_limbs(&hits, 120, false);

    assert_eq!(solved.len(), 2);
    assert!(solved.iter().any(|hit| hit.midi == voices::ACOUSTIC_SNARE));
    assert!(solved.iter().any(|hit| hit.midi == voices::CRASH_CYMBAL_1));
    assert!(!solved.iter().any(|hit| hit.midi == voices::CLOSED_HI_HAT));
    assert_eq!(count_limb_violations(&hits, 120), 2);
}

#[test]
fn limb_solver_alternates_close_kicks_and_drops_conflicting_pedal_hat() {
    let hits = vec![
        hit(voices::ACOUSTIC_BASS_DRUM, 0),
        hit(voices::ACOUSTIC_BASS_DRUM, 1),
        hit(voices::PEDAL_HI_HAT, 1),
    ];

    let solved = solve_limbs(&hits, 140, false);

    assert_eq!(solved.len(), 2);
    assert!(solved
        .iter()
        .all(|hit| hit.midi == voices::ACOUSTIC_BASS_DRUM));
    assert_eq!(count_limb_violations(&hits, 140), 1);
}

#[test]
fn limb_solver_is_deterministic_and_caps_each_limb_group() {
    let hits = vec![
        hit(voices::ACOUSTIC_SNARE, 0),
        hit(voices::CLOSED_HI_HAT, 0),
        hit(voices::RIDE_CYMBAL_1, 0),
        hit(voices::SPLASH_CYMBAL, 0),
        hit(voices::ACOUSTIC_BASS_DRUM, 0),
        hit(voices::PEDAL_HI_HAT, 0),
        hit(voices::ACOUSTIC_BASS_DRUM, 0),
    ];

    let first = solve_limbs(&hits, 120, true);
    let second = solve_limbs(&hits, 120, true);

    assert_eq!(first, second);
    assert_eq!(
        first
            .iter()
            .filter(|hit| hit.midi == voices::ACOUSTIC_BASS_DRUM)
            .count(),
        2
    );
    assert!(
        first
            .iter()
            .filter(|hit| !matches!(hit.midi, voices::ACOUSTIC_BASS_DRUM | voices::PEDAL_HI_HAT))
            .count()
            <= 2
    );
}

#[test]
fn seeded_generation_is_reproducible_and_profile_driven() {
    let source = DrumPattern {
        steps_per_bar: 4,
        bars: vec![vec![
            tabslib::generation::drums::PatternStep {
                onset: true,
                accent: true,
                dead: false,
                palm_mute: false,
            },
            tabslib::generation::drums::PatternStep {
                onset: true,
                accent: false,
                dead: false,
                palm_mute: false,
            },
            tabslib::generation::drums::PatternStep {
                onset: false,
                accent: false,
                dead: false,
                palm_mute: false,
            },
            tabslib::generation::drums::PatternStep {
                onset: true,
                accent: false,
                dead: true,
                palm_mute: false,
            },
        ]],
    };
    let profile = DrumProfile {
        kick_voice: voices::ACOUSTIC_BASS_DRUM,
        snare_voice: voices::ACOUSTIC_SNARE,
        cymbal_voice: voices::CLOSED_HI_HAT,
        kick_probability: 100,
        snare_probability: 0,
        cymbal_probability: 0,
    };

    let first = generate_seeded_hits(&source, &profile, 42);
    let second = generate_seeded_hits(&source, &profile, 42);

    assert_eq!(first, second);
    assert_eq!(
        first.iter().map(|hit| hit.step).collect::<Vec<_>>(),
        vec![0, 0, 1, 3]
    );
    assert_eq!(
        first
            .iter()
            .filter(|hit| hit.midi == voices::ACOUSTIC_BASS_DRUM)
            .count(),
        3
    );
    assert!(first.iter().any(|hit| hit.midi == voices::ACOUSTIC_SNARE));
}

#[test]
fn fills_and_transitions_are_explicit_and_idempotent() {
    let mut hits = vec![hit(voices::CLOSED_HI_HAT, 0), hit(voices::CLOSED_HI_HAT, 1)];
    let fill = FillPattern {
        length: 2,
        hits: vec![hit(voices::HIGH_TOM, 0), hit(voices::LOW_TOM, 1)],
    };

    assert_eq!(apply_fill(&mut hits, &fill, 4, Some(0..2)), 2);
    assert_eq!(
        hits.iter().map(|hit| hit.step).collect::<Vec<_>>(),
        vec![4, 5]
    );
    assert!(add_transition_marker(&mut hits, 6, voices::CRASH_CYMBAL_1));
    assert!(!add_transition_marker(&mut hits, 6, voices::CRASH_CYMBAL_1));
}

#[test]
fn humanization_is_seeded_and_preserves_hit_identity() {
    let mut first = vec![hit(voices::ACOUSTIC_SNARE, 2), hit(voices::HIGH_TOM, 3)];
    let mut second = first.clone();
    humanize_hits(
        &mut first,
        7,
        HumanizationConfig {
            velocity_range: 8,
            timing_range: 2,
        },
    );
    humanize_hits(
        &mut second,
        7,
        HumanizationConfig {
            velocity_range: 8,
            timing_range: 2,
        },
    );

    assert_eq!(first, second);
    assert_eq!(
        first
            .iter()
            .map(|hit| (hit.midi, hit.step))
            .collect::<Vec<_>>(),
        vec![(38, 2), (50, 3)]
    );
    assert!(first.iter().all(|hit| (1..=127).contains(&hit.velocity)));
    assert!(first
        .iter()
        .all(|hit| (-2..=2).contains(&hit.timing_offset)));
}

#[test]
fn phrase_clock_schedules_four_eight_and_sixteen_bar_ends_and_sections() {
    let fills = PhraseClock::schedule_fills(16, 16, 4, &[6, 8]);

    assert_eq!(
        fills
            .iter()
            .map(|fill| fill.bar_boundary)
            .collect::<Vec<_>>(),
        vec![4, 6, 8, 12, 16]
    );
    assert_eq!(fills[0].phrase_lengths, vec![4]);
    assert!(fills[1].section_boundary);
    assert_eq!(fills[1].start_step, 92);
    assert_eq!(fills[2].phrase_lengths, vec![4, 8]);
    assert!(fills[2].section_boundary);
    assert_eq!(fills[4].phrase_lengths, vec![4, 8, 16]);
    assert!(fills.iter().all(|fill| fill.length == 4));
}

#[test]
fn seeded_generation_uses_absolute_steps_across_bars() {
    let source = DrumPattern {
        steps_per_bar: 4,
        bars: vec![
            vec![
                PatternStep {
                    onset: true,
                    ..PatternStep::default()
                };
                4
            ],
            vec![
                PatternStep {
                    onset: true,
                    ..PatternStep::default()
                };
                4
            ],
        ],
    };
    let profile = DrumProfile {
        kick_voice: voices::ACOUSTIC_BASS_DRUM,
        snare_voice: voices::ACOUSTIC_SNARE,
        cymbal_voice: voices::CLOSED_HI_HAT,
        kick_probability: 100,
        snare_probability: 0,
        cymbal_probability: 0,
    };

    let hits = generate_seeded_hits(&source, &profile, 12);

    assert_eq!(
        hits.iter().map(|hit| hit.step).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4, 5, 6, 7]
    );
}

#[test]
fn open_hats_are_choked_by_the_earliest_later_closed_or_pedal_hat() {
    let mut hits = vec![
        hit(voices::OPEN_HI_HAT, 2),
        hit(voices::CLOSED_HI_HAT, 6),
        hit(voices::PEDAL_HI_HAT, 5),
        hit(voices::OPEN_HI_HAT, 7),
        hit(voices::CLOSED_HI_HAT, 7),
    ];

    assert_eq!(pair_hihat_chokes(&mut hits), 1);
    assert_eq!(hits[0].choked_at_step, Some(5));
    assert_eq!(hits[3].choked_at_step, None);
    assert_eq!(pair_hihat_chokes(&mut hits), 0);
}

#[test]
fn requested_flams_and_drags_add_timed_ghost_hits() {
    let mut hits = vec![
        hit(voices::ACOUSTIC_SNARE, 4),
        hit(voices::ACOUSTIC_SNARE, 8),
    ];
    let requests = [
        ArticulationRequest {
            midi: voices::ACOUSTIC_SNARE,
            step: 4,
            ornament: DrumOrnament::Flam,
        },
        ArticulationRequest {
            midi: voices::ACOUSTIC_SNARE,
            step: 8,
            ornament: DrumOrnament::Drag,
        },
    ];

    assert_eq!(apply_articulations(&mut hits, &requests, 2), 2);
    assert_eq!(hits.len(), 5);
    assert_eq!(
        hits.iter()
            .filter(|hit| hit.step == 4 && hit.ghost)
            .map(|hit| hit.timing_offset)
            .collect::<Vec<_>>(),
        vec![-2]
    );
    assert_eq!(
        hits.iter()
            .filter(|hit| hit.step == 8 && hit.ghost)
            .map(|hit| hit.timing_offset)
            .collect::<Vec<_>>(),
        vec![-4, -2]
    );
    assert!(hits
        .iter()
        .any(|hit| hit.step == 4 && hit.ornament == Some(DrumOrnament::Flam)));
    assert!(hits
        .iter()
        .any(|hit| hit.step == 8 && hit.ornament == Some(DrumOrnament::Drag)));
}

#[test]
fn assessment_driven_improvement_increases_kick_lock_and_varies_repeated_bars() {
    let source_bar = || {
        vec![
            PatternStep {
                onset: true,
                ..PatternStep::default()
            },
            PatternStep::default(),
            PatternStep {
                onset: true,
                ..PatternStep::default()
            },
            PatternStep::default(),
        ]
    };
    let source = DrumPattern {
        steps_per_bar: 4,
        bars: vec![source_bar(), source_bar()],
    };
    let mut hits = vec![
        hit(voices::ACOUSTIC_BASS_DRUM, 0),
        hit(voices::CLOSED_HI_HAT, 3),
        hit(voices::ACOUSTIC_BASS_DRUM, 4),
        hit(voices::CLOSED_HI_HAT, 7),
    ];
    let initial = assess(&hits, &source, voices::ACOUSTIC_BASS_DRUM);

    assert_eq!(initial.kick_lock_density.numerator, 2);
    assert_eq!(initial.kick_lock_density.denominator, 4);
    assert_eq!(initial.repeated_bars, 1);

    let report = improve(
        &mut hits,
        &source,
        DrumImprovementConfig {
            kick_voice: voices::ACOUSTIC_BASS_DRUM,
            target_kick_lock_percent: 75,
            max_repeated_bar_percent: 0,
            max_passes: 4,
        },
    );

    assert_eq!(report.initial, initial);
    assert_eq!(report.final_assessment.kick_lock_density.numerator, 3);
    assert_eq!(report.final_assessment.repeated_bars, 0);
    assert_eq!(report.changed_kicks, 1);
    assert!(report.targets_met);
    assert!(hits
        .iter()
        .any(|hit| hit.midi == voices::ACOUSTIC_BASS_DRUM && hit.step == 2));
}
