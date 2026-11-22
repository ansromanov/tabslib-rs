//! Applies caller-defined energy and tempo contours to an arrangement.
//!
//! The module owns [`apply_energy_contour`], [`apply_tempo_contour`],
//! [`diff_plan_sections`] and [`compose_plan_incremental`]. Curves are supplied
//! by callers; no style profile is built into the library.

use crate::model::{Document, TempoChange};

use super::arrangement::{compose_plan, ArrangementError, ArrangementPlan, Motif, SectionRequest};

/// Caller-supplied inclusive tempo bounds, in beats per minute.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoBounds {
    /// Lowest tempo allowed by the caller.
    pub minimum_bpm: f64,
    /// Highest tempo allowed by the caller.
    pub maximum_bpm: f64,
}

/// A failure applying an energy or tempo contour.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ContourError {
    /// The curve has a different number of values than the plan has sections.
    #[error("contour has {curve} values for {sections} sections")]
    Length {
        /// Number of values in the supplied curve.
        curve: usize,
        /// Number of sections in the plan.
        sections: usize,
    },
    /// An energy value is outside the supported 0..=100 range.
    #[error("energy {0} is outside 0..=100")]
    EnergyOutOfRange(u8),
    /// Tempo bounds must be finite, positive, and ordered.
    #[error("tempo bounds must be finite, positive, and ordered")]
    InvalidBounds,
    /// A tempo value is not finite, positive, or within the supplied bounds.
    #[error("tempo {0} BPM is outside the supplied bounds")]
    TempoOutOfBounds(f64),
    /// Section lengths must be nonzero and cover the document exactly.
    #[error("section lengths must be nonzero and cover all {bars} document bars")]
    InvalidSectionLengths {
        /// Number of bars in the document.
        bars: usize,
    },
}

/// Assigns section energy values from a caller-supplied contour.
///
/// The input plan is not mutated. Values map one-to-one to section order.
///
/// # Errors
///
/// Returns [`ContourError::Length`] if `energies` does not have one value per section,
/// and [`ContourError::EnergyOutOfRange`] if a value is above 100.
pub fn apply_energy_contour(
    plan: &ArrangementPlan,
    energies: &[u8],
) -> Result<ArrangementPlan, ContourError> {
    if energies.len() != plan.sections.len() {
        return Err(ContourError::Length {
            curve: energies.len(),
            sections: plan.sections.len(),
        });
    }
    if let Some(value) = energies.iter().copied().find(|value| *value > 100) {
        return Err(ContourError::EnergyOutOfRange(value));
    }
    let mut shaped = plan.clone();
    for (section, energy) in shaped.sections.iter_mut().zip(energies.iter().copied()) {
        section.energy = Some(energy);
    }
    Ok(shaped)
}

/// Writes a caller-supplied tempo contour at each section boundary.
///
/// `section_bars` and `tempos_bpm` map one-to-one and must cover the document.
/// Tempo changes inside the covered range are replaced by the contour, while
/// changes after the arrangement are preserved.
///
/// # Errors
///
/// Returns [`ContourError::InvalidBounds`] if `bounds` is not finite, positive and
/// ordered, [`ContourError::Length`] if `section_bars` and `tempos_bpm` differ in length,
/// and [`ContourError::TempoOutOfBounds`] if a tempo is not finite or outside `bounds`.
/// Returns [`ContourError::InvalidSectionLengths`] if a section has zero bars or the
/// sections do not cover the document exactly.
pub fn apply_tempo_contour(
    document: &Document,
    section_bars: &[usize],
    tempos_bpm: &[f64],
    bounds: TempoBounds,
) -> Result<Document, ContourError> {
    if !bounds.minimum_bpm.is_finite()
        || !bounds.maximum_bpm.is_finite()
        || bounds.minimum_bpm <= 0.0
        || bounds.minimum_bpm > bounds.maximum_bpm
    {
        return Err(ContourError::InvalidBounds);
    }
    if section_bars.len() != tempos_bpm.len() {
        return Err(ContourError::Length {
            curve: tempos_bpm.len(),
            sections: section_bars.len(),
        });
    }
    if tempos_bpm.iter().any(|tempo| {
        !tempo.is_finite() || *tempo < bounds.minimum_bpm || *tempo > bounds.maximum_bpm
    }) {
        let tempo = tempos_bpm
            .iter()
            .copied()
            .find(|tempo| {
                !tempo.is_finite() || *tempo < bounds.minimum_bpm || *tempo > bounds.maximum_bpm
            })
            .unwrap_or(f64::NAN);
        return Err(ContourError::TempoOutOfBounds(tempo));
    }
    let mut offset = 0usize;
    let mut starts = Vec::with_capacity(section_bars.len());
    for bars in section_bars {
        if *bars == 0 {
            return Err(ContourError::InvalidSectionLengths {
                bars: document.master_bars.len(),
            });
        }
        starts.push(offset);
        offset = offset.saturating_add(*bars);
    }
    if offset != document.master_bars.len() {
        return Err(ContourError::InvalidSectionLengths {
            bars: document.master_bars.len(),
        });
    }

    let mut output = document.clone();
    output.tempo_map.retain(|change| change.at_bar >= offset);
    output
        .tempo_map
        .extend(
            starts
                .into_iter()
                .zip(tempos_bpm.iter().copied())
                .map(|(at_bar, bpm)| TempoChange {
                    at_bar,
                    bpm,
                    linear: false,
                }),
        );
    output.tempo_map.sort_by_key(|change| change.at_bar);
    Ok(output)
}

