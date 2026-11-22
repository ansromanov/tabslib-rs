//! Measurement and caller-governed improvement of generated drum patterns.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{DrumHit, DrumPattern};

/// An exact non-negative ratio used by drum assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrumRatio {
    /// Number of measured events satisfying a condition.
    pub numerator: usize,
    /// Number of eligible events.
    pub denominator: usize,
}

impl DrumRatio {
    /// Returns the ratio as a whole-number percentage, rounded down.
    pub fn percent(self) -> u8 {
        if self.denominator == 0 {
            return 0;
        }
        ((self.numerator.saturating_mul(100) / self.denominator).min(100)) as u8
    }
}

/// Measurements of kick alignment and repeated active drum bars.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrumAssessment {
    /// Fraction of source onset positions occupied by at least one kick.
    pub kick_lock_density: DrumRatio,
    /// Fraction of non-empty bars that duplicate an earlier active bar.
    pub repeated_bar_ratio: DrumRatio,
    /// Number of source onset positions occupied by at least one kick.
    pub locked_onsets: usize,
    /// Number of source onset positions available for kick alignment.
    pub source_onsets: usize,
    /// Number of active bars that duplicate an earlier active bar.
    pub repeated_bars: usize,
    /// Number of non-empty bars considered for repetition.
    pub active_bars: usize,
}

/// Caller-selected targets for deterministic drum improvement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrumImprovementConfig {
    /// General MIDI voice to use for inserted or relocated kicks.
    pub kick_voice: u8,
    /// Minimum share of source onset positions occupied by a kick.
    pub target_kick_lock_percent: u8,
    /// Maximum share of active bars allowed to duplicate an earlier bar.
    pub max_repeated_bar_percent: u8,
    /// Maximum number of assessment-and-adjustment passes.
    pub max_passes: usize,
}

/// Before-and-after evidence from an improvement run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrumImprovementReport {
    /// Assessment before the first adjustment.
    pub initial: DrumAssessment,
    /// Assessment after the last adjustment.
    pub final_assessment: DrumAssessment,
    /// Number of passes that made at least one adjustment.
    pub passes: usize,
    /// Number of kick hits inserted or relocated.
    pub changed_kicks: usize,
    /// Whether both requested targets were reached.
    pub targets_met: bool,
}

type HitSignature = (u32, u8, bool, bool, u8);

fn signatures(hits: &[DrumHit], pattern: &DrumPattern) -> Vec<Vec<HitSignature>> {
    let mut bars = vec![Vec::new(); pattern.bars.len()];
    if pattern.steps_per_bar == 0 {
        return bars;
    }
    for hit in hits {
        let bar_index = usize::try_from(hit.step / pattern.steps_per_bar).unwrap_or(usize::MAX);
        let Some(bar) = bars.get_mut(bar_index) else {
            continue;
        };
        let ornament = match hit.ornament {
            None => 0,
            Some(super::DrumOrnament::Flam) => 1,
            Some(super::DrumOrnament::Drag) => 2,
        };
        bar.push((
            hit.step % pattern.steps_per_bar,
            hit.midi,
            hit.accent,
            hit.ghost,
            ornament,
        ));
    }
    for bar in &mut bars {
        bar.sort_unstable();
    }
    bars
}

/// Measures kick alignment with source onsets and repetition across active bars.
pub fn assess(hits: &[DrumHit], pattern: &DrumPattern, kick_voice: u8) -> DrumAssessment {
    let mut source_slots = BTreeSet::new();
    for (bar_index, bar) in pattern.bars.iter().enumerate() {
        for (step, source) in bar.iter().enumerate() {
            if source.onset {
                source_slots.insert((bar_index, step));
            }
        }
    }
    let locked_slots = hits
        .iter()
        .filter(|hit| hit.midi == kick_voice && pattern.steps_per_bar > 0)
        .filter_map(|hit| {
            let bar = usize::try_from(hit.step / pattern.steps_per_bar).ok()?;
            let step = usize::try_from(hit.step % pattern.steps_per_bar).ok()?;
            source_slots.contains(&(bar, step)).then_some((bar, step))
        })
        .collect::<BTreeSet<_>>();

    let bars = signatures(hits, pattern);
    let mut seen = BTreeSet::new();
    let mut active_bars = 0;
    let mut repeated_bars = 0;
    for bar in bars.into_iter().filter(|bar| !bar.is_empty()) {
        active_bars += 1;
        if !seen.insert(bar) {
            repeated_bars += 1;
        }
    }
    DrumAssessment {
        kick_lock_density: DrumRatio {
            numerator: locked_slots.len(),
            denominator: source_slots.len(),
        },
        repeated_bar_ratio: DrumRatio {
            numerator: repeated_bars,
            denominator: active_bars,
        },
        locked_onsets: locked_slots.len(),
        source_onsets: source_slots.len(),
        repeated_bars,
        active_bars,
    }
}

fn target_count(total: usize, percent: u8) -> usize {
    total
        .saturating_mul(usize::from(percent.min(100)))
        .saturating_add(99)
        / 100
}

