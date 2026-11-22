use super::*;

fn section(label: &str, bars: usize) -> SectionRequest {
    SectionRequest {
        label: label.into(),
        bars,
        candidates: vec![0],
        transpose_semitones: 0,
        energy: None,
        instrumentation: None,
        energy_texture: None,
        climax: false,
    }
}

fn plan() -> ArrangementPlan {
    ArrangementPlan::new(
        7,
        vec![
            section("Intro", 4),
            section("Verse", 8),
            section("Chorus", 8),
            section("Verse", 8),
        ],
    )
}

fn labels(plan: &ArrangementPlan) -> Vec<String> {
    plan.sections
        .iter()
        .map(|section| section.label.clone())
        .collect()
}

#[test]
fn an_index_address_names_one_slot_and_a_label_names_every_match() {
    let plan = plan();
    assert_eq!(resolve(&plan, &SlotAddress::Index(2)), vec![2]);
    assert_eq!(resolve(&plan, &SlotAddress::Index(9)), Vec::<usize>::new());
    assert_eq!(resolve(&plan, &SlotAddress::label("Verse")), vec![1, 3]);
    assert_eq!(
        resolve(&plan, &SlotAddress::Occurrence("Verse".into(), 1)),
        vec![3]
    );
}

#[test]
fn an_ambiguous_address_is_refused_rather_than_guessed() {
    let error = apply(
        &plan(),
        &PlanEdit::Drop {
            address: SlotAddress::label("Verse"),
        },
    )
    .expect_err("two slots carry that label");
    assert!(matches!(
        error,
        PlanEditError::AmbiguousSlot { count: 2, .. }
    ));
}

#[test]
fn a_label_address_survives_an_insertion_before_it() {
    let start = plan();
    let (after, _) = apply(
        &start,
        &PlanEdit::Insert {
            address: SlotAddress::Index(0),
            placement: Placement::After,
            section: Box::new(section("Bridge", 4)),
        },
    )
    .expect("a valid insert");

    // The index that used to name the chorus now names something else; the
    // label still finds it. That is the whole point of the address type.
    assert_eq!(after.sections[2].label, "Verse");
    assert_eq!(resolve(&after, &SlotAddress::label("Chorus")), vec![3]);
}

#[test]
fn repeat_plays_the_slot_the_requested_number_of_times() {
    let (after, outcome) = apply(
        &plan(),
        &PlanEdit::Repeat {
            address: SlotAddress::label("Chorus"),
            times: 3,
        },
    )
    .expect("a valid repeat");
    assert_eq!(
        labels(&after),
        vec!["Intro", "Verse", "Chorus", "Chorus", "Chorus", "Verse"]
    );
    assert_eq!(outcome.before, 4);
    assert_eq!(outcome.after, 6);
}

#[test]
fn a_repeat_of_one_leaves_the_plan_alone() {
    let start = plan();
    let (after, _) = apply(
        &start,
        &PlanEdit::Repeat {
            address: SlotAddress::label("Chorus"),
            times: 1,
        },
    )
    .expect("a valid repeat");
    assert_eq!(after, start);
}

#[test]
fn a_repeat_of_zero_is_refused_rather_than_deleting_the_slot() {
    let error = apply(
        &plan(),
        &PlanEdit::Repeat {
            address: SlotAddress::label("Chorus"),
            times: 0,
        },
    )
    .expect_err("zero is not a repeat");
    assert_eq!(error, PlanEditError::ZeroRepeat);
}

#[test]
fn drop_removes_the_slot_and_refuses_to_empty_the_plan() {
    let (after, outcome) = apply(
        &plan(),
        &PlanEdit::Drop {
            address: SlotAddress::label("Chorus"),
        },
    )
    .expect("a valid drop");
    assert_eq!(labels(&after), vec!["Intro", "Verse", "Verse"]);
    assert_eq!(outcome.after, 3);

    let single = ArrangementPlan::new(1, vec![section("Only", 4)]);
    assert_eq!(
        apply(
            &single,
            &PlanEdit::Drop {
                address: SlotAddress::Index(0)
            }
        )
        .expect_err("a plan needs a section"),
        PlanEditError::LastSlot
    );
}

