use super::*;
use crate::generation::arrangement::{ArrangementPlan, SectionRequest};
use crate::generation::plan_edit::{PlanEdit, SlotAddress};

fn make_plan() -> ArrangementPlan {
    ArrangementPlan::new(
        100,
        vec![
            SectionRequest {
                label: "intro".to_string(),
                bars: 4,
                candidates: vec![0],
                transpose_semitones: 0,
                energy: Some(40),
                instrumentation: None,
                energy_texture: None,
                climax: false,
            },
            SectionRequest {
                label: "verse".to_string(),
                bars: 8,
                candidates: vec![1],
                transpose_semitones: 0,
                energy: Some(60),
                instrumentation: None,
                energy_texture: None,
                climax: false,
            },
        ],
    )
}

#[test]
fn apply_intent_rejects_empty_intent() {
    let plan = make_plan();
    let req = ArrangementIntent {
        intent: "   ".to_string(),
        edits: vec![],
        timestamp: None,
    };
    let err = apply_intent(&plan, &req, &[]).expect_err("empty intent must be rejected");
    assert_eq!(err, IntentError::EmptyIntent);
}

#[test]
fn apply_intent_success_and_error_isolation() {
    let plan = make_plan();
    let req = ArrangementIntent {
        intent: "Double intro and adjust nonexistent section".to_string(),
        edits: vec![
            // Valid edit
            PlanEdit::Repeat {
                address: SlotAddress::label("intro"),
                times: 2,
            },
            // Invalid edit: nonexistent target
            PlanEdit::Drop {
                address: SlotAddress::label("chorus"),
            },
            // Valid subsequent edit: rename verse to verse_a
            PlanEdit::Rename {
                address: SlotAddress::label("verse"),
                label: "verse_a".to_string(),
            },
        ],
        timestamp: Some("2026-09-19T00:00:00Z".to_string()),
    };

    let result = apply_intent(&plan, &req, &[]).expect("apply_intent should succeed");

    // Edit 0 succeeded: intro repeated
    // Edit 1 failed: chorus doesn't exist
    // Edit 2 succeeded: verse renamed to verse_a
    assert_eq!(result.applied.len(), 2);
    assert_eq!(result.rejected.len(), 1);

    assert_eq!(result.rejected[0].index, 1);
    assert!(result.rejected[0].reason.contains("chorus"));

    assert_eq!(result.applied[0].index, 0);
    assert_eq!(result.applied[0].edit.verb, "repeat");
    assert_eq!(result.applied[1].index, 2);
    assert_eq!(result.applied[1].edit.verb, "rename");

    // Resulting plan has 3 sections: intro, intro, verse_a
    assert_eq!(result.plan.sections.len(), 3);
    assert_eq!(result.plan.sections[0].label, "intro");
    assert_eq!(result.plan.sections[1].label, "intro");
    assert_eq!(result.plan.sections[2].label, "verse_a");

    assert_eq!(result.edit_history.len(), 2);
    assert_eq!(
        result.edit_history[0].intent.as_deref(),
        Some(req.intent.as_str())
    );
}
