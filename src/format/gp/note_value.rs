//! Note-value spellings for this format.
//!
//! The file writes `16th`, not `Sixteenth`, and accepts both on read. That
//! asymmetry is a property of the format, so it lives here rather than on
//! [`NoteValue`], which describes music and not files.

use crate::model::NoteValue;

/// Parses a written note value, accepting either spelling.
pub(crate) fn parse(s: &str) -> Option<NoteValue> {
    Some(match s {
        "Whole" => NoteValue::Whole,
        "Half" => NoteValue::Half,
        "Quarter" => NoteValue::Quarter,
        "Eighth" => NoteValue::Eighth,
        "Sixteenth" | "16th" => NoteValue::Sixteenth,
        "ThirtySecond" | "32nd" => NoteValue::ThirtySecond,
        "SixtyFourth" | "64th" => NoteValue::SixtyFourth,
        _ => return None,
    })
}

/// The single spelling this format writes.
///
/// Writing `Sixteenth` produces a file the originating application does not
/// read back the same way, so the writer must not choose freely here.
pub(crate) fn spell(value: NoteValue) -> &'static str {
    match value {
        NoteValue::Whole => "Whole",
        NoteValue::Half => "Half",
        NoteValue::Quarter => "Quarter",
        NoteValue::Eighth => "Eighth",
        NoteValue::Sixteenth => "16th",
        NoteValue::ThirtySecond => "32nd",
        NoteValue::SixtyFourth => "64th",
    }
}
