//! Caller-editable drum profiles and performance shaping utilities.

use super::performance::DrumProfile;
use super::{voices, DrumHit};

/// Section-level drum feel presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum SectionDrumFeel {
    /// Snare on every grouped backbeat.
    FullBackbeat,
    /// Snare on every second grouped backbeat.
    HalfBackbeat,
    /// Snare on every fourth grouped backbeat.
    QuarterBackbeat,
    /// Alternate kick and snare on successive grouped pulses.
    AlternatingKickSnare,
    /// Kick follows source onsets while a cymbal keeps every grid step.
    LockedKickSteadyCymbal,
}

impl SectionDrumFeel {
    /// Stable name recorded in generation logs.
    pub const fn name(self) -> &'static str {
        match self {
            Self::FullBackbeat => "full_backbeat",
            Self::HalfBackbeat => "half_backbeat",
            Self::QuarterBackbeat => "quarter_backbeat",
            Self::AlternatingKickSnare => "alternating_kick_snare",
            Self::LockedKickSteadyCymbal => "locked_kick_steady_cymbal",
        }
    }

    /// Base profile for this feel. Section placement is applied separately.
    pub fn profile(self) -> DrumProfile {
        let mut profile = rock_straight_preset();
        profile.snare_probability = 0;
        match self {
            Self::FullBackbeat | Self::HalfBackbeat | Self::QuarterBackbeat => {
                profile.kick_probability = 55;
            }
            Self::AlternatingKickSnare | Self::LockedKickSteadyCymbal => {
                profile.kick_probability = 0;
            }
        }
        if matches!(self, Self::LockedKickSteadyCymbal) {
            profile.cymbal_probability = 0;
        }
        profile
    }
}

/// A generic straight groove profile with all layers enabled.
pub fn rock_straight_preset() -> DrumProfile {
    DrumProfile {
        kick_voice: voices::ACOUSTIC_BASS_DRUM,
        snare_voice: voices::ACOUSTIC_SNARE,
        cymbal_voice: voices::CLOSED_HI_HAT,
        kick_probability: 80,
        snare_probability: 70,
        cymbal_probability: 90,
    }
}

/// A sparse half-time profile with a backbeat on the third pulse.
pub fn half_time_groove_preset() -> DrumProfile {
    DrumProfile {
        kick_voice: voices::ACOUSTIC_BASS_DRUM,
        snare_voice: voices::ACOUSTIC_SNARE,
        cymbal_voice: voices::CLOSED_HI_HAT,
        kick_probability: 65,
        snare_probability: 45,
        cymbal_probability: 75,
    }
}

/// Resolves a caller-facing preset name without embedding genre-specific data.
pub fn resolve_auto_preset(name: &str) -> Option<DrumProfile> {
    match name.to_ascii_lowercase().as_str() {
        "rock" | "straight" => Some(rock_straight_preset()),
        "half-time" | "half_time" | "halftime" => Some(half_time_groove_preset()),
        _ => None,
    }
}

/// Adds a cymbal ostinato at regular subdivision steps.
pub fn cymbal_ostinato(hits: &mut Vec<DrumHit>, steps: u32, voice: u8) {
    if steps == 0 {
        return;
    }
    let end = hits
        .iter()
        .map(|hit| hit.step)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let mut step = 0;
    while step < end {
        if !hits.iter().any(|hit| hit.step == step && hit.midi == voice) {
            hits.push(DrumHit::new(voice, step));
        }
        step = step.saturating_add(steps);
    }
}

/// Adds kicks at caller-selected steps.
pub fn riff_kicks(hits: &mut Vec<DrumHit>, steps: &[u32], voice: u8) {
    for step in steps {
        hits.push(DrumHit::new(voice, *step));
    }
}
/// Adds accented snares at caller-selected steps.
pub fn backbeat_snares(hits: &mut Vec<DrumHit>, steps: &[u32], voice: u8) {
    for step in steps {
        let mut hit = DrumHit::new(voice, *step);
        hit.accent = true;
        hits.push(hit);
    }
}

/// Shapes hit velocities according to accent and ghost flags.
pub fn shape_velocities(hits: &mut [DrumHit], base: u8) {
    for hit in hits {
        hit.velocity = if hit.ghost {
            base.saturating_sub(35).max(1)
        } else if hit.accent {
            base.saturating_add(15).min(127)
        } else {
            base
        };
    }
}
/// Promotes non-ghost backbeats on the supplied voice to accented hits.
pub fn unghost_backbeats(hits: &mut [DrumHit], voice: u8, steps: &[u32]) {
    for hit in hits {
        if hit.midi == voice && steps.contains(&hit.step) {
            hit.ghost = false;
            hit.accent = true;
        }
    }
}
/// Converts a zero-to-one-hundred intensity to a velocity multiplier.
pub fn intensity_velocity_factor(intensity: u8) -> f32 {
    f32::from(intensity.min(100)) / 100.0
}
/// Shifts every velocity by an explicit signed tier.
pub fn apply_explicit_velocity_tier_shift(hits: &mut [DrumHit], shift: i8) {
    for hit in hits {
        hit.velocity = i16::from(hit.velocity)
            .saturating_add(i16::from(shift))
            .clamp(1, 127) as u8;
    }
}

/// Applies deterministic alternating velocity variation to a hit stream.
pub fn apply_variations(hits: &mut [DrumHit], amount: u8) {
    for (index, hit) in hits.iter_mut().enumerate() {
        let delta = if index % 2 == 0 {
            i16::from(amount)
        } else {
            -i16::from(amount)
        };
        hit.velocity = i16::from(hit.velocity).saturating_add(delta).clamp(1, 127) as u8;
    }
}

#[cfg(test)]
#[path = "presets_test.rs"]
mod tests;
