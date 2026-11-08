use super::*;

#[test]
fn standard_and_whole_step_down() {
    assert_eq!(
        describe(&[40, 45, 50, 55, 59, 64]).as_deref(),
        Some("E std")
    );
    assert_eq!(
        describe(&[38, 43, 48, 53, 57, 62]).as_deref(),
        Some("D std")
    );
    assert_eq!(
        describe(&[39, 44, 49, 54, 58, 63]).as_deref(),
        Some("Eb std")
    );
}

#[test]
fn drop_tunings_name_the_dropped_string() {
    assert_eq!(
        describe(&[38, 45, 50, 55, 59, 64]).as_deref(),
        Some("Drop D")
    );
    assert_eq!(
        describe(&[37, 44, 49, 54, 58, 63]).as_deref(),
        Some("Drop C#")
    );
}

#[test]
fn bass_and_seven_string_layouts() {
    assert_eq!(describe(&[28, 33, 38, 43]).as_deref(), Some("E std"));
    assert_eq!(describe(&[18, 25, 30, 35]).as_deref(), Some("Drop F#"));
    assert_eq!(
        describe(&[30, 37, 42, 47, 52, 56, 61]).as_deref(),
        Some("Drop F# 7str")
    );
}

#[test]
fn unrecognised_layouts_are_not_guessed() {
    assert_eq!(describe(&[40, 45, 50, 55, 60, 64]), None);
    assert_eq!(describe(&[0, 0, 0, 0, 0, 0]), None);
    assert_eq!(describe(&[]), None);
}

#[test]
fn score_label_skips_percussion_and_prefers_six_strings() {
    let mut doc = crate::fixtures::bass_line();
    assert_eq!(
        score_label(&doc).as_deref(),
        describe(&doc.tracks[0].tuning).as_deref()
    );
    doc.tracks[0].tuning = vec![0; 6];
    assert_eq!(score_label(&doc), None);
}

#[test]
fn unrecognised_tuning_is_labelled_custom_with_its_notes() {
    let mut doc = crate::fixtures::bass_line();
    doc.tracks[0].tuning = vec![33, 40, 45, 50, 55, 59];
    assert_eq!(score_label(&doc).as_deref(), Some("custom A E A D G B"));
    assert_eq!(notes(&[40, 45]), "E A");
}
