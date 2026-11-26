//! Named, style-neutral solo scale presets and lead placement helpers.

use crate::generation::solo::{generate_solo, GeneratedSolo, SoloConfig};
use crate::model::Document;
use crate::{Error, Result};

/// A caller-selectable scale vocabulary for melodic generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoloPreset {
    /// Major pentatonic: 1, 2, 3, 5, 6.
    Pentatonic,
    /// Minor pentatonic with the blue fifth added.
    Blues,
    /// Seven-note diatonic major scale.
    Diatonic,
    /// Root, third, fifth, and seventh.
    Arpeggio,
}

/// Lists every built-in, style-neutral solo preset.
pub fn list_solo_presets() -> &'static [SoloPreset] {
    &[
        SoloPreset::Pentatonic,
        SoloPreset::Blues,
        SoloPreset::Diatonic,
        SoloPreset::Arpeggio,
    ]
}

/// Returns scale intervals for a named preset.
pub fn get_solo_preset(preset: SoloPreset) -> &'static [u8] {
    match preset {
        SoloPreset::Pentatonic => &[0, 2, 4, 7, 9],
        SoloPreset::Blues => &[0, 3, 5, 6, 7, 10],
        SoloPreset::Diatonic => &[0, 2, 4, 5, 7, 9, 11],
        SoloPreset::Arpeggio => &[0, 4, 7, 11],
    }
}

/// Returns the preset intervals under the explicit names used by callers.
pub fn pentatonic_scale_steps() -> &'static [u8] {
    get_solo_preset(SoloPreset::Pentatonic)
}
/// Returns the blues scale intervals.
pub fn blues_scale_steps() -> &'static [u8] {
    get_solo_preset(SoloPreset::Blues)
}
/// Returns the diatonic scale intervals.
pub fn diatonic_scale_steps() -> &'static [u8] {
    get_solo_preset(SoloPreset::Diatonic)
}
/// Returns the arpeggio intervals.
pub fn arpeggio_scale_steps() -> &'static [u8] {
    get_solo_preset(SoloPreset::Arpeggio)
}

/// Maps a normalized lead intensity to a MIDI velocity.
pub fn calculate_lead_beat_dynamics(intensity: u8) -> u8 {
    u8::try_from(u16::from(intensity.min(100)) * 127 / 100).unwrap_or(127)
}

/// A generated solo population member, retaining its seed and result.
#[derive(Debug, Clone, PartialEq)]
pub struct SoloPopulationCandidate {
    /// Seed used for this candidate.
    pub seed: u64,
    /// Generated score.
    pub document: Document,
    /// Generation diagnostics.
    pub generated: GeneratedSolo,
}

/// Generates seeded solo candidates without imposing a musical ranking.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `count` is zero. Otherwise returns the first error from
/// [`generate_solo`] for any candidate.
pub fn generate_solo_population(
    source: &Document,
    config: &SoloConfig,
    count: usize,
) -> Result<Vec<SoloPopulationCandidate>> {
    if count == 0 {
        return Err(Error::Malformed(
            "solo population needs at least one candidate".into(),
        ));
    }
    (0..count)
        .map(|index| {
            let mut request = config.clone();
            request.seed = config
                .seed
                .wrapping_add(u64::try_from(index).unwrap_or(u64::MAX));
            let mut document = source.clone();
            let generated = generate_solo(&mut document, &request)?;
            Ok(SoloPopulationCandidate {
                seed: request.seed,
                document,
                generated,
            })
        })
        .collect()
}

/// Inclusive lead span in score bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeadSpan {
    /// First bar occupied by the lead.
    pub start_bar: usize,
    /// Last bar occupied by the lead.
    pub end_bar: usize,
}

/// Plans lead spans from arrangement sections, preserving section order.
pub fn plan_solo_spans(bars: &[usize]) -> Vec<LeadSpan> {
    let mut cursor: usize = 0;
    bars.iter()
        .filter_map(|count| {
            let start = cursor;
            cursor = cursor.saturating_add(*count);
            (*count > 0).then_some(LeadSpan {
                start_bar: start,
                end_bar: cursor - 1,
            })
        })
        .collect()
}

/// Alias used by song-layer callers.
pub fn plan_lead_spans_from_plan(bars: &[usize]) -> Vec<LeadSpan> {
    plan_solo_spans(bars)
}

#[cfg(test)]
#[path = "solo_presets_test.rs"]
mod tests;
