use super::*;

#[test]
fn note_names_use_scientific_octaves() {
    assert_eq!(note_name(60), "C4");
    assert_eq!(note_name(40), "E2");
    assert_eq!(note_name(37), "C#2");
    assert_eq!(note_name(-1), "B-2");
}

#[test]
fn string_labels_put_the_highest_string_first() {
    // Lowest-first input; a reversed label list would start with the low E.
    assert_eq!(
        string_labels(&[40, 45, 50, 55, 59, 64]),
        ["e", "B", "G", "D", "A", "E"]
    );
}

#[test]
fn only_a_top_string_repeating_a_lower_pitch_class_is_lower_cased() {
    // Drop C: C appears twice but not on the top string.
    assert_eq!(
        string_labels(&[36, 43, 48, 53, 57, 62]),
        ["D", "A", "F", "C", "G", "C"]
    );
    // C# standard: the top string repeats the lowest.
    assert_eq!(
        string_labels(&[37, 42, 47, 52, 56, 61]),
        ["c#", "G#", "E", "B", "F#", "C#"]
    );
    // Seven-string: the top E repeats the sixth string, not the lowest B.
    assert_eq!(
        string_labels(&[35, 40, 45, 50, 55, 59, 64]),
        ["e", "B", "G", "D", "A", "E", "B"]
    );
    assert_eq!(string_labels(&[28, 33, 38, 43]), ["G", "D", "A", "E"]);
    assert!(string_labels(&[]).is_empty());
}

#[test]
fn tuning_name_prefers_the_preset_name() {
    assert_eq!(tuning_name(&[40, 45, 50, 55, 59, 64]), "E Standard");
    assert_eq!(tuning_name(&[36, 43, 48, 53, 57, 62]), "Drop C");
    assert_eq!(tuning_name(&[36, 43, 48, 53, 57, 63]), "C G C F A D#");
}
