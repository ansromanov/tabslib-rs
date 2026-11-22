use super::*;
use crate::generation::plan_edit::{Placement, PlanEdit, SlotAddress};

fn sample() -> (Document, Vec<Motif>, ArrangementPlan) {
    let source = crate::fixtures::repeated_frets();
    let motif = Motif {
        start_bar: 0,
        end_bar: 0,
        fingerprint: super::super::arrangement::fingerprint_bar(&source, 0)
            .expect("fixture bar has a fingerprint"),
    };
    let section = |label: &str| SectionRequest {
        label: label.into(),
        bars: 1,
        candidates: vec![0],
        transpose_semitones: 0,
        energy: None,
        instrumentation: None,
        energy_texture: None,
        climax: false,
    };
    let plan = ArrangementPlan::new(
        19,
        vec![section("Intro"), section("Verse"), section("Chorus")],
    );
    (source, vec![motif], plan)
}

#[test]
fn contours_use_caller_values_and_enforce_their_bounds() {
    let (source, motifs, plan) = sample();
    let shaped = apply_energy_contour(&plan, &[25, 64, 90]).expect("valid energy curve");
    assert_eq!(
        shaped
            .sections
            .iter()
            .map(|section| section.energy)
            .collect::<Vec<_>>(),
        vec![Some(25), Some(64), Some(90)],
        "each caller value must be written to its matching section"
    );
    assert_eq!(
        plan.sections[0].energy, None,
        "the input plan stays unchanged"
    );

    let arranged = compose_plan(&source, &motifs, &plan).expect("valid arrangement");
    let tempo = apply_tempo_contour(
        &arranged,
        &[1, 1, 1],
        &[90.0, 120.0, 150.0],
        TempoBounds {
            minimum_bpm: 80.0,
            maximum_bpm: 160.0,
        },
    )
    .expect("valid tempo curve");
    assert_eq!(
        tempo
            .tempo_map
            .iter()
            .map(|change| (change.at_bar, change.bpm))
            .collect::<Vec<_>>(),
        vec![(0, 90.0), (1, 120.0), (2, 150.0)],
        "tempo changes must land at each section boundary"
    );
    assert!(matches!(
        apply_energy_contour(&plan, &[20, 101, 90]),
        Err(ContourError::EnergyOutOfRange(101))
    ));
    assert!(matches!(
        apply_tempo_contour(
            &arranged,
            &[1, 1, 1],
            &[90.0, 170.0, 150.0],
            TempoBounds {
                minimum_bpm: 80.0,
                maximum_bpm: 160.0,
            }
        ),
        Err(ContourError::TempoOutOfBounds(170.0))
    ));
}

#[test]
fn plan_diff_names_changed_inserted_and_removed_positions() {
    let (_, _, before) = sample();
    let mut after = before.clone();
    after.sections[1].energy = Some(72);
    assert_eq!(diff_plan_sections(&before, &after), vec![1]);

    after.sections.push(after.sections[0].clone());
    assert_eq!(diff_plan_sections(&before, &after), vec![1, 3]);
    after.seed += 1;
    assert_eq!(diff_plan_sections(&before, &after), vec![0, 1, 2, 3]);
}

#[test]
fn incremental_rebuild_matches_full_build_after_each_plan_edit_verb() {
    let (source, motifs, before) = sample();
    let previous_document = compose_plan(&source, &motifs, &before).expect("initial build");
    let edits = vec![
        PlanEdit::Repeat {
            address: SlotAddress::Index(0),
            times: 2,
        },
        PlanEdit::Drop {
            address: SlotAddress::Index(2),
        },
        PlanEdit::Insert {
            address: SlotAddress::Index(0),
            placement: Placement::After,
            section: Box::new(SectionRequest {
                label: "Bridge".into(),
                bars: 1,
                candidates: vec![0],
                transpose_semitones: 0,
                energy: None,
                instrumentation: None,
                energy_texture: None,
                climax: false,
            }),
        },
        PlanEdit::Move {
            address: SlotAddress::Index(0),
            target: SlotAddress::Index(2),
            placement: Placement::After,
        },
        PlanEdit::Rename {
            address: SlotAddress::Index(0),
            label: "Opening".into(),
        },
        PlanEdit::Transpose {
            address: SlotAddress::Index(0),
            semitones: 2,
        },
        PlanEdit::Variation {
            address: SlotAddress::Index(0),
            candidates: vec![0],
        },
        PlanEdit::Energy {
            address: SlotAddress::Index(0),
            energy: Some(77),
        },
        PlanEdit::Instrumentation {
            address: SlotAddress::Index(0),
            tracks: Some(vec![0]),
        },
    ];

    for edit in edits {
        let (next_plan, _) =
            super::super::plan_edit::apply(&before, &edit).expect("test edit is valid");
        let incremental =
            compose_plan_incremental(&source, &motifs, &before, &previous_document, &next_plan)
                .expect("incremental build succeeds");
        let full = compose_plan(&source, &motifs, &next_plan).expect("full build succeeds");
        assert_eq!(
            incremental.document, full,
            "incremental output diverged after {edit:?}"
        );
        assert_eq!(
            incremental.changed_sections,
            diff_plan_sections(&before, &next_plan)
        );
        if matches!(edit, PlanEdit::Drop { .. }) {
            assert!(
                incremental.reused_bars > 0,
                "an unchanged section should reuse its prior bar data"
            );
        }
    }
}
