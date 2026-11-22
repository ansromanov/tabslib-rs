//! Editing an arrangement plan through addressed verbs.
//!
//! The module owns [`SlotAddress`], [`PlanEdit`], [`apply`] and [`apply_all`].
//! An [`ArrangementPlan`] was a one-shot
//! structure: build it, serialize it, render it. Changing one section meant
//! rebuilding the whole plan, which discards every other decision in it.
//!
//! These verbs change one slot and leave the rest alone. Each returns the new
//! plan and says what moved; an edit that cannot apply returns
//! [`PlanEditError`] and changes nothing, because a plan that silently ignored
//! half a request is worse than one that refused it.

use serde::{Deserialize, Serialize};

use super::arrangement::{ArrangementPlan, SectionRequest};

/// How an edit names the slot it acts on.
///
/// Positions shift under insertion and moves, so an address that has to
/// survive a sequence of edits should name a label rather than an index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotAddress {
    /// The slot at this zero-based position.
    Index(usize),
    /// The only slot carrying this label.
    Label(String),
    /// The nth slot carrying this label, counting from zero.
    Occurrence(String, usize),
}

impl SlotAddress {
    /// Addresses a slot by label.
    pub fn label(label: impl Into<String>) -> Self {
        Self::Label(label.into())
    }
}

/// Where an inserted slot goes relative to the addressed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Placement {
    /// Immediately before the addressed slot.
    Before,
    /// Immediately after the addressed slot.
    After,
}

/// One change to a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanEdit {
    /// Play the addressed slot `times` times in total.
    Repeat {
        /// Slot to repeat.
        address: SlotAddress,
        /// Total number of consecutive plays, at least one.
        times: usize,
    },
    /// Remove the addressed slot.
    Drop {
        /// Slot to remove.
        address: SlotAddress,
    },
    /// Add a slot next to the addressed one.
    Insert {
        /// Slot to insert beside.
        address: SlotAddress,
        /// Which side to insert on.
        placement: Placement,
        /// The new slot.
        section: Box<SectionRequest>,
    },
    /// Move the addressed slot next to another one.
    Move {
        /// Slot to move.
        address: SlotAddress,
        /// Slot to move it beside.
        target: SlotAddress,
        /// Which side of the target to land on.
        placement: Placement,
    },
    /// Set the addressed slot's label.
    Rename {
        /// Slot to rename.
        address: SlotAddress,
        /// New label.
        label: String,
    },
    /// Set the addressed slot's transposition.
    Transpose {
        /// Slot to transpose.
        address: SlotAddress,
        /// Semitones applied to every bar of the section.
        semitones: i32,
    },
    /// Set which motifs the addressed slot may draw from.
    Variation {
        /// Slot to change.
        address: SlotAddress,
        /// Candidate motif indexes, in preference order.
        candidates: Vec<usize>,
    },
    /// Set the addressed slot's energy, or clear it.
    Energy {
        /// Slot to change.
        address: SlotAddress,
        /// Energy from zero to one hundred, or `None` to leave dynamics alone.
        energy: Option<u8>,
    },
    /// Set which tracks sound in the addressed slot, or clear the restriction.
    Instrumentation {
        /// Slot to change.
        address: SlotAddress,
        /// Track indexes that sound, or `None` for every track.
        tracks: Option<Vec<usize>>,
    },
}

/// Why an edit could not be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanEditError {
    /// The address matched no slot.
    #[error("no slot matches {0}")]
    NoSuchSlot(String),
    /// The address matched more than one slot.
    #[error("{count} slots match {address}; name an occurrence to choose one")]
    AmbiguousSlot {
        /// The address that matched several slots.
        address: String,
        /// How many slots it matched.
        count: usize,
    },
    /// A repeat count of zero would delete the slot instead of repeating it.
    #[error("a repeat count must be at least one; use Drop to remove a slot")]
    ZeroRepeat,
    /// The edit would leave the plan with no sections.
    #[error("a plan needs at least one section")]
    LastSlot,
    /// An inserted or edited section is not itself valid.
    #[error("invalid section: {0}")]
    InvalidSection(String),
    /// An energy value outside the range the scale defines.
    #[error("energy {0} is outside 0..=100")]
    EnergyOutOfRange(u8),
}

/// What one edit did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditOutcome {
    /// Slot positions the edit touched, after it was applied.
    pub slots: Vec<usize>,
    /// Section count before the edit.
    pub before: usize,
    /// Section count after the edit.
    pub after: usize,
}

/// An edit that was refused, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedEdit {
    /// Position of the edit in the requested list.
    pub index: usize,
    /// Why it could not be applied.
    pub error: PlanEditError,
}

/// The result of applying a list of edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditReport {
    /// The plan after every applicable edit.
    pub plan: ArrangementPlan,
    /// Outcomes of the edits that applied, in request order.
    pub applied: Vec<EditOutcome>,
    /// Edits that were refused, in request order.
    pub rejected: Vec<RejectedEdit>,
}

/// Resolves an address to the slot positions it names.
///
/// A label address matches every slot carrying that label, which is what makes
/// it useful for finding sections and dangerous for editing them. [`apply`]
/// refuses an ambiguous address rather than picking one.
pub fn resolve(plan: &ArrangementPlan, address: &SlotAddress) -> Vec<usize> {
    match address {
        SlotAddress::Index(index) => {
            if *index < plan.sections.len() {
                vec![*index]
            } else {
                Vec::new()
            }
        }
        SlotAddress::Label(label) => plan
            .sections
            .iter()
            .enumerate()
            .filter(|(_, section)| section.label == *label)
            .map(|(index, _)| index)
            .collect(),
        SlotAddress::Occurrence(label, occurrence) => plan
            .sections
            .iter()
            .enumerate()
            .filter(|(_, section)| section.label == *label)
            .map(|(index, _)| index)
            .nth(*occurrence)
            .into_iter()
            .collect(),
    }
}

