//! Note names for MIDI pitches and tablature string labels.
//!
//! Owns [`pitch_class_name`], [`note_name`], [`string_labels`] and
//! [`tuning_name`]. Names use sharps and scientific octaves, where MIDI 60 is
//! `C4`, so a label never depends on a key or a genre convention.

use super::tuning::preset_name_for_tuning;

/// Sharp-spelled name of a pitch class: `pitch_class_name(61)` is `"C#"`.
pub fn pitch_class_name(midi: i32) -> &'static str {
    match midi.rem_euclid(12) {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        // `rem_euclid(12)` leaves only 11 here.
        _ => "B",
    }
}

/// Name with octave, where MIDI 60 is `C4` and MIDI 40 is `E2`.
pub fn note_name(midi: i32) -> String {
    format!("{}{}", pitch_class_name(midi), midi.div_euclid(12) - 1)
}

/// One label per string, **highest-pitched string first**, as tablature is read.
///
/// `tuning` is lowest string first, as [`crate::model::Track::tuning`] stores it.
/// Labels are pitch-class names; when the top string shares its pitch class
/// with a lower string it is lower-cased, so standard tuning reads
/// `e B G D A E` and the two E strings stay distinguishable.
///
/// ```
/// use tabslib::pitch::string_labels;
///
/// assert_eq!(string_labels(&[40, 45, 50, 55, 59, 64]), ["e", "B", "G", "D", "A", "E"]);
/// assert_eq!(string_labels(&[36, 43, 48, 53, 57, 62]), ["D", "A", "F", "C", "G", "C"]);
/// ```
pub fn string_labels(tuning: &[i32]) -> Vec<String> {
    let mut labels: Vec<String> = tuning
        .iter()
        .rev()
        .map(|pitch| pitch_class_name(*pitch).to_string())
        .collect();
    let repeats_lower = tuning.split_last().is_some_and(|(top, lower)| {
        lower
            .iter()
            .any(|pitch| pitch.rem_euclid(12) == top.rem_euclid(12))
    });
    if repeats_lower {
        if let Some(top) = labels.first_mut() {
            *top = top.to_ascii_lowercase();
        }
    }
    labels
}

/// Display name of a tuning: its preset name when it matches one exactly,
/// otherwise its note names from the lowest string up, such as `C G C F A D`.
pub fn tuning_name(tuning: &[i32]) -> String {
    match preset_name_for_tuning(tuning) {
        Some(name) => name.to_string(),
        None => tuning
            .iter()
            .map(|pitch| pitch_class_name(*pitch))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

#[cfg(test)]
#[path = "names_test.rs"]
mod tests;
