//! Composition helpers for assembling several native riff documents.
//!
//! Sources are ordered and key-aligned deterministically. The module keeps
//! source tracks and written bars intact; style and section choices remain
//! caller-owned.

use std::collections::BTreeMap;

use crate::model::{Document, KeySignature, TempoChange};

use super::arrangement::{fingerprint_bar, score_compatibility, Compatibility};

/// A named riff source with an optional caller-supplied energy value.
#[derive(Debug, Clone)]
pub struct RiffSource {
    /// Human-readable source label.
    pub label: String,
    /// Native riff document.
    pub document: Document,
    /// Ordering hint from zero to one hundred.
    pub energy: u8,
}

/// Options for multi-source composition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RiffCompositionConfig {
    /// Seed used to break equal ordering decisions.
    pub seed: u64,
    /// Optional target key. Without one, the first source key is used.
    pub target_key: Option<KeySignature>,
    /// Whether sources are ordered by ascending energy.
    pub order_by_energy: bool,
}

/// One source assigned to one output arrangement slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiffSlot {
    /// Index into the source slice.
    pub source: usize,
    /// Section marker written at the slot's first bar.
    pub label: String,
    /// Number of bars copied from the source, repeating its material as needed.
    pub bars: usize,
}

/// Ordered slot plan for multi-source composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotPlan {
    /// Slots in output order.
    pub slots: Vec<RiffSlot>,
    /// Maximum output bars; later slots are reported as dropped.
    pub max_bars: Option<usize>,
}

/// A range and source mapping emitted by slot composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotRange {
    /// Slot label.
    pub label: String,
    /// Source index used for the slot.
    pub source: usize,
    /// Half-open output bar range.
    pub output_bars: (usize, usize),
}

/// Output and deterministic adaptation information for slot composition.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotCompositionResult {
    /// Composed document.
    pub document: Document,
    /// Output ranges for slots that fit the budget.
    pub slot_ranges: Vec<SlotRange>,
    /// Labels of slots omitted after the bar budget was reached.
    pub dropped_slots: Vec<String>,
    /// Adaptation applied once to each source used by the plan.
    pub adaptations: Vec<SlotAdaptation>,
    /// Aggregate adaptation cost in semitones plus changed dimensions.
    pub adaptation_cost: u32,
}

/// Reconciliation details for one distinct source document.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotAdaptation {
    /// Source index.
    pub source: usize,
    /// Key transposition in semitones.
    pub key_semitones: i32,
    /// Source and target meter.
    pub meter: Option<((u32, u32), (u32, u32))>,
    /// Source and target tempo.
    pub tempo: Option<(f64, f64)>,
}

/// Errors returned by source composition.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RiffCompositionError {
    /// The source collection cannot be assembled.
    #[error("invalid riff composition input: {0}")]
    InvalidInput(String),
    /// A source could not be transposed into the requested key.
    #[error("could not transpose source {label}: {reason}")]
    Transposition {
        /// Label of the source that failed.
        label: String,
        /// Why the transposition failed.
        reason: String,
    },
}

/// Computes pairwise bar compatibility between two documents.
///
/// # Errors
///
/// Returns [`RiffCompositionError::InvalidInput`] if a bar of either document cannot be
/// fingerprinted, for example when it has a different number of track references than tracks.
pub fn compatibility_matrix(
    left: &Document,
    right: &Document,
) -> Result<Vec<Vec<Compatibility>>, RiffCompositionError> {
    let left_bars = (0..left.master_bars.len()).map(|index| fingerprint_bar(left, index));
    let right_fingerprints = (0..right.master_bars.len())
        .map(|index| fingerprint_bar(right, index))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| RiffCompositionError::InvalidInput(error.to_string()))?;
    left_bars
        .map(|left| {
            let left =
                left.map_err(|error| RiffCompositionError::InvalidInput(error.to_string()))?;
            Ok(right_fingerprints
                .iter()
                .map(|right| score_compatibility(&left, right))
                .collect())
        })
        .collect()
}

/// Matches tracks by name first, then by tuning, returning source to target indexes.
pub fn build_track_mapping(source: &Document, target: &Document) -> BTreeMap<usize, usize> {
    let mut mapping = BTreeMap::new();
    let mut used = Vec::new();
    for (source_index, source_track) in source.tracks.iter().enumerate() {
        let matching = target.tracks.iter().enumerate().find(|(index, track)| {
            !used.contains(index)
                && (track.name == source_track.name || track.tuning == source_track.tuning)
        });
        if let Some((target_index, _)) = matching {
            mapping.insert(source_index, target_index);
            used.push(target_index);
        }
    }
    mapping
}

