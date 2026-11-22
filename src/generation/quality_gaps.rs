//! Composition quality, horizontal playability, and drum-role helpers.
//!
//! The module owns small, deterministic reports used by callers before a
//! generated arrangement is released. It does not choose a style or repair a
//! score silently: callers receive the exact violations and decide whether to
//! regenerate or edit.

use crate::generation::assessment::{self, AssessmentReport};
use crate::generation::requirements::{self, Violation};
use crate::generation::sections::RiffStab;
use crate::model::Document;
use crate::percussion::DrumRole;

/// A hard-constraint and quality report for an already composed document.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositionQuality {
    /// Deterministic assessment measurements.
    pub assessment: AssessmentReport,
    /// Hard requirements the document violates.
    pub hard_constraint_violations: Vec<Violation>,
    /// Cross-bar pitch movements over the caller's limit.
    pub horizontal_motion_violations: Vec<HorizontalMotionViolation>,
}

/// One excessive cross-bar pitch movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HorizontalMotionViolation {
    /// Track containing the movement.
    pub track: usize,
    /// First bar in the movement.
    pub from_bar: usize,
    /// Second bar in the movement.
    pub to_bar: usize,
    /// Absolute semitone distance between representative pitches.
    pub semitones: i32,
}

/// Evaluates composition quality and all hard requirements together.
pub fn evaluate_arrangement(document: &Document, max_horizontal_motion: i32) -> CompositionQuality {
    CompositionQuality {
        assessment: assessment::assess(document),
        hard_constraint_violations: requirements::check(document),
        horizontal_motion_violations: find_horizontal_motion_violations(
            document,
            max_horizontal_motion,
        ),
    }
}

/// Returns only the hard requirements violated by a composed arrangement.
pub fn arrangement_hard_constraint_violations(document: &Document) -> Vec<Violation> {
    requirements::check(document)
}

/// Finds adjacent-bar representative pitches that move too far.
pub fn find_horizontal_motion_violations(
    document: &Document,
    max_semitones: i32,
) -> Vec<HorizontalMotionViolation> {
    let mut violations = Vec::new();
    for track in 0..document.tracks.len() {
        let pitches = (0..document.master_bars.len())
            .map(|bar| representative_pitch(document, track, bar))
            .collect::<Vec<_>>();
        for bar in 1..pitches.len() {
            let (Some(previous), Some(current)) = (pitches[bar - 1], pitches[bar]) else {
                continue;
            };
            let semitones = (current - previous).abs();
            if semitones > max_semitones {
                violations.push(HorizontalMotionViolation {
                    track,
                    from_bar: bar - 1,
                    to_bar: bar,
                    semitones,
                });
            }
        }
    }
    violations
}

fn representative_pitch(document: &Document, track: usize, master_bar: usize) -> Option<i32> {
    let id = u32::try_from(*document.master_bars.get(master_bar)?.bar_ids.get(track)?).ok()?;
    let bar = document.bars.iter().find(|bar| bar.id == id)?;
    bar.voices
        .iter()
        .flat_map(|voice| voice.beats.iter())
        .flat_map(|beat| beat.notes.iter())
        .find_map(|note| note.midi)
}

/// A drum role chosen for one detected riff stab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrumRoleDecision {
    /// Bar containing the stab.
    pub bar: usize,
    /// Grid step containing the stab.
    pub step: u32,
    /// Role that should catch the stab.
    pub role: DrumRole,
}

/// Plans deterministic drum roles for riff stabs.
pub fn plan_drum_roles(stabs: &[RiffStab]) -> Vec<DrumRoleDecision> {
    stabs
        .iter()
        .map(|stab| DrumRoleDecision {
            bar: stab.bar_index,
            step: stab.step,
            role: if stab.lowest_pitch.is_some_and(|pitch| pitch < 45) {
                DrumRole::Kick
            } else if stab.polyphony > 1 {
                DrumRole::Snare
            } else {
                DrumRole::HiHat
            },
        })
        .collect()
}

#[cfg(test)]
#[path = "quality_gaps_test.rs"]
mod tests;
