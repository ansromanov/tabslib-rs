//! Song-form templates and deterministic section-slot planning.
//!
//! This module owns [`FormTemplate`] and [`plan_form`], which expand familiar
//! section orders into editable [`SectionRequest`] values.

use serde::{Deserialize, Serialize};

use super::arrangement::{ArrangementError, SectionRequest};

/// Built-in song-form section orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormTemplate {
    /// Verse, chorus, verse, chorus, bridge, chorus.
    VerseChorusBridge,
    /// Intro, verse, chorus, verse, chorus, bridge, chorus, outro.
    IntroVerseChorusBridge,
    /// Verse, chorus, repeated once.
    VerseChorus,
}

impl FormTemplate {
    /// Returns the ordered labels used by this template.
    pub fn labels(self) -> &'static [&'static str] {
        match self {
            Self::VerseChorusBridge => &["Verse", "Chorus", "Verse", "Chorus", "Bridge", "Chorus"],
            Self::IntroVerseChorusBridge => &[
                "Intro", "Verse", "Chorus", "Verse", "Chorus", "Bridge", "Chorus", "Outro",
            ],
            Self::VerseChorus => &["Verse", "Chorus", "Verse", "Chorus"],
        }
    }
}

/// Expands a form template into section requests sharing bar length and candidates.
///
/// The returned requests are ordinary editable plan slots; callers can alter
/// individual lengths, candidate indexes, labels, or transpositions before assembly.
pub fn plan_form(
    template: FormTemplate,
    bars_per_section: usize,
    candidates: &[usize],
    transpose_semitones: i32,
) -> Result<Vec<SectionRequest>, ArrangementError> {
    if bars_per_section == 0 || candidates.is_empty() {
        return Err(ArrangementError::InvalidInput(
            "form slots need a positive bar count and at least one candidate".into(),
        ));
    }
    Ok(template
        .labels()
        .iter()
        .map(|label| SectionRequest {
            label: (*label).to_string(),
            bars: bars_per_section,
            candidates: candidates.to_vec(),
            transpose_semitones,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        })
        .collect())
}

#[cfg(test)]
#[path = "forms_test.rs"]
mod tests;
