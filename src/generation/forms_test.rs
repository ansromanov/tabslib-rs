use super::*;

#[test]
fn built_in_templates_expand_ordered_editable_slots() {
    let sections = plan_form(FormTemplate::IntroVerseChorusBridge, 4, &[0, 2], 1).unwrap();
    assert_eq!(
        sections
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        ["Intro", "Verse", "Chorus", "Verse", "Chorus", "Bridge", "Chorus", "Outro"]
    );
    assert!(sections
        .iter()
        .all(|item| item.bars == 4 && item.candidates == [0, 2]));
    let mut editable = sections[2].clone();
    editable.bars = 8;
    editable.candidates = vec![3];
    assert_eq!((editable.bars, editable.candidates), (8, vec![3]));
}

#[test]
fn planner_rejects_unfillable_slots() {
    assert!(plan_form(FormTemplate::VerseChorus, 0, &[0], 0).is_err());
    assert!(plan_form(FormTemplate::VerseChorus, 4, &[], 0).is_err());
}
