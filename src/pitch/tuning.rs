//! Named standard guitar and bass tunings, stored from the lowest string up.
//!
//! This module owns the preset catalog, case-insensitive name resolution,
//! reverse lookup, and the target type accepted by retune operations.

use std::borrow::Cow;
use std::fmt;

/// A named instrument tuning, with pitches ordered from the lowest string up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuningPreset {
    /// Canonical display and resolution name.
    pub name: &'static str,
    /// Open-string MIDI pitches, in `Track::tuning` order.
    pub pitches: &'static [i32],
}

/// Built-in standard guitar and bass tuning presets.
pub const TUNING_PRESETS: [TuningPreset; 14] = [
    TuningPreset {
        name: "E Standard",
        pitches: &[40, 45, 50, 55, 59, 64],
    },
    TuningPreset {
        name: "Eb Standard",
        pitches: &[39, 44, 49, 54, 58, 63],
    },
    TuningPreset {
        name: "D Standard",
        pitches: &[38, 43, 48, 53, 57, 62],
    },
    TuningPreset {
        name: "C# Standard",
        pitches: &[37, 42, 47, 52, 56, 61],
    },
    TuningPreset {
        name: "Drop D",
        pitches: &[38, 45, 50, 55, 59, 64],
    },
    TuningPreset {
        name: "Drop C",
        pitches: &[36, 43, 48, 53, 57, 62],
    },
    TuningPreset {
        name: "Drop B",
        pitches: &[35, 42, 47, 52, 56, 61],
    },
    TuningPreset {
        name: "Drop A",
        pitches: &[33, 40, 45, 50, 54, 59],
    },
    TuningPreset {
        name: "7-String Standard",
        pitches: &[35, 40, 45, 50, 55, 59, 64],
    },
    TuningPreset {
        name: "7-String Drop A",
        pitches: &[33, 40, 45, 50, 55, 59, 64],
    },
    TuningPreset {
        name: "4-String Bass Standard",
        pitches: &[28, 33, 38, 43],
    },
    TuningPreset {
        name: "4-String Bass Drop D",
        pitches: &[26, 33, 38, 43],
    },
    TuningPreset {
        name: "5-String Bass Standard",
        pitches: &[23, 28, 33, 38, 43],
    },
    TuningPreset {
        name: "5-String Bass Drop A",
        pitches: &[21, 28, 33, 38, 43],
    },
];

/// A tuning supplied directly as pitches or resolved from a preset name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchTarget<'a> {
    /// MIDI open-string pitches ordered lowest string first.
    Pitches(&'a [i32]),
    /// A case-insensitive name from [`TUNING_PRESETS`].
    Preset(&'a str),
}

impl<'a> From<&'a [i32]> for PitchTarget<'a> {
    fn from(pitches: &'a [i32]) -> Self {
        Self::Pitches(pitches)
    }
}

impl<'a, const N: usize> From<&'a [i32; N]> for PitchTarget<'a> {
    fn from(pitches: &'a [i32; N]) -> Self {
        Self::Pitches(pitches)
    }
}

impl<'a> From<&'a Vec<i32>> for PitchTarget<'a> {
    fn from(pitches: &'a Vec<i32>) -> Self {
        Self::Pitches(pitches)
    }
}

impl<'a> From<&'a str> for PitchTarget<'a> {
    fn from(name: &'a str) -> Self {
        Self::Preset(name)
    }
}

impl<'a> From<&'a String> for PitchTarget<'a> {
    fn from(name: &'a String) -> Self {
        Self::Preset(name)
    }
}

/// Error returned when a preset name cannot be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuningPresetError {
    /// Name supplied by the caller.
    pub requested: String,
    /// All accepted preset names, in catalog order.
    pub available: Vec<&'static str>,
}

impl fmt::Display for TuningPresetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown tuning preset {:?}; available presets: {}",
            self.requested,
            self.available.join(", ")
        )
    }
}

impl std::error::Error for TuningPresetError {}

/// Resolves a preset name without regard to letter case or surrounding spaces.
pub fn resolve_tuning_preset(name: &str) -> Result<&'static TuningPreset, TuningPresetError> {
    TUNING_PRESETS
        .iter()
        .find(|preset| preset.name.eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| TuningPresetError {
            requested: name.to_string(),
            available: TUNING_PRESETS.iter().map(|preset| preset.name).collect(),
        })
}

/// Returns the canonical preset name for an exact lowest-string-first tuning.
pub fn preset_name_for_tuning(pitches: &[i32]) -> Option<&'static str> {
    TUNING_PRESETS
        .iter()
        .find(|preset| preset.pitches == pitches)
        .map(|preset| preset.name)
}

pub(super) fn resolve_pitch_target(
    target: PitchTarget<'_>,
) -> Result<Cow<'_, [i32]>, TuningPresetError> {
    match target {
        PitchTarget::Pitches(pitches) => Ok(Cow::Borrowed(pitches)),
        PitchTarget::Preset(name) => Ok(Cow::Borrowed(resolve_tuning_preset(name)?.pitches)),
    }
}
