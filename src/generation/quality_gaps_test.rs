use super::*;
use crate::fixtures::repeated_frets;

#[test]
fn quality_report_exposes_hard_requirements_and_motion() {
    let document = repeated_frets();
    let report = evaluate_arrangement(&document, 100);
    assert!(!report.assessment.metrics.is_empty());
    assert!(report.hard_constraint_violations.is_empty());
    assert!(report.horizontal_motion_violations.is_empty());
}

#[test]
fn drum_role_planning_preserves_stab_locations() {
    let stabs = vec![RiffStab {
        bar_index: 2,
        step: 8,
        lowest_pitch: Some(36),
        polyphony: 2,
        track_names: vec!["riff".into()],
    }];
    assert_eq!(
        plan_drum_roles(&stabs),
        vec![DrumRoleDecision {
            bar: 2,
            step: 8,
            role: DrumRole::Kick,
        }]
    );
}

#[test]
fn stab_catches_add_accented_voices_without_replacing_hits() {
    let stabs = vec![RiffStab {
        bar_index: 0,
        step: 4,
        lowest_pitch: Some(40),
        polyphony: 1,
        track_names: vec!["riff".into()],
    }];
    let decisions = plan_drum_roles(&stabs);
    let mut hits = vec![crate::generation::drums::DrumHit::new(42, 4)];
    crate::generation::drums::apply_stab_catches(&mut hits, &decisions);
    assert!(hits.iter().any(|hit| hit.midi == 42 && hit.step == 4));
    assert!(hits.iter().any(|hit| {
        hit.midi == crate::generation::drums::voices::ACOUSTIC_BASS_DRUM
            && hit.step == 4
            && hit.accent
    }));
    assert!(hits
        .iter()
        .any(|hit| hit.midi == crate::generation::drums::voices::CHINESE_CYMBAL));
}
