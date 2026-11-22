#![cfg(feature = "rhythm")]

use tabslib::fixtures;
use tabslib::generation::drums::extract_bar_onsets;
use tabslib::generation::rhythm::{
    apply_bass_fill, derive_bass_tuning, extract_riff_roots, generate_bass_pattern,
    generate_riff_locked_bass, optimize_bass_positions, score_rhythm_compatibility,
    voice_leading_leaps, BassFillStep, BassHit, BassProfile, BassStrategy,
};

fn profile() -> BassProfile {
    BassProfile {
        min_midi: 28,
        max_midi: 52,
        max_leap: 7,
        accent_velocity: 110,
        normal_velocity: 90,
    }
}

#[test]
fn extracts_root_candidates_from_riff_onsets() {
    let doc = fixtures::power_chords();
    let onsets = extract_bar_onsets(&doc, 0, 0);
    let roots = extract_riff_roots(&onsets);

    assert_eq!(roots.len(), onsets.len());
    assert!(roots.iter().all(|root| root.pitch_class < 12));
    assert_eq!(roots[0].step, onsets[0].step);
    assert_eq!(roots[0].midi, onsets[0].lowest_pitch.unwrap());
}

#[test]
fn generates_riff_locked_bass_inside_external_register_profile() {
    let doc = fixtures::power_chords();
    let onsets = extract_bar_onsets(&doc, 0, 0);
    let bass = generate_riff_locked_bass(&onsets, profile());

    assert_eq!(bass.len(), onsets.len());
    assert!(bass.iter().all(|hit| (28..=52).contains(&hit.midi)));
    assert!(bass.iter().all(|hit| !hit.fill));
    assert!(voice_leading_leaps(&bass).iter().all(|leap| *leap <= 7));
}

#[test]
fn bass_fills_are_marked_and_excluded_from_riff_lock_score() {
    let doc = fixtures::power_chords();
    let onsets = extract_bar_onsets(&doc, 0, 0);
    let mut bass = generate_riff_locked_bass(&onsets, profile());
    let inserted = apply_bass_fill(
        &mut bass,
        &[BassFillStep {
            step: 1,
            midi: 35,
            velocity: 80,
        }],
        20,
    );
    let score = score_rhythm_compatibility(&onsets, &bass);

    assert_eq!(inserted, 1);
    assert!(bass.iter().any(|hit| hit.fill && hit.step == 21));
    assert_eq!(score.locked_onsets, onsets.len());
    assert_eq!(score.score, 1.0);
}

fn onset(step: u32, midi: i32, accent: bool) -> tabslib::generation::drums::RiffOnset {
    tabslib::generation::drums::RiffOnset {
        step,
        exact: true,
        onset: tabslib::inspect::Fraction::new(u64::from(step), 16),
        lowest_pitch: Some(midi),
        palm_mute: false,
        accent,
        dead: false,
        polyphony: 1,
        track_count: 1,
        track_names: vec!["Rhythm".into()],
        unison_score: 1,
    }
}

#[test]
fn bass_strategies_produce_distinct_patterns_and_sustained_durations() {
    let roots = [onset(0, 40, true), onset(8, 43, false), onset(12, 45, true)];
    let riff = generate_bass_pattern(&roots, profile(), BassStrategy::RiffFollow, 16);
    let pumping = generate_bass_pattern(&roots, profile(), BassStrategy::PumpingEighths, 16);
    let fifths = generate_bass_pattern(&roots, profile(), BassStrategy::RootFifth, 16);
    let walking = generate_bass_pattern(&roots, profile(), BassStrategy::Walking, 16);
    let sustained = generate_bass_pattern(&roots, profile(), BassStrategy::Sustained, 16);

    assert_eq!(
        riff.iter().map(|hit| hit.step).collect::<Vec<_>>(),
        [0, 8, 12]
    );
    assert_eq!(
        pumping.iter().map(|hit| hit.step).collect::<Vec<_>>(),
        [0, 2, 4, 6, 8, 10, 12, 14]
    );
    assert_eq!(
        fifths[1].midi % 12,
        u8::try_from((roots[1].lowest_pitch.unwrap() + 7) % 12).unwrap()
    );
    assert!(walking.len() > riff.len());
    assert_eq!(sustained.len(), 1);
    assert_eq!(sustained[0].duration_steps, 16);
}

#[test]
fn supported_guitar_tunings_map_to_matching_bass_ranges() {
    assert_eq!(
        derive_bass_tuning(&tabslib::fixtures::TUNING_E_STANDARD),
        Some(vec![28, 33, 38, 43])
    );
    assert_eq!(
        derive_bass_tuning(&tabslib::fixtures::TUNING_DROP_D),
        Some(vec![26, 33, 38, 43])
    );
    assert_eq!(
        derive_bass_tuning(&[36, 43, 48, 53, 57, 62]),
        Some(vec![24, 31, 36, 41])
    );
    assert_eq!(
        derive_bass_tuning(&[35, 42, 47, 52, 56, 61]),
        Some(vec![23, 30, 35, 40])
    );
    assert_eq!(
        derive_bass_tuning(&[33, 40, 45, 50, 54, 59, 64]),
        Some(vec![21, 28, 33, 38, 43])
    );
    assert_eq!(
        derive_bass_tuning(&[37, 42, 47, 52, 56, 61]),
        Some(vec![25, 30, 35, 40])
    );
    assert_eq!(
        derive_bass_tuning(&[38, 43, 48, 53, 57, 62]),
        Some(vec![26, 31, 36, 41])
    );
    assert_eq!(derive_bass_tuning(&[40, 45, 50, 55]), None);
}

#[test]
fn fretboard_optimizer_keeps_a_line_in_one_playable_hand_position() {
    let hits = [40, 41, 40]
        .into_iter()
        .map(|midi| BassHit {
            step: 0,
            midi,
            velocity: 90,
            accent: false,
            fill: false,
            duration_steps: 1,
        })
        .collect::<Vec<_>>();
    let tuning = [28, 33, 38, 43];
    let positions = optimize_bass_positions(&hits, &tuning, 12).unwrap();

    assert!(positions.iter().all(|position| position.fret <= 12));
    assert_eq!(positions[0].string, positions[1].string);
    assert_eq!(positions[1].string, positions[2].string);
    for (hit, position) in hits.iter().zip(positions) {
        assert_eq!(
            tuning[position.string as usize] + position.fret as i32,
            i32::from(hit.midi)
        );
    }
}
