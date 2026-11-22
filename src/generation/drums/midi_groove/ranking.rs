//! Query scoring for indexed MIDI grooves.

use super::{MidiGroove, MidiGrooveCandidate, MidiGrooveIndex, MidiGrooveQuery};

/// Ranks indexed grooves against role, feel, genre, density, sustain and tempo.
pub fn find_best_midi_grooves(
    index: &MidiGrooveIndex,
    query: &MidiGrooveQuery,
) -> Vec<MidiGrooveCandidate> {
    let mut candidates = index
        .grooves
        .iter()
        .filter(|groove| groove.grid.meter == query.meter)
        .filter(|groove| {
            query.allow_blast
                || query.role.eq_ignore_ascii_case("blast")
                || !is_blast_groove(groove)
        })
        .filter_map(|groove| {
            let mut score = 0;
            if !query.role.is_empty() {
                if groove.role == query.role {
                    score += 40;
                } else if roles_partially_compatible(&groove.role, &query.role) {
                    score += 18;
                } else {
                    return None;
                }
            }
            if let Some(feel) = query.feel {
                score += if groove.feel == feel { 25 } else { -12 };
            }
            if let Some(genre) = &query.genre {
                score += if groove.genre == genre.to_ascii_lowercase() {
                    20
                } else {
                    -8
                };
            }
            score -=
                i32::try_from(groove.tempo_bpm.abs_diff(query.tempo_bpm) / 2).unwrap_or(i32::MAX);
            if let Some(density) = query.riff_density {
                score -= (groove_density(groove) - density).abs().mul_add(30.0, 0.0) as i32;
            }
            if let Some(sustain) = query.sustain_ratio {
                score -= (groove_sustain_ratio(groove) - sustain)
                    .abs()
                    .mul_add(20.0, 0.0) as i32;
            }
            Some(MidiGrooveCandidate {
                groove: groove.clone(),
                score,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.groove.tempo_bpm.cmp(&right.groove.tempo_bpm))
            .then_with(|| left.groove.id.cmp(&right.groove.id))
    });
    candidates
}

fn roles_partially_compatible(candidate: &str, query: &str) -> bool {
    (candidate.eq_ignore_ascii_case("half-time") && query.eq_ignore_ascii_case("breakdown"))
        || (candidate.eq_ignore_ascii_case("breakdown") && query.eq_ignore_ascii_case("half-time"))
}

fn is_blast_groove(groove: &MidiGroove) -> bool {
    groove.role.to_ascii_lowercase().contains("blast")
        || groove.id.to_ascii_lowercase().contains("blast")
}

fn groove_density(groove: &MidiGroove) -> f32 {
    let width = groove.grid.steps_per_bar().unwrap_or(1) as f32;
    let total = groove.bars.len().max(1) as f32 * width;
    groove.bars.iter().flatten().count() as f32 / total
}

fn groove_sustain_ratio(groove: &MidiGroove) -> f32 {
    let notes = groove.bars.iter().flatten().count();
    if notes == 0 {
        0.0
    } else {
        groove
            .bars
            .iter()
            .flatten()
            .filter(|hit| !hit.ghost)
            .count() as f32
            / notes as f32
    }
}
