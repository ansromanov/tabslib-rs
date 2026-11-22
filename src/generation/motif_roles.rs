//! Structural role assignment and role-based candidate motif selection.
//!
//! This module assigns primary, secondary, and tertiary roles to detected
//! musical motifs by their relative energy, and generates candidate pools biased
//! toward canonical role representatives.

use serde::{Deserialize, Serialize};

/// An identified motif associated with an energy rating.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotifEnergy {
    /// Unique motif identifier or index.
    pub id: usize,
    /// Energy measurement (e.g. 0.0 to 1.0 or note density).
    pub energy: f64,
}

impl MotifEnergy {
    /// Creates a new motif descriptor with an ID and energy value.
    pub const fn new(id: usize, energy: f64) -> Self {
        Self { id, energy }
    }
}

/// Structural roles assigned to motifs within an arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotifRole {
    /// The lowest-energy motif, typically grounding verses or exposition.
    Primary,
    /// The highest-energy motif, typically driving choruses or climaxes.
    Secondary,
    /// The median-energy motif, used for bridges or contrasting sections.
    Tertiary,
}

/// Assigned motif IDs for primary, secondary, and tertiary roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleAssignment {
    /// Motif ID assigned to the primary role.
    pub primary: usize,
    /// Motif ID assigned to the secondary role.
    pub secondary: usize,
    /// Motif ID assigned to the tertiary role.
    pub tertiary: usize,
}

/// Seeded candidate selection pool biased toward a canonical role motif.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleCandidates {
    /// Candidate motif IDs in selection order.
    pub ids: Vec<usize>,
    /// Relative selection weights aligned with `ids`.
    pub weights: Vec<u32>,
}

/// Assigns structural roles from the energy ordering of detected motifs.
///
/// If empty, returns zeroes. If a single motif is provided, all roles point to it.
/// With two motifs, the lowest energy is primary and tertiary while the highest
/// is secondary. With three or more, lowest is primary, highest is secondary,
/// and the median-energy motif is tertiary.
pub fn assign_roles(motifs: &[MotifEnergy]) -> RoleAssignment {
    if motifs.is_empty() {
        return RoleAssignment {
            primary: 0,
            secondary: 0,
            tertiary: 0,
        };
    }
    if motifs.len() == 1 {
        let only = motifs[0].id;
        return RoleAssignment {
            primary: only,
            secondary: only,
            tertiary: only,
        };
    }

    let mut by_energy = motifs.to_vec();
    by_energy.sort_by(|a, b| a.energy.total_cmp(&b.energy));

    let lowest = by_energy[0].id;
    let highest = by_energy[by_energy.len() - 1].id;

    if motifs.len() == 2 {
        return RoleAssignment {
            primary: lowest,
            secondary: highest,
            tertiary: lowest,
        };
    }

    let mid_idx = by_energy.len() / 2;
    let middle = by_energy[mid_idx].id;
    let tertiary = if middle == lowest || middle == highest {
        by_energy
            .iter()
            .find(|m| m.id != lowest && m.id != highest)
            .map(|m| m.id)
            .unwrap_or(lowest)
    } else {
        middle
    };

    RoleAssignment {
        primary: lowest,
        secondary: highest,
        tertiary,
    }
}

/// Returns candidate motif IDs and weights biased toward the canonical role motif.
///
/// Primary roles select from motifs at or below the median energy.
/// Secondary roles select from motifs at or above the median energy.
/// Tertiary roles consider all motifs. The canonical motif receives 3x weight.
pub fn candidate_motifs_for_role(
    motifs: &[MotifEnergy],
    role: MotifRole,
    canonical_id: usize,
) -> RoleCandidates {
    if motifs.len() <= 1 {
        return RoleCandidates {
            ids: vec![canonical_id],
            weights: vec![1],
        };
    }

    let mut by_energy = motifs.to_vec();
    by_energy.sort_by(|a, b| a.energy.total_cmp(&b.energy));

    let mid_idx = by_energy.len() / 2;
    let median_energy = by_energy[mid_idx].energy;

    let pool: Vec<usize> = match role {
        MotifRole::Primary => {
            let filtered: Vec<usize> = motifs
                .iter()
                .filter(|m| m.energy <= median_energy)
                .map(|m| m.id)
                .collect();
            if filtered.is_empty() {
                motifs.iter().map(|m| m.id).collect()
            } else {
                filtered
            }
        }
        MotifRole::Secondary => {
            let filtered: Vec<usize> = motifs
                .iter()
                .filter(|m| m.energy >= median_energy)
                .map(|m| m.id)
                .collect();
            if filtered.is_empty() {
                motifs.iter().map(|m| m.id).collect()
            } else {
                filtered
            }
        }
        MotifRole::Tertiary => motifs.iter().map(|m| m.id).collect(),
    };

    let weights: Vec<u32> = pool
        .iter()
        .map(|&id| if id == canonical_id { 3 } else { 1 })
        .collect();

    RoleCandidates { ids: pool, weights }
}

#[cfg(test)]
#[path = "motif_roles_test.rs"]
mod tests;