/// Returns the most compatible key among keyed sources, falling back to C major.
pub fn central_key(sources: &[RiffSource]) -> KeySignature {
    let keyed = sources
        .iter()
        .filter_map(|source| source.document.key)
        .collect::<Vec<_>>();
    keyed
        .iter()
        .copied()
        .enumerate()
        .min_by_key(|(index, candidate)| {
            let distance = keyed
                .iter()
                .map(|key| {
                    let difference = i32::from(candidate.tonic) - i32::from(key.tonic);
                    let circular = difference.abs().min(12 - difference.abs());
                    let mode_penalty = i32::from(candidate.minor != key.minor) * 2;
                    circular + mode_penalty
                })
                .sum::<i32>();
            // In a modal tie, prefer the minor target; otherwise preserve input order.
            (distance, !candidate.minor, *index)
        })
        .map(|(_, key)| key)
        .unwrap_or(KeySignature {
            tonic: 0,
            minor: false,
        })
}

/// Orders sources by energy, preserving input order for ties and using `seed` only for API stability.
pub fn order_sources(sources: &[RiffSource], config: RiffCompositionConfig) -> Vec<usize> {
    let mut indexes = (0..sources.len()).collect::<Vec<_>>();
    if config.order_by_energy {
        indexes.sort_by_key(|index| (sources[*index].energy, *index));
    }
    indexes
}

