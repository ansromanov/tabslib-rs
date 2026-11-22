use super::*;
use crate::fixtures;

#[test]
fn observation_preserves_rhythm_and_technique_counts() {
    let document = fixtures::repeated_frets();
    let observation = observe(&document);
    assert_eq!(observation.structure.notes, document.note_count());
    assert!(observation.duration_histogram.values().sum::<usize>() > 0);
    assert!(!observation.technique_counts.is_empty());
}

#[test]
fn observation_json_has_stable_exported_shape() {
    let json = to_json(&fixtures::repeated_frets()).expect("fixture serializes");
    assert!(json.contains("\"durationHistogram\""));
    assert!(json.contains("\"notesPerBar\""));
    assert!(!json.contains("source"));
}
