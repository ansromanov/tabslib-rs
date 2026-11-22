//! Rhythmic and melodic bar texture classification and preservation checks.
//!
//! Classifies bars into structural texture categories (`Silent`, `Sustain`,
//! `Chug`, `Movement`) and verifies that generative transforms preserve the
//! intended texture category across bars.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::Bar;

/// Rhythmic and harmonic texture class for a single bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarTexture {
    /// The bar contains no sounding note attacks.
    Silent,
    /// Exactly one note attack in the bar.
    Sustain,
    /// Multiple attacks playing identical pitch and string positions.
    Chug,
    /// Multiple attacks with changing pitch or string movement.
    Movement,
}

/// Errors raised when texture classes are not preserved across a transformation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TextureError {
    /// The transformation changed the total number of bars.
    #[error("{label} changed bar count from {expected} to {actual}")]
    BarCountMismatch {
        /// Contextual label describing the transformation.
        label: String,
        /// Expected bar count before transformation.
        expected: usize,
        /// Bar count produced after transformation.
        actual: usize,
    },
    /// A specific bar's texture class was modified.
    #[error("{label} changed bar {bar} texture from {expected:?} to {actual:?}")]
    TextureMismatch {
        /// Contextual label describing the transformation.
        label: String,
        /// 1-based bar number.
        bar: usize,
        /// Texture class before transformation.
        expected: BarTexture,
        /// Texture class after transformation.
        actual: BarTexture,
    },
}

/// Classifies the texture category of a written bar.
///
/// Scans all sounding beats across voices. Zero attacks classify as `Silent`,
/// one attack as `Sustain`. Multiple attacks with identical string and fret
/// positions classify as `Chug`, while varying notes classify as `Movement`.
pub fn classify_bar_texture(bar: &Bar) -> BarTexture {
    let mut attack_signatures = Vec::new();

    for voice in &bar.voices {
        for beat in &voice.beats {
            if beat.notes.is_empty() {
                continue;
            }
            let mut notes: Vec<(u32, i32)> = beat
                .notes
                .iter()
                .filter_map(|n| {
                    if let (Some(string), Some(fret)) = (n.string, n.fret) {
                        Some((string, fret))
                    } else {
                        None
                    }
                })
                .collect();
            notes.sort_unstable();
            notes.dedup();
            attack_signatures.push(notes);
        }
    }

    if attack_signatures.is_empty() {
        return BarTexture::Silent;
    }
    if attack_signatures.len() == 1 {
        return BarTexture::Sustain;
    }

    let first = &attack_signatures[0];
    if attack_signatures.iter().all(|sig| sig == first) {
        BarTexture::Chug
    } else {
        BarTexture::Movement
    }
}

/// Asserts that texture classes are preserved bar-for-bar across a transformation.
///
/// Returns an error if the slice lengths differ or if any bar's texture changed.
pub fn assert_texture_class_preserved(
    before: &[BarTexture],
    after: &[BarTexture],
    label: &str,
) -> Result<(), TextureError> {
    if before.len() != after.len() {
        return Err(TextureError::BarCountMismatch {
            label: label.to_string(),
            expected: before.len(),
            actual: after.len(),
        });
    }

    for (index, (&b, &a)) in before.iter().zip(after.iter()).enumerate() {
        if b != a {
            return Err(TextureError::TextureMismatch {
                label: label.to_string(),
                bar: index + 1,
                expected: b,
                actual: a,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "texture_test.rs"]
mod tests;