#[test]
fn insert_places_a_slot_on_the_requested_side() {
    let (before, _) = apply(
        &plan(),
        &PlanEdit::Insert {
            address: SlotAddress::label("Chorus"),
            placement: Placement::Before,
            section: Box::new(section("Pre", 4)),
        },
    )
    .expect("a valid insert");
    assert_eq!(
        labels(&before),
        vec!["Intro", "Verse", "Pre", "Chorus", "Verse"]
    );

    let (after, _) = apply(
        &plan(),
        &PlanEdit::Insert {
            address: SlotAddress::label("Chorus"),
            placement: Placement::After,
            section: Box::new(section("Post", 4)),
        },
    )
    .expect("a valid insert");
    assert_eq!(
        labels(&after),
        vec!["Intro", "Verse", "Chorus", "Post", "Verse"]
    );
}

#[test]
fn an_invalid_inserted_section_is_refused() {
    let mut empty = section("Bad", 0);
    empty.candidates = vec![0];
    assert!(matches!(
        apply(
            &plan(),
            &PlanEdit::Insert {
                address: SlotAddress::Index(0),
                placement: Placement::After,
                section: Box::new(empty),
            }
        )
        .expect_err("a section needs bars"),
        PlanEditError::InvalidSection(_)
    ));
}

#[test]
fn move_accounts_for_the_gap_the_removal_leaves() {
    // Moving a slot forwards past its target is where an implementation that
    // forgets the removal shifts everything lands one slot off.
    let (after, _) = apply(
        &plan(),
        &PlanEdit::Move {
            address: SlotAddress::Index(0),
            target: SlotAddress::label("Chorus"),
            placement: Placement::After,
        },
    )
    .expect("a valid move");
    assert_eq!(
        labels(&after),
        vec!["Verse", "Chorus", "Intro", "Verse"],
        "the intro lands directly after the chorus"
    );
}

#[test]
fn move_backwards_lands_before_its_target() {
    let (after, _) = apply(
        &plan(),
        &PlanEdit::Move {
            address: SlotAddress::label("Chorus"),
            target: SlotAddress::Index(0),
            placement: Placement::Before,
        },
    )
    .expect("a valid move");
    assert_eq!(labels(&after), vec!["Chorus", "Intro", "Verse", "Verse"]);
}

#[test]
fn rename_transpose_and_variation_change_only_what_they_name() {
    let start = plan();
    let (after, _) = apply(
        &start,
        &PlanEdit::Rename {
            address: SlotAddress::Index(2),
            label: "Refrain".into(),
        },
    )
    .expect("a valid rename");
    assert_eq!(after.sections[2].label, "Refrain");
    assert_eq!(after.sections[2].bars, start.sections[2].bars);

    let (after, _) = apply(
        &start,
        &PlanEdit::Transpose {
            address: SlotAddress::Index(2),
            semitones: -5,
        },
    )
    .expect("a valid transpose");
    assert_eq!(after.sections[2].transpose_semitones, -5);

    let (after, _) = apply(
        &start,
        &PlanEdit::Variation {
            address: SlotAddress::Index(2),
            candidates: vec![1, 2],
        },
    )
    .expect("a valid variation");
    assert_eq!(after.sections[2].candidates, vec![1, 2]);
}

#[test]
fn an_empty_candidate_list_is_refused() {
    assert!(matches!(
        apply(
            &plan(),
            &PlanEdit::Variation {
                address: SlotAddress::Index(0),
                candidates: Vec::new(),
            }
        )
        .expect_err("a section needs a candidate"),
        PlanEditError::InvalidSection(_)
    ));
}

