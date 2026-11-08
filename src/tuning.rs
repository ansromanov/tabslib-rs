//! Names a string tuning from its open-string pitches.
//!
//! Owns [`describe`] and [`score_label`]. Both read the written tuning only:
//! no genre, instrument database or corpus is consulted.

use crate::model::Document;

const NOTES: [&str; 12] = [
    "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];

/// Intervals between adjacent strings (low to high) of a tuning in fourths
/// with the usual major third, by string count.
fn standard_steps(strings: usize) -> Option<&'static [i32]> {
    match strings {
        4 => Some(&[5, 5, 5]),
        5 => Some(&[5, 5, 5, 5]),
        6 => Some(&[5, 5, 5, 4, 5]),
        7 => Some(&[5, 5, 5, 5, 4, 5]),
        8 => Some(&[5, 5, 5, 5, 5, 4, 5]),
        _ => None,
    }
}

/// Describes a tuning given as open-string pitches, lowest string first.
///
/// Returns `"E std"` for a standard layout, `"Drop D"` when only the lowest
/// string sits a fifth under its neighbour instead of a fourth, and `None` for
/// anything else (including percussion). Seven and eight string layouts get a
/// `7str` or `8str` suffix.
pub fn describe(tuning: &[i32]) -> Option<String> {
    let steps = standard_steps(tuning.len())?;
    let actual: Vec<i32> = tuning.windows(2).map(|w| w[1] - w[0]).collect();
    let drop = if actual == steps {
        false
    } else if actual.first() == Some(&7) && actual.get(1..) == steps.get(1..) {
        true
    } else {
        return None;
    };
    let root = NOTES.get(tuning.first()?.rem_euclid(12) as usize)?;
    let mut label = if drop {
        format!("Drop {root}")
    } else {
        format!("{root} std")
    };
    if tuning.len() >= 7 {
        label.push_str(&format!(" {}str", tuning.len()));
    }
    Some(label)
}

/// Spells open-string pitches as note names, lowest string first (`"A E A D"`).
pub fn notes(tuning: &[i32]) -> String {
    let name = |p: &i32| NOTES[p.rem_euclid(12) as usize];
    tuning.iter().map(name).collect::<Vec<_>>().join(" ")
}

/// Labels a score by its first fretted track with six or more strings, falling
/// back to the first fretted track of any size. A tuning [`describe`] does not
/// recognise is labelled `custom` followed by its [`notes`], never guessed.
pub fn score_label(doc: &Document) -> Option<String> {
    let fretted = |t: &&crate::model::Track| t.tuning.iter().any(|p| *p != 0);
    let pick = doc
        .tracks
        .iter()
        .filter(fretted)
        .find(|t| t.tuning.len() >= 6)
        .or_else(|| doc.tracks.iter().find(fretted))?;
    Some(describe(&pick.tuning).unwrap_or_else(|| format!("custom {}", notes(&pick.tuning))))
}

#[cfg(test)]
#[path = "tuning_test.rs"]
mod tests;