/// Returns section indexes whose plan values changed, including inserted or
/// removed positions. A changed seed or transition list invalidates every
/// position because either can affect the assembled output globally.
pub fn diff_plan_sections(before: &ArrangementPlan, after: &ArrangementPlan) -> Vec<usize> {
    let count = before.sections.len().max(after.sections.len());
    let global_change = before.seed != after.seed || before.transitions != after.transitions;
    (0..count)
        .filter(|index| global_change || before.sections.get(*index) != after.sections.get(*index))
        .collect()
}

/// The result of rebuilding an arrangement after a plan change.
#[derive(Debug, Clone, PartialEq)]
pub struct IncrementalBuild {
    /// The document produced by the updated plan.
    pub document: Document,
    /// Changed section positions reported by [`diff_plan_sections`].
    pub changed_sections: Vec<usize>,
    /// Bars copied from the prior document because their rebuilt values match.
    pub reused_bars: usize,
}

/// Composes an updated plan and reuses unchanged section bars when identical.
///
/// The deterministic full result is computed first as a correctness oracle;
/// section data is then retained from `previous_document` wherever its plan
/// slot and rebuilt values are unchanged. This makes reuse conservative across
/// seed, transition, and preceding-section changes.
///
/// # Errors
///
/// Returns any error from [`compose_plan`] when composing `next_plan`.
pub fn compose_plan_incremental(
    source: &Document,
    motifs: &[Motif],
    previous_plan: &ArrangementPlan,
    previous_document: &Document,
    next_plan: &ArrangementPlan,
) -> Result<IncrementalBuild, ArrangementError> {
    let changed_sections = diff_plan_sections(previous_plan, next_plan);
    let mut document = compose_plan(source, motifs, next_plan)?;
    let mut reused_bars = 0;
    let mut old_start = 0usize;
    let mut new_start = 0usize;
    for (index, (old_section, new_section)) in previous_plan
        .sections
        .iter()
        .zip(next_plan.sections.iter())
        .enumerate()
    {
        let old_end = old_start.saturating_add(old_section.bars);
        let new_end = new_start.saturating_add(new_section.bars);
        if !changed_sections.contains(&index)
            && old_section == new_section
            && old_end <= previous_document.master_bars.len()
            && new_end <= document.master_bars.len()
            && previous_document.master_bars[old_start..old_end]
                == document.master_bars[new_start..new_end]
        {
            let old_bar_ids = previous_document.master_bars[old_start..old_end]
                .iter()
                .flat_map(|bar| bar.bar_ids.iter().copied())
                .collect::<Vec<_>>();
            let new_bar_ids = document.master_bars[new_start..new_end]
                .iter()
                .flat_map(|bar| bar.bar_ids.iter().copied())
                .collect::<Vec<_>>();
            let old_bars = old_bar_ids
                .iter()
                .filter_map(|id| {
                    previous_document
                        .bars
                        .iter()
                        .find(|bar| i64::from(bar.id) == i64::from(*id))
                })
                .collect::<Vec<_>>();
            let new_bars = new_bar_ids
                .iter()
                .filter_map(|id| {
                    document
                        .bars
                        .iter()
                        .find(|bar| i64::from(bar.id) == i64::from(*id))
                })
                .collect::<Vec<_>>();
            if old_bars == new_bars {
                for (old, new) in old_bar_ids.iter().zip(new_bar_ids.iter()) {
                    if let Some(previous) = previous_document
                        .bars
                        .iter()
                        .find(|bar| i64::from(bar.id) == i64::from(*old))
                    {
                        if let Some(current) = document
                            .bars
                            .iter_mut()
                            .find(|bar| i64::from(bar.id) == i64::from(*new))
                        {
                            *current = previous.clone();
                            reused_bars += 1;
                        }
                    }
                }
            }
        }
        old_start = old_end;
        new_start = new_end;
    }
    Ok(IncrementalBuild {
        document,
        changed_sections,
        reused_bars,
    })
}

/// Returns the bar indexes occupied by each section in a plan.
pub fn section_ranges(sections: &[SectionRequest]) -> Vec<std::ops::Range<usize>> {
    let mut start = 0;
    sections
        .iter()
        .map(|section| {
            let range = start..start + section.bars;
            start += section.bars;
            range
        })
        .collect()
}

#[cfg(test)]
#[path = "arrangement_dynamics_test.rs"]
mod tests;