fn add_kicks_to_target(
    hits: &mut Vec<DrumHit>,
    pattern: &DrumPattern,
    kick_voice: u8,
    target_percent: u8,
) -> usize {
    if pattern.steps_per_bar == 0 {
        return 0;
    }
    let occupied = hits
        .iter()
        .filter(|hit| hit.midi == kick_voice)
        .map(|hit| {
            (
                hit.step / pattern.steps_per_bar,
                hit.step % pattern.steps_per_bar,
            )
        })
        .collect::<BTreeSet<_>>();
    let mut source_slots = Vec::new();
    for (bar_index, bar) in pattern.bars.iter().enumerate() {
        let bar_index = u32::try_from(bar_index).unwrap_or(u32::MAX);
        let bar_start = bar_index.saturating_mul(pattern.steps_per_bar);
        for (step, source) in bar.iter().enumerate() {
            if !source.onset {
                continue;
            }
            let Ok(local) = u32::try_from(step) else {
                continue;
            };
            if local >= pattern.steps_per_bar {
                continue;
            }
            source_slots.push((bar_index, local, bar_start.saturating_add(local)));
        }
    }
    let currently_locked = source_slots
        .iter()
        .filter(|(bar, step, _)| occupied.contains(&(*bar, *step)))
        .count();
    let desired = target_count(source_slots.len(), target_percent);
    let needed = desired.saturating_sub(currently_locked);
    let candidates = source_slots
        .iter()
        .filter(|(bar, step, _)| !occupied.contains(&(*bar, *step)))
        .take(needed)
        .map(|(_, _, absolute)| *absolute)
        .collect::<Vec<_>>();
    for step in &candidates {
        hits.push(DrumHit::new(kick_voice, *step));
    }
    candidates.len()
}

fn reduce_repetition(
    hits: &mut Vec<DrumHit>,
    pattern: &DrumPattern,
    kick_voice: u8,
    max_repeated_percent: u8,
) -> usize {
    if pattern.steps_per_bar == 0 {
        return 0;
    }
    let current = assess(hits, pattern, kick_voice);
    let allowed = current
        .active_bars
        .saturating_mul(usize::from(max_repeated_percent.min(100)))
        / 100;
    if current.repeated_bars <= allowed {
        return 0;
    }
    let bar_signatures = signatures(hits, pattern);
    let mut seen = BTreeSet::new();
    let repeated_indexes = bar_signatures
        .iter()
        .enumerate()
        .filter_map(|(index, signature)| {
            if signature.is_empty() {
                return None;
            }
            (!seen.insert(signature.clone())).then_some(index)
        })
        .collect::<Vec<_>>();
    let mut changed = 0;
    for bar_index in repeated_indexes {
        let updated = assess(hits, pattern, kick_voice);
        if updated.repeated_bars <= allowed {
            break;
        }
        let Some(source_bar) = pattern.bars.get(bar_index) else {
            continue;
        };
        let bar_start = u32::try_from(bar_index)
            .unwrap_or(u32::MAX)
            .saturating_mul(pattern.steps_per_bar);
        let existing_kicks = hits
            .iter()
            .enumerate()
            .filter(|(_, hit)| {
                hit.midi == kick_voice
                    && hit.step / pattern.steps_per_bar
                        == u32::try_from(bar_index).unwrap_or(u32::MAX)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let Some((candidate_step, _)) = source_bar.iter().enumerate().find(|(step, source)| {
            if !source.onset {
                return false;
            }
            let absolute = bar_start.saturating_add(u32::try_from(*step).unwrap_or(u32::MAX));
            !hits
                .iter()
                .any(|hit| hit.midi == kick_voice && hit.step == absolute)
        }) else {
            continue;
        };
        let new_step = bar_start.saturating_add(u32::try_from(candidate_step).unwrap_or(u32::MAX));
        if let Some(index) = existing_kicks.first().copied() {
            if let Some(hit) = hits.get_mut(index) {
                hit.step = new_step;
                changed += 1;
            }
        } else {
            hits.push(DrumHit::new(kick_voice, new_step));
            changed += 1;
        }
    }
    hits.sort_by_key(|hit| (hit.step, hit.midi));
    changed
}

fn targets_met(assessment: &DrumAssessment, config: DrumImprovementConfig) -> bool {
    (assessment.source_onsets == 0
        || assessment.locked_onsets
            >= target_count(assessment.source_onsets, config.target_kick_lock_percent))
        && assessment.repeated_bars
            <= assessment
                .active_bars
                .saturating_mul(usize::from(config.max_repeated_bar_percent.min(100)))
                / 100
}

/// Iteratively fills missing kick locks and varies repeated active bars.
///
/// The source pattern supplies the only allowed kick-lock positions. The
/// caller sets both targets; the helper does not apply genre or style rules.
pub fn improve(
    hits: &mut Vec<DrumHit>,
    pattern: &DrumPattern,
    config: DrumImprovementConfig,
) -> DrumImprovementReport {
    let initial = assess(hits, pattern, config.kick_voice);
    let mut final_assessment = initial.clone();
    let mut passes = 0;
    let mut changed_kicks = 0;
    for _ in 0..config.max_passes {
        if targets_met(&final_assessment, config) {
            break;
        }
        let added = add_kicks_to_target(
            hits,
            pattern,
            config.kick_voice,
            config.target_kick_lock_percent,
        );
        let varied = reduce_repetition(
            hits,
            pattern,
            config.kick_voice,
            config.max_repeated_bar_percent,
        );
        if added == 0 && varied == 0 {
            break;
        }
        passes += 1;
        changed_kicks += added + varied;
        final_assessment = assess(hits, pattern, config.kick_voice);
    }
    DrumImprovementReport {
        initial,
        final_assessment: final_assessment.clone(),
        passes,
        changed_kicks,
        targets_met: targets_met(&final_assessment, config),
    }
}
