//! Intent-based arrangement editing with isolated edit error handling.
//!
//! An arrangement intent pairs a natural-language description of an editing goal
//! with an ordered sequence of executable plan edits. Each edit is applied
//! independently: failures on individual edits are recorded in `rejected` and
//! do not abort the sequence, allowing valid subsequent edits to take effect.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::arrangement::ArrangementPlan;
use super::plan_edit::{apply, PlanEdit, SlotAddress};
use super::plan_sidecar::PlanEditRecord;

/// A caller or model intent containing a description and executable edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrangementIntent {
    /// Human-readable explanation of the editing goal.
    pub intent: String,
    /// Ordered edits to apply.
    pub edits: Vec<PlanEdit>,
    /// Optional timestamp to associate with generated history records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// An edit that successfully applied to the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedIntentEdit {
    /// Zero-based index of the edit within `ArrangementIntent::edits`.
    pub index: usize,
    /// Recorded history entry for the applied edit.
    pub edit: PlanEditRecord,
}

/// An edit that could not be applied, along with the failure reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedIntentEdit {
    /// Zero-based index of the edit within `ArrangementIntent::edits`.
    pub index: usize,
    /// The edit that failed.
    pub edit: PlanEdit,
    /// Why the edit could not be applied.
    pub reason: String,
}

/// Result of applying an arrangement intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentApplication {
    /// Resulting arrangement plan after all successful edits.
    pub plan: ArrangementPlan,
    /// Chronological edit history including previous entries and newly applied ones.
    pub edit_history: Vec<PlanEditRecord>,
    /// Successful edits from this batch.
    pub applied: Vec<AppliedIntentEdit>,
    /// Refused edits from this batch.
    pub rejected: Vec<RejectedIntentEdit>,
}

/// Errors produced during intent validation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IntentError {
    /// The intent description was empty or contained only whitespace.
    #[error("intent description must not be empty")]
    EmptyIntent,
}

fn describe_slot_address(address: &SlotAddress) -> String {
    match address {
        SlotAddress::Index(i) => format!("index:{i}"),
        SlotAddress::Label(l) => format!("label:{l}"),
        SlotAddress::Occurrence(l, o) => format!("occurrence:{l}#{o}"),
    }
}

fn edit_details(edit: &PlanEdit) -> (String, String, Option<String>) {
    match edit {
        PlanEdit::Repeat { address, times } => (
            "repeat".into(),
            describe_slot_address(address),
            Some(times.to_string()),
        ),
        PlanEdit::Drop { address } => ("drop".into(), describe_slot_address(address), None),
        PlanEdit::Insert {
            address,
            placement,
            section,
        } => (
            "insert".into(),
            describe_slot_address(address),
            Some(format!("{placement:?} {}", section.label)),
        ),
        PlanEdit::Move {
            address,
            target,
            placement,
        } => (
            "move".into(),
            describe_slot_address(address),
            Some(format!("{placement:?} {}", describe_slot_address(target))),
        ),
        PlanEdit::Rename { address, label } => (
            "rename".into(),
            describe_slot_address(address),
            Some(label.clone()),
        ),
        PlanEdit::Transpose { address, semitones } => (
            "transpose".into(),
            describe_slot_address(address),
            Some(semitones.to_string()),
        ),
        PlanEdit::Variation {
            address,
            candidates,
        } => (
            "variation".into(),
            describe_slot_address(address),
            Some(format!("{candidates:?}")),
        ),
        PlanEdit::Energy { address, energy } => (
            "energy".into(),
            describe_slot_address(address),
            energy.map(|e| e.to_string()),
        ),
        PlanEdit::Instrumentation { address, tracks } => (
            "instrumentation".into(),
            describe_slot_address(address),
            tracks.as_ref().map(|t| format!("{t:?}")),
        ),
    }
}

/// Applies an arrangement intent to an existing plan and history.
///
/// Each edit is executed in sequence. Invalid addresses or parameters are
/// captured in `rejected` while remaining valid edits apply.
pub fn apply_intent(
    original_plan: &ArrangementPlan,
    request: &ArrangementIntent,
    existing_history: &[PlanEditRecord],
) -> Result<IntentApplication, IntentError> {
    if request.intent.trim().is_empty() {
        return Err(IntentError::EmptyIntent);
    }

    let mut plan = original_plan.clone();
    let mut edit_history = existing_history.to_vec();
    let mut applied = Vec::new();
    let mut rejected = Vec::new();

    let timestamp = request
        .timestamp
        .clone()
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

    for (index, edit) in request.edits.iter().enumerate() {
        match apply(&plan, edit) {
            Ok((next_plan, _outcome)) => {
                plan = next_plan;
                let (verb, target, value) = edit_details(edit);
                let record = PlanEditRecord {
                    verb,
                    target,
                    value,
                    timestamp: timestamp.clone(),
                    intent: Some(request.intent.clone()),
                    decision_index: Some(index),
                };
                edit_history.push(record.clone());
                applied.push(AppliedIntentEdit {
                    index,
                    edit: record,
                });
            }
            Err(error) => {
                rejected.push(RejectedIntentEdit {
                    index,
                    edit: edit.clone(),
                    reason: error.to_string(),
                });
            }
        }
    }

    Ok(IntentApplication {
        plan,
        edit_history,
        applied,
        rejected,
    })
}

#[cfg(test)]
#[path = "intent_test.rs"]
mod tests;