/// Concatenates several riff documents, aligning them to one key.
///
/// # Errors
///
/// Returns [`RiffCompositionError::InvalidInput`] if `sources` is empty, the first source has no
/// tracks or bars, the sources have different track counts, or a bar reference is missing or
/// out of range. Returns [`RiffCompositionError::Transposition`] if a keyed source cannot be
/// transposed to the target key.
pub fn compose_from_riffs(
    sources: &[RiffSource],
    config: RiffCompositionConfig,
) -> Result<Document, RiffCompositionError> {
    let first = sources.first().ok_or_else(|| {
        RiffCompositionError::InvalidInput("at least one source is required".into())
    })?;
    if first.document.tracks.is_empty() || first.document.master_bars.is_empty() {
        return Err(RiffCompositionError::InvalidInput(
            "source must contain tracks and bars".into(),
        ));
    }
    let indexes = order_sources(sources, config);
    let target_key = config.target_key.unwrap_or_else(|| central_key(sources));
    let mut output = first.document.clone();
    output.master_bars.clear();
    output.bars.clear();
    output.tempo_map.clear();
    for source_index in indexes {
        let source = sources
            .get(source_index)
            .ok_or_else(|| RiffCompositionError::InvalidInput("source index".into()))?;
        if source.document.tracks.len() != first.document.tracks.len() {
            return Err(RiffCompositionError::InvalidInput(
                "all sources need the same track layout".into(),
            ));
        }
        let mut document = source.document.clone();
        if let Some(key) = document.key {
            let semitones = i32::from(target_key.tonic) - i32::from(key.tonic);
            crate::pitch::transpose(&mut document, semitones).map_err(|error| {
                RiffCompositionError::Transposition {
                    label: source.label.clone(),
                    reason: format!("{error:?}"),
                }
            })?;
        }
        let next_id = output
            .bars
            .iter()
            .map(|bar| bar.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut next = next_id;
        for mut master in document.master_bars {
            let mut ids = Vec::with_capacity(master.bar_ids.len());
            for id in master.bar_ids {
                if id < 0 {
                    ids.push(-1);
                    continue;
                }
                let source_id = u32::try_from(id)
                    .map_err(|_| RiffCompositionError::InvalidInput("negative bar id".into()))?;
                let mut bar = document
                    .bars
                    .iter()
                    .find(|bar| bar.id == source_id)
                    .cloned()
                    .ok_or_else(|| {
                        RiffCompositionError::InvalidInput("missing source bar".into())
                    })?;
                bar.id = next;
                ids.push(
                    i32::try_from(next).map_err(|_| {
                        RiffCompositionError::InvalidInput("bar id overflow".into())
                    })?,
                );
                output.bars.push(bar);
                next = next.saturating_add(1);
            }
            master.index = output.master_bars.len();
            master.bar_ids = ids;
            output.master_bars.push(master);
        }
    }
    output.key = Some(target_key);
    output.source = None;
    Ok(output)
}

/// Recommends an energy-ordered slot plan for a caller-selected song form.
///
/// # Errors
///
/// Returns [`RiffCompositionError::InvalidInput`] if `sources` is empty or
/// `bars_per_section` is zero.
pub fn recommend_slot_order(
    sources: &[RiffSource],
    form: super::forms::FormTemplate,
    bars_per_section: usize,
) -> Result<SlotPlan, RiffCompositionError> {
    if sources.is_empty() || bars_per_section == 0 {
        return Err(RiffCompositionError::InvalidInput(
            "sources and section length are required".into(),
        ));
    }
    let mut indexes = (0..sources.len()).collect::<Vec<_>>();
    indexes.sort_by_key(|index| (sources[*index].energy, *index));
    let slots = form
        .labels()
        .iter()
        .enumerate()
        .map(|(position, label)| RiffSlot {
            source: indexes[position % indexes.len()],
            label: (*label).to_string(),
            bars: bars_per_section,
        })
        .collect();
    Ok(SlotPlan {
        slots,
        max_bars: None,
    })
}

/// Composes source documents into labelled slots while allowing different track layouts.
///
/// # Errors
///
/// Returns [`RiffCompositionError::InvalidInput`] if `sources` is empty, the first source has no
/// tracks or bars, a source has no fretted track or cannot be adapted to the target key, meter
/// and tempo, a slot names a source that does not exist, or a bar id overflows.
pub fn compose_from_slots(
    sources: &[RiffSource],
    plan: &SlotPlan,
    config: RiffCompositionConfig,
) -> Result<SlotCompositionResult, RiffCompositionError> {
    let first = sources.first().ok_or_else(|| {
        RiffCompositionError::InvalidInput("at least one source is required".into())
    })?;
    if first.document.tracks.is_empty() {
        return Err(RiffCompositionError::InvalidInput(
            "source needs tracks".into(),
        ));
    }
    let mut output = first.document.clone();
    output.master_bars.clear();
    output.bars.clear();
    output.tempo_map.clear();
    output.source = None;
    let target_key = config.target_key.unwrap_or_else(|| central_key(sources));
    let target_meter = first
        .document
        .master_bars
        .first()
        .map(|bar| bar.time)
        .ok_or_else(|| RiffCompositionError::InvalidInput("source has no master bars".into()))?;
    let target_tempo = tempo_at(&first.document, 0).unwrap_or(120.0);
    output.tempo_map.push(TempoChange {
        at_bar: 0,
        bpm: target_tempo,
        linear: false,
    });
    let mut adapted_sources = Vec::with_capacity(sources.len());
    let mut adaptations = Vec::new();
    for (source_index, source) in sources.iter().enumerate() {
        let (document, adaptation) =
            adapt_slot_source(source, source_index, target_key, target_meter, target_tempo)?;
        adapted_sources.push(document);
        adaptations.push(adaptation);
    }
    let mut next_bar = 1u32;
    let mut ranges = Vec::new();
    let mut dropped = Vec::new();
    for slot in &plan.slots {
        let Some(_source) = sources.get(slot.source) else {
            return Err(RiffCompositionError::InvalidInput(format!(
                "slot {} references source {} but only {} sources exist",
                slot.label,
                slot.source,
                sources.len()
            )));
        };
        let document = adapted_sources
            .get(slot.source)
            .ok_or_else(|| RiffCompositionError::InvalidInput("adapted source index".into()))?;
        let remaining = plan.max_bars.map_or(Some(slot.bars), |limit| {
            let available = limit.saturating_sub(output.master_bars.len());
            (available >= slot.bars).then_some(slot.bars)
        });
        let Some(remaining) = remaining else {
            dropped.push(slot.label.clone());
            continue;
        };
        let mapping = build_track_mapping(document, &output);
        let start = output.master_bars.len();
        for offset in 0..remaining {
            let source_bar_index = offset % document.master_bars.len().max(1);
            let source_master = document.master_bars.get(source_bar_index).ok_or_else(|| {
                RiffCompositionError::InvalidInput("source has no master bars".into())
            })?;
            let mut master = source_master.clone();
            master.index = output.master_bars.len();
            master.section = (offset == 0).then(|| slot.label.clone());
            master.bar_ids = vec![-1; output.tracks.len()];
            for (source_track, target_track) in &mapping {
                let Some(source_id) = source_master.bar_ids.get(*source_track).copied() else {
                    continue;
                };
                let Some(id) = u32::try_from(source_id).ok() else {
                    continue;
                };
                let Some(source_bar) = document.bars.iter().find(|bar| bar.id == id) else {
                    continue;
                };
                let mut copied = source_bar.clone();
                copied.id = next_bar;
                next_bar = next_bar.saturating_add(1);
                master.bar_ids[*target_track] = i32::try_from(copied.id)
                    .map_err(|_| RiffCompositionError::InvalidInput("bar id overflow".into()))?;
                output.bars.push(copied);
            }
            output.master_bars.push(master);
        }
        ranges.push(SlotRange {
            label: slot.label.clone(),
            source: slot.source,
            output_bars: (start, output.master_bars.len()),
        });
    }
    output.key = Some(target_key);
    Ok(SlotCompositionResult {
        document: output,
        slot_ranges: ranges,
        dropped_slots: dropped,
        adaptation_cost: adaptations
            .iter()
            .map(|adaptation| adaptation.key_semitones.unsigned_abs())
            .sum::<u32>()
            .saturating_add(
                u32::try_from(
                    adaptations
                        .iter()
                        .filter(|adaptation| adaptation.meter.is_some())
                        .count(),
                )
                .unwrap_or(u32::MAX),
            )
            .saturating_add(
                u32::try_from(
                    adaptations
                        .iter()
                        .filter(|adaptation| adaptation.tempo.is_some())
                        .count(),
                )
                .unwrap_or(u32::MAX),
            ),
        adaptations,
    })
}

fn adapt_slot_source(
    source: &RiffSource,
    source_index: usize,
    target_key: KeySignature,
    target_meter: (u32, u32),
    target_tempo: f64,
) -> Result<(Document, SlotAdaptation), RiffCompositionError> {
    let mut document = source.document.clone();
    let source_meter = document
        .master_bars
        .first()
        .map(|bar| bar.time)
        .ok_or_else(|| RiffCompositionError::InvalidInput("source has no master bars".into()))?;
    let source_tempo = tempo_at(&document, 0).unwrap_or(target_tempo);
    let track = document
        .tracks
        .iter()
        .position(|track| !track.tuning.is_empty())
        .ok_or_else(|| {
            RiffCompositionError::InvalidInput(format!(
                "source {} has no fretted track for exact adaptation",
                source.label
            ))
        })?;
    let plan = super::riff_adaptation::plan_riff_adaptation(
        &document,
        super::riff_adaptation::RiffAdaptationRequest {
            track,
            start_bar: 0,
            end_bar: document.master_bars.len() - 1,
            target_key,
            target_tempo: (source_tempo != target_tempo).then_some(target_tempo),
            target_meter: (source_meter != target_meter).then_some(target_meter),
            fret_window: (0, 24),
        },
    )
    .map_err(|error| RiffCompositionError::InvalidInput(format!("{}: {error}", source.label)))?;
    let key_semitones = plan.key.semitones;
    super::riff_adaptation::apply_riff_adaptation(&mut document, &plan).map_err(|error| {
        RiffCompositionError::InvalidInput(format!("{}: {error}", source.label))
    })?;
    Ok((
        document,
        SlotAdaptation {
            source: source_index,
            key_semitones,
            meter: (source_meter != target_meter).then_some((source_meter, target_meter)),
            tempo: (source_tempo != target_tempo).then_some((source_tempo, target_tempo)),
        },
    ))
}

fn tempo_at(document: &Document, bar: usize) -> Option<f64> {
    document
        .tempo_map
        .iter()
        .filter(|tempo| tempo.at_bar <= bar)
        .max_by_key(|tempo| tempo.at_bar)
        .map(|tempo| tempo.bpm)
}

/// Alias for callers that describe the operation as arranging riffs.
///
/// # Errors
///
/// Returns any error from [`compose_from_riffs`].
pub fn arrange_from_riffs(
    sources: &[RiffSource],
    config: RiffCompositionConfig,
) -> Result<Document, RiffCompositionError> {
    compose_from_riffs(sources, config)
}

#[cfg(test)]
#[path = "riff_composition_test.rs"]
mod tests;