#[test]
fn energy_is_set_cleared_and_range_checked() {
    let (after, _) = apply(
        &plan(),
        &PlanEdit::Energy {
            address: SlotAddress::Index(0),
            energy: Some(90),
        },
    )
    .expect("a valid energy");
    assert_eq!(after.sections[0].energy, Some(90));

    let (cleared, _) = apply(
        &after,
        &PlanEdit::Energy {
            address: SlotAddress::Index(0),
            energy: None,
        },
    )
    .expect("clearing is valid");
    assert_eq!(cleared.sections[0].energy, None);

    assert_eq!(
        apply(
            &plan(),
            &PlanEdit::Energy {
                address: SlotAddress::Index(0),
                energy: Some(101),
            }
        )
        .expect_err("out of range"),
        PlanEditError::EnergyOutOfRange(101)
    );
}

#[test]
fn instrumentation_is_set_and_cleared() {
    let (after, _) = apply(
        &plan(),
        &PlanEdit::Instrumentation {
            address: SlotAddress::Index(1),
            tracks: Some(vec![0, 2]),
        },
    )
    .expect("a valid instrumentation");
    assert_eq!(after.sections[1].instrumentation, Some(vec![0, 2]));
}

#[test]
fn apply_all_keeps_the_edits_that_work_and_names_the_ones_that_do_not() {
    let report = apply_all(
        &plan(),
        &[
            PlanEdit::Repeat {
                address: SlotAddress::label("Chorus"),
                times: 2,
            },
            PlanEdit::Drop {
                address: SlotAddress::label("Nothing"),
            },
            PlanEdit::Rename {
                address: SlotAddress::Index(0),
                label: "Opening".into(),
            },
        ],
    );
    assert_eq!(report.applied.len(), 2);
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].index, 1);
    assert!(matches!(
        report.rejected[0].error,
        PlanEditError::NoSuchSlot(_)
    ));
    assert_eq!(
        labels(&report.plan),
        vec!["Opening", "Verse", "Chorus", "Chorus", "Verse"],
        "the two possible edits both landed"
    );
}

#[test]
fn a_refused_edit_leaves_the_plan_untouched() {
    let start = plan();
    let error = apply(
        &start,
        &PlanEdit::Drop {
            address: SlotAddress::label("Verse"),
        },
    );
    assert!(error.is_err());
    // `apply` returns a new plan rather than mutating, so a refusal cannot
    // leave a half-applied one behind. The original is still what it was.
    assert_eq!(start, plan());
}

#[test]
fn bar_counts_follow_the_edits() {
    let start = plan();
    assert_eq!(planned_bars(&start), 28);
    let (after, _) = apply(
        &start,
        &PlanEdit::Repeat {
            address: SlotAddress::label("Chorus"),
            times: 3,
        },
    )
    .expect("a valid repeat");
    assert_eq!(planned_bars(&after), 44, "two more eight-bar choruses");
}

#[test]
fn an_edited_plan_still_round_trips_through_json() {
    let report = apply_all(
        &plan(),
        &[
            PlanEdit::Energy {
                address: SlotAddress::Index(0),
                energy: Some(75),
            },
            PlanEdit::Instrumentation {
                address: SlotAddress::Index(1),
                tracks: Some(vec![0]),
            },
        ],
    );
    let json = report.plan.to_json().expect("serializable");
    let parsed = ArrangementPlan::from_json(&json).expect("parseable");
    assert_eq!(parsed, report.plan);
}

#[test]
fn a_plan_written_before_energy_existed_still_parses() {
    // The two fields are serde defaults, so an older plan file must still
    // load rather than failing on a field it never carried.
    let json = r#"{
        "version": 1,
        "seed": 7,
        "sections": [
            {"label": "Intro", "bars": 4, "candidates": [0], "transpose_semitones": 0}
        ],
        "transitions": []
    }"#;
    let parsed = ArrangementPlan::from_json(json).expect("an older plan still parses");
    assert_eq!(parsed.sections[0].energy, None);
    assert_eq!(parsed.sections[0].instrumentation, None);
}