fn describe(address: &SlotAddress) -> String {
    match address {
        SlotAddress::Index(index) => format!("slot {index}"),
        SlotAddress::Label(label) => format!("label {label:?}"),
        SlotAddress::Occurrence(label, occurrence) => {
            format!("occurrence {occurrence} of label {label:?}")
        }
    }
}

/// Resolves an address to exactly one slot.
fn resolve_one(plan: &ArrangementPlan, address: &SlotAddress) -> Result<usize, PlanEditError> {
    let matches = resolve(plan, address);
    match matches.len() {
        0 => Err(PlanEditError::NoSuchSlot(describe(address))),
        1 => Ok(matches[0]),
        count => Err(PlanEditError::AmbiguousSlot {
            address: describe(address),
            count,
        }),
    }
}

fn check_section(section: &SectionRequest) -> Result<(), PlanEditError> {
    if section.bars == 0 {
        return Err(PlanEditError::InvalidSection(
            "a section needs at least one bar".into(),
        ));
    }
    if section.candidates.is_empty() {
        return Err(PlanEditError::InvalidSection(
            "a section needs at least one candidate motif".into(),
        ));
    }
    Ok(())
}

/// Applies one edit, returning the new plan and what it touched.
///
/// The plan is returned rather than mutated in place, so a refused edit cannot
/// leave a half-applied plan behind.
pub fn apply(
    plan: &ArrangementPlan,
    edit: &PlanEdit,
) -> Result<(ArrangementPlan, EditOutcome), PlanEditError> {
    let before = plan.sections.len();
    let mut next = plan.clone();
    let slots = match edit {
        PlanEdit::Repeat { address, times } => {
            if *times == 0 {
                return Err(PlanEditError::ZeroRepeat);
            }
            let index = resolve_one(plan, address)?;
            let section = next.sections[index].clone();
            let existing = 1;
            for offset in existing..*times {
                next.sections.insert(index + offset, section.clone());
            }
            (index..index + times).collect()
        }
        PlanEdit::Drop { address } => {
            let index = resolve_one(plan, address)?;
            if next.sections.len() == 1 {
                return Err(PlanEditError::LastSlot);
            }
            next.sections.remove(index);
            vec![index.min(next.sections.len().saturating_sub(1))]
        }
        PlanEdit::Insert {
            address,
            placement,
            section,
        } => {
            check_section(section)?;
            let index = resolve_one(plan, address)?;
            let at = match placement {
                Placement::Before => index,
                Placement::After => index + 1,
            };
            next.sections.insert(at, section.as_ref().clone());
            vec![at]
        }
        PlanEdit::Move {
            address,
            target,
            placement,
        } => {
            let from = resolve_one(plan, address)?;
            let to = resolve_one(plan, target)?;
            let section = next.sections.remove(from);
            // Removing the slot shifts everything after it down by one, so a
            // target past the removal point has already moved.
            let adjusted = if to > from { to - 1 } else { to };
            let at = match placement {
                Placement::Before => adjusted,
                Placement::After => adjusted + 1,
            };
            let at = at.min(next.sections.len());
            next.sections.insert(at, section);
            vec![at]
        }
        PlanEdit::Rename { address, label } => {
            let index = resolve_one(plan, address)?;
            next.sections[index].label = label.clone();
            vec![index]
        }
        PlanEdit::Transpose { address, semitones } => {
            let index = resolve_one(plan, address)?;
            next.sections[index].transpose_semitones = *semitones;
            vec![index]
        }
        PlanEdit::Variation {
            address,
            candidates,
        } => {
            let index = resolve_one(plan, address)?;
            let mut section = next.sections[index].clone();
            section.candidates = candidates.clone();
            check_section(&section)?;
            next.sections[index] = section;
            vec![index]
        }
        PlanEdit::Energy { address, energy } => {
            if let Some(value) = energy {
                if *value > 100 {
                    return Err(PlanEditError::EnergyOutOfRange(*value));
                }
            }
            let index = resolve_one(plan, address)?;
            next.sections[index].energy = *energy;
            vec![index]
        }
        PlanEdit::Instrumentation { address, tracks } => {
            let index = resolve_one(plan, address)?;
            next.sections[index].instrumentation = tracks.clone();
            vec![index]
        }
    };
    let after = next.sections.len();
    Ok((
        next,
        EditOutcome {
            slots,
            before,
            after,
        },
    ))
}

/// Applies a list of edits in order, reporting what applied and what did not.
///
/// A refused edit does not stop the list. Callers acting on a person's stated
/// intent need to know which parts of it were impossible, not to lose the
/// parts that were fine.
pub fn apply_all(plan: &ArrangementPlan, edits: &[PlanEdit]) -> EditReport {
    let mut current = plan.clone();
    let mut applied = Vec::new();
    let mut rejected = Vec::new();
    for (index, edit) in edits.iter().enumerate() {
        match apply(&current, edit) {
            Ok((next, outcome)) => {
                current = next;
                applied.push(outcome);
            }
            Err(error) => rejected.push(RejectedEdit { index, error }),
        }
    }
    EditReport {
        plan: current,
        applied,
        rejected,
    }
}

/// Total output bars a plan assembles to.
pub fn planned_bars(plan: &ArrangementPlan) -> usize {
    plan.sections.iter().map(|section| section.bars).sum()
}

#[cfg(test)]
#[path = "plan_edit_test.rs"]
mod tests;
