//! Motif segmentation, compatibility scoring, transitions, and song assembly.
//!
//! The operations in this module work on written document bars. They preserve
//! exact rhythms and report invalid timing instead of changing it.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::inspect::{bar_capacity, bar_integrity, rhythm_duration, Fraction};
use crate::model::Document;

use super::arrangement_support::{
    fit_bar_to_capacity, range_can_transpose, reassign_ids, seeded_index, splitmix64,
    transpose_bar, transpose_fingerprint, transpose_selected_bars, u32_to_bar_ref, IdAllocator,
};

/// A stable, style-neutral summary of one master bar across all tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotifFingerprint {
    /// Exact beat lengths, in whole-note fractions, in track and voice order.
    pub rhythms: Vec<Vec<(u64, u64)>>,
    /// Pitch classes in track, beat, and note order.
    pub pitch_classes: Vec<Vec<u8>>,
    /// Consecutive melodic intervals, in semitones, per track.
    pub intervals: Vec<Vec<i32>>,
    /// Number of sounding notes per track.
    pub note_counts: Vec<usize>,
    /// Track indexes that contain no sounding notes in this bar.
    pub empty_tracks: Vec<usize>,
}

/// A contiguous region with similar per-bar fingerprints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Motif {
    /// Inclusive starting master-bar index.
    pub start_bar: usize,
    /// Inclusive ending master-bar index.
    pub end_bar: usize,
    /// Combined fingerprint for the region.
    pub fingerprint: MotifFingerprint,
}

/// Component scores for two motifs; each component is in the range `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Compatibility {
    /// Similarity of exact written beat durations.
    pub rhythm: f64,
    /// Similarity of pitch-class contours.
    pub pitch: f64,
    /// Similarity of melodic interval sequences.
    pub contour: f64,
    /// Similarity of per-track note density.
    pub density: f64,
    /// Weighted aggregate score.
    pub score: f64,
}

/// Result of checking a structural join between two master bars.
#[derive(Debug, Clone, PartialEq)]
pub struct JoinReport {
    /// Whether the join is structurally valid without changing either bar.
    pub valid: bool,
    /// Compatibility score between the bars, even when the join is valid.
    pub compatibility: Compatibility,
    /// Timing or reference problems that prevent assembly.
    pub problems: Vec<String>,
}

/// One requested section in a deterministic composition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionRequest {
    /// Label written to the first bar of the section.
    pub label: String,
    /// Number of output bars. The selected motif repeats or truncates to fit.
    pub bars: usize,
    /// Candidate indexes into the motif list supplied to [`compose_song`].
    pub candidates: Vec<usize>,
    /// Semitone transposition applied to every selected bar in this section.
    #[serde(default)]
    pub transpose_semitones: i32,
    /// Written intensity from zero to one hundred, or `None` to leave the
    /// assembled dynamics as the source wrote them.
    #[serde(default)]
    pub energy: Option<u8>,
    /// Track indexes that sound in this section, or `None` for every track.
    ///
    /// Tracks outside the list are silenced across the section's bars rather
    /// than removed, so the assembled document keeps its track layout.
    #[serde(default)]
    pub instrumentation: Option<Vec<usize>>,
    /// Optional mapping from section energy to additional texture changes.
    #[serde(default)]
    pub energy_texture: Option<EnergyTexture>,
    /// Whether this is the one marked sectional climax in the plan.
    #[serde(default)]
    pub climax: bool,
}

/// Caller-supplied texture mapping driven by a section's energy value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnergyTexture {
    /// Tracks that are silent below `unmute_at` and sound at or above it.
    pub optional_tracks: Vec<usize>,
    /// Energy threshold at which optional tracks are restored.
    pub unmute_at: u8,
    /// Track to double one octave above `octave_at`, when configured.
    pub octave_track: Option<usize>,
    /// Energy threshold at which octave doubling begins.
    pub octave_at: u8,
}

impl EnergyTexture {
    /// Creates a caller-defined energy-to-texture mapping.
    pub const fn new(
        optional_tracks: Vec<usize>,
        unmute_at: u8,
        octave_track: Option<usize>,
        octave_at: u8,
    ) -> Self {
        Self {
            optional_tracks,
            unmute_at,
            octave_track,
            octave_at,
        }
    }
}

/// Dynamic written for an energy value, coarse enough to survive a round trip.
fn dynamic_for(energy: u8) -> &'static str {
    match energy {
        0..=16 => "PPP",
        17..=33 => "PP",
        34..=49 => "P",
        50..=66 => "MF",
        67..=83 => "F",
        84..=91 => "FF",
        _ => "FFF",
    }
}

/// A serializable arrangement request that can be reviewed or edited before assembly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrangementPlan {
    /// Schema version for forward-compatible plan files.
    pub version: u32,
    /// Seed used to resolve equally ranked motif candidates.
    pub seed: u64,
    /// Maximum number of source bars grouped into one motif.
    #[serde(default = "default_motif_max_bars")]
    pub motif_max_bars: usize,
    /// Minimum adjacent-bar compatibility required to keep a motif together.
    #[serde(default = "default_motif_threshold")]
    pub motif_threshold: f64,
    /// Ordered sections to assemble.
    pub sections: Vec<SectionRequest>,
    /// Transition rules applied after the sections are assembled.
    #[serde(default)]
    pub transitions: Vec<super::transitions::TransitionRule>,
}

impl PartialEq for ArrangementPlan {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
            && self.seed == other.seed
            && self.motif_max_bars == other.motif_max_bars
            && self.motif_threshold.to_bits() == other.motif_threshold.to_bits()
            && self.sections == other.sections
            && self.transitions == other.transitions
    }
}

impl Eq for ArrangementPlan {}

impl ArrangementPlan {
    /// Creates a plan with the current schema version.
    pub fn new(seed: u64, sections: Vec<SectionRequest>) -> Self {
        Self {
            version: 1,
            seed,
            motif_max_bars: default_motif_max_bars(),
            motif_threshold: default_motif_threshold(),
            sections,
            transitions: Vec::new(),
        }
    }

    /// Serializes this plan as pretty JSON.
    pub fn to_json(&self) -> Result<String, ArrangementError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| ArrangementError::Serialization(error.to_string()))
    }

    /// Parses and validates a JSON arrangement plan.
    pub fn from_json(json: &str) -> Result<Self, ArrangementError> {
        let plan: Self = serde_json::from_str(json)
            .map_err(|error| ArrangementError::Serialization(error.to_string()))?;
        if plan.version != 1 {
            return Err(ArrangementError::InvalidInput(format!(
                "unsupported arrangement plan version {}",
                plan.version
            )));
        }
        if plan.sections.is_empty() {
            return Err(ArrangementError::InvalidInput(
                "arrangement plan needs at least one section".into(),
            ));
        }
        if plan.motif_max_bars == 0 {
            return Err(ArrangementError::InvalidInput(
                "motif_max_bars must be greater than zero".into(),
            ));
        }
        if !plan.motif_threshold.is_finite() || !(0.0..=1.0).contains(&plan.motif_threshold) {
            return Err(ArrangementError::InvalidInput(
                "motif_threshold must be between 0 and 1".into(),
            ));
        }
        Ok(plan)
    }
}

fn default_motif_max_bars() -> usize {
    16
}

fn default_motif_threshold() -> f64 {
    0.25
}

/// Errors returned by segmentation and composition operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArrangementError {
    /// A document reference, range, or composition request is invalid.
    #[error("invalid arrangement input: {0}")]
    InvalidInput(String),
    /// A composed bar has an overfull voice.
    #[error(
        "bar {bar} on track {track}, voice {voice} exceeds its meter (source bar {source_bar})"
    )]
    OverfullBar {
        /// Track index containing the invalid voice.
        track: usize,
        /// Destination master-bar number (one-based) containing the invalid voice.
        bar: usize,
        /// Voice index containing the invalid timing.
        voice: usize,
        /// Source master-bar number (one-based) that supplied the invalid material.
        source_bar: usize,
    },
    /// A transposition would leave the representable pitch or fret range.
    #[error("transposition {semitones} is invalid for note {note_id}")]
    InvalidTransposition {
        /// Affected note identity.
        note_id: u32,
        /// Requested semitone offset.
        semitones: i32,
    },
    /// A plan could not be encoded or decoded.
    #[error("arrangement plan serialization failed: {0}")]
    Serialization(String),
}

/// Fingerprints the content of one master bar across its tracks.
pub fn fingerprint_bar(
    doc: &Document,
    bar_index: usize,
) -> Result<MotifFingerprint, ArrangementError> {
    let master = doc
        .master_bars
        .get(bar_index)
        .ok_or_else(|| ArrangementError::InvalidInput(format!("bar index {bar_index}")))?;
    if master.bar_ids.len() != doc.tracks.len() {
        return Err(ArrangementError::InvalidInput(format!(
            "bar {bar_index} has {} track references for {} tracks",
            master.bar_ids.len(),
            doc.tracks.len()
        )));
    }

    let mut rhythms = Vec::with_capacity(doc.tracks.len());
    let mut pitch_classes = Vec::with_capacity(doc.tracks.len());
    let mut intervals = Vec::with_capacity(doc.tracks.len());
    let mut note_counts = Vec::with_capacity(doc.tracks.len());
    let mut empty_tracks = Vec::new();

    for (track_index, bar_id) in master.bar_ids.iter().enumerate() {
        let Some(id) = u32::try_from(*bar_id).ok() else {
            if *bar_id == -1 {
                rhythms.push(Vec::new());
                pitch_classes.push(Vec::new());
                intervals.push(Vec::new());
                note_counts.push(0);
                empty_tracks.push(track_index);
                continue;
            }
            return Err(ArrangementError::InvalidInput(format!(
                "bar {bar_index} refers to missing bar id {bar_id}"
            )));
        };
        let Some(bar) = doc.bars.iter().find(|bar| bar.id == id) else {
            return Err(ArrangementError::InvalidInput(format!(
                "bar {bar_index} refers to missing bar id {bar_id}"
            )));
        };

        let mut track_rhythms = Vec::new();
        let mut track_pitches = Vec::new();
        let mut prior_pitch = None;
        let mut track_intervals = Vec::new();
        let mut count = 0;
        for voice in &bar.voices {
            for beat in &voice.beats {
                track_rhythms.push(beat.rhythm.as_fraction());
                for note in &beat.notes {
                    if let Some(midi) = note.midi {
                        let pitch_class = u8::try_from(midi.rem_euclid(12)).unwrap_or_default();
                        track_pitches.push(pitch_class);
                        if let Some(previous) = prior_pitch {
                            track_intervals.push(midi - previous);
                        }
                        prior_pitch = Some(midi);
                        count += 1;
                    }
                }
            }
        }
        if count == 0 {
            empty_tracks.push(track_index);
        }
        rhythms.push(track_rhythms);
        pitch_classes.push(track_pitches);
        intervals.push(track_intervals);
        note_counts.push(count);
    }

    Ok(MotifFingerprint {
        rhythms,
        pitch_classes,
        intervals,
        note_counts,
        empty_tracks,
    })
}

/// Segments the score into contiguous regions of similar bars.
///
/// Regions split when adjacent-bar compatibility drops below `threshold` or
/// `max_bars` is reached. Thresholds are clamped to `0..=1`.
pub fn segment_motifs(
    doc: &Document,
    max_bars: usize,
    threshold: f64,
) -> Result<Vec<Motif>, ArrangementError> {
    if max_bars == 0 {
        return Err(ArrangementError::InvalidInput(
            "max_bars must be greater than zero".into(),
        ));
    }
    let mut fingerprints = Vec::with_capacity(doc.master_bars.len());
    for index in 0..doc.master_bars.len() {
        fingerprints.push(fingerprint_bar(doc, index)?);
    }
    if fingerprints.is_empty() {
        return Ok(Vec::new());
    }

    let threshold = threshold.clamp(0.0, 1.0);
    let mut motifs = Vec::new();
    let mut start = 0;
    for index in 1..=fingerprints.len() {
        let section_boundary = index < fingerprints.len()
            && doc
                .master_bars
                .get(index)
                .and_then(|bar| bar.section.as_ref())
                .is_some();
        let split = index == fingerprints.len()
            || index - start >= max_bars
            || section_boundary
            || score_compatibility(&fingerprints[index - 1], &fingerprints[index]).score
                < threshold;
        if split {
            let end = index - 1;
            motifs.push(Motif {
                start_bar: start,
                end_bar: end,
                fingerprint: combine_fingerprints(&fingerprints[start..=end]),
            });
            start = index;
        }
    }
    Ok(motifs)
}

/// Scores how well two fingerprints fit together.
pub fn score_compatibility(left: &MotifFingerprint, right: &MotifFingerprint) -> Compatibility {
    let rhythm = nested_similarity(&left.rhythms, &right.rhythms);
    let pitch = nested_similarity(&left.pitch_classes, &right.pitch_classes);
    let contour = nested_similarity(&left.intervals, &right.intervals);
    let density = count_similarity(&left.note_counts, &right.note_counts);
    Compatibility {
        rhythm,
        pitch,
        contour,
        density,
        score: rhythm * 0.35 + pitch * 0.25 + contour * 0.25 + density * 0.15,
    }
}

/// Checks whether adjacent bars can be joined without repairing timing.
pub fn check_join(
    doc: &Document,
    left_bar: usize,
    right_bar: usize,
) -> Result<JoinReport, ArrangementError> {
    let left = fingerprint_bar(doc, left_bar)?;
    let right = fingerprint_bar(doc, right_bar)?;
    let mut problems = Vec::new();
    for item in bar_integrity(doc) {
        if (item.bar_index == left_bar || item.bar_index == right_bar)
            && item.duration > item.capacity
        {
            problems.push(format!(
                "track {} bar {} voice {} is overfull",
                item.track_index, item.bar_index, item.voice_index
            ));
        }
    }
    Ok(JoinReport {
        valid: problems.is_empty(),
        compatibility: score_compatibility(&left, &right),
        problems,
    })
}

/// Assembles requested sections from existing motif bars.
///
/// Candidate ties are resolved with `seed`; identical inputs and seed always
/// produce an identical document. Motifs repeat from their first bar to fill
/// the requested section length. Every copied rhythmic value is preserved.
pub fn compose_song(
    source: &Document,
    motifs: &[Motif],
    sections: &[SectionRequest],
    seed: u64,
) -> Result<Document, ArrangementError> {
    if sections.is_empty() {
        return Err(ArrangementError::InvalidInput(
            "composition needs at least one section".into(),
        ));
    }
    if sections.iter().filter(|section| section.climax).count() > 1 {
        return Err(ArrangementError::InvalidInput(
            "an arrangement can have only one sectional climax".into(),
        ));
    }

    let mut chosen = Vec::with_capacity(sections.len());
    let mut state = seed;
    let mut prior_fingerprint: Option<MotifFingerprint> = None;
    for section in sections {
        if section.bars == 0 || section.candidates.is_empty() {
            return Err(ArrangementError::InvalidInput(format!(
                "section {:?} must request bars and candidates",
                section.label
            )));
        }
        let mut ranked = Vec::new();
        for index in &section.candidates {
            let motif = motifs.get(*index).ok_or_else(|| {
                ArrangementError::InvalidInput(format!(
                    "motif index {index} is unavailable (available motifs: {})",
                    motifs.len()
                ))
            })?;
            validate_motif(source, motif)?;
            if !range_can_transpose(
                source,
                motif.start_bar,
                motif.end_bar,
                section.transpose_semitones,
            ) {
                continue;
            }
            let candidate_start = transpose_fingerprint(
                fingerprint_bar(source, motif.start_bar)?,
                section.transpose_semitones,
            );
            let score = prior_fingerprint.as_ref().map_or(1.0, |prior| {
                score_compatibility(prior, &candidate_start).score
            });
            ranked.push((*index, score));
        }
        if ranked.is_empty() {
            return Err(ArrangementError::InvalidInput(format!(
                "section {:?} has no candidate valid for its transposition",
                section.label
            )));
        }
        let best = ranked
            .iter()
            .map(|(_, score)| *score)
            .fold(f64::NEG_INFINITY, f64::max);
        let tied = ranked
            .iter()
            .filter(|(_, score)| (*score - best).abs() < f64::EPSILON)
            .map(|(index, _)| *index)
            .collect::<Vec<_>>();
        state = splitmix64(state);
        let choice = tied[seeded_index(state, tied.len())];
        let motif = &motifs[choice];
        let last_offset = (section.bars - 1) % (motif.end_bar - motif.start_bar + 1);
        let last_bar = motif.start_bar + last_offset;
        prior_fingerprint = Some(transpose_fingerprint(
            fingerprint_bar(source, last_bar)?,
            section.transpose_semitones,
        ));
        chosen.push(choice);
    }

    let mut output = source.clone();
    output.master_bars.clear();
    output.bars.clear();
    output.tempo_map.clear();
    output.source = None;
    output.title = if source.title.is_empty() {
        "Arrangement".into()
    } else {
        format!("{} (arrangement)", source.title)
    };

    let mut next_ids = IdAllocator::from_document(source);
    for (section, motif_index) in sections.iter().zip(chosen) {
        let motif = &motifs[motif_index];
        let motif_len = motif.end_bar - motif.start_bar + 1;
        for offset in 0..section.bars {
            let source_bar_index = motif.start_bar + offset % motif_len;
            let source_master = source.master_bars.get(source_bar_index).ok_or_else(|| {
                ArrangementError::InvalidInput(format!("source bar {source_bar_index}"))
            })?;

            let mut master = source_master.clone();
            master.index = output.master_bars.len();
            master.section = if offset == 0 {
                Some(section.label.clone())
            } else {
                None
            };
            master.repeat_start = false;
            master.repeat_end = None;
            master.alternate_ending = 0;
            master.direction = None;
            master.bar_ids.clear();
            let destination_capacity = bar_capacity(master.time);
            for (track_index, source_id) in source_master.bar_ids.iter().enumerate() {
                if *source_id < 0 {
                    master.bar_ids.push(-1);
                    continue;
                }
                let source_id = u32::try_from(*source_id).map_err(|_| {
                    ArrangementError::InvalidInput(format!("invalid bar reference {source_id}"))
                })?;
                let bar = source
                    .bars
                    .iter()
                    .find(|bar| bar.id == source_id)
                    .ok_or_else(|| {
                        ArrangementError::InvalidInput(format!("missing source bar id {source_id}"))
                    })?;
                let mut copied = bar.clone();
                fit_bar_to_capacity(&mut copied, destination_capacity);
                for (voice_index, voice) in copied.voices.iter().enumerate() {
                    let duration = voice.beats.iter().fold(Fraction::new(0, 1), |sum, beat| {
                        sum.plus(rhythm_duration(beat.rhythm))
                    });
                    if duration > destination_capacity {
                        return Err(ArrangementError::OverfullBar {
                            track: track_index,
                            bar: master.index + 1,
                            voice: voice_index,
                            source_bar: source_bar_index + 1,
                        });
                    }
                }
                reassign_ids(&mut copied, &mut next_ids)?;
                transpose_bar(&mut copied, section.transpose_semitones)?;
                master.bar_ids.push(u32_to_bar_ref(copied.id)?);
                output.bars.push(copied);
            }
            let output_bar = output.master_bars.len();
            for tempo in source
                .tempo_map
                .iter()
                .filter(|tempo| tempo.at_bar == source_bar_index)
            {
                let mut mapped = *tempo;
                mapped.at_bar = output_bar;
                output.tempo_map.push(mapped);
            }
            output.master_bars.push(master);
        }
    }
    Ok(output)
}

/// Assembles and applies a reviewed arrangement plan.
pub fn compose_plan(
    source: &Document,
    motifs: &[Motif],
    plan: &ArrangementPlan,
) -> Result<Document, ArrangementError> {
    if plan.version != 1 {
        return Err(ArrangementError::InvalidInput(format!(
            "unsupported arrangement plan version {}",
            plan.version
        )));
    }
    let mut output = compose_song(source, motifs, &plan.sections, plan.seed)?;
    apply_section_settings(&mut output, &plan.sections)?;
    super::transitions::apply_transitions(&mut output, &plan.transitions)?;
    Ok(output)
}

/// Writes each section's energy and instrumentation onto the assembled bars.
///
/// Both are plan-level decisions with no place in the motif material, so they
/// are applied after assembly rather than folded into the selection. A section
/// that sets neither is left exactly as `compose_song` wrote it.
fn apply_section_settings(
    doc: &mut Document,
    sections: &[SectionRequest],
) -> Result<(), ArrangementError> {
    let mut start = 0usize;
    for section in sections {
        let end = start + section.bars;
        if let Some(energy) = section.energy.or(section.climax.then_some(100)) {
            if energy > 100 {
                return Err(ArrangementError::InvalidInput(format!(
                    "section energy {energy} is outside 0..=100"
                )));
            }
            let dynamic = dynamic_for(energy);
            for bar_index in start..end.min(doc.master_bars.len()) {
                for track_index in 0..doc.tracks.len() {
                    let Some(id) = doc
                        .master_bars
                        .get(bar_index)
                        .and_then(|master| master.bar_ids.get(track_index))
                        .copied()
                    else {
                        continue;
                    };
                    let Some(bar) = doc
                        .bars
                        .iter_mut()
                        .find(|bar| i64::from(bar.id) == i64::from(id))
                    else {
                        continue;
                    };
                    for voice in &mut bar.voices {
                        for beat in &mut voice.beats {
                            if !beat.notes.is_empty() {
                                beat.dynamic = Some(dynamic.to_string());
                            }
                        }
                    }
                }
            }
        }
        if let Some(tracks) = &section.instrumentation {
            if let Some(bad) = tracks.iter().find(|index| **index >= doc.tracks.len()) {
                return Err(ArrangementError::InvalidInput(format!(
                    "section instrumentation names track {bad}, which does not exist"
                )));
            }
        }
        if let Some(texture) = &section.energy_texture {
            validate_energy_texture(doc, texture)?;
        }
        if section.instrumentation.is_some() || section.energy.is_some() {
            for bar_index in start..end.min(doc.master_bars.len()) {
                for track_index in 0..doc.tracks.len() {
                    let instrumentation_mutes = !section.climax
                        && section
                            .instrumentation
                            .as_ref()
                            .is_some_and(|tracks| !tracks.contains(&track_index));
                    let texture_mutes = section
                        .energy_texture
                        .as_ref()
                        .zip(section.energy)
                        .is_some_and(|(texture, energy)| {
                            energy < texture.unmute_at
                                && texture.optional_tracks.contains(&track_index)
                        });
                    if !instrumentation_mutes && !texture_mutes {
                        continue;
                    }
                    let Some(id) = doc
                        .master_bars
                        .get(bar_index)
                        .and_then(|master| master.bar_ids.get(track_index))
                        .copied()
                    else {
                        continue;
                    };
                    let Some(bar) = doc
                        .bars
                        .iter_mut()
                        .find(|bar| i64::from(bar.id) == i64::from(id))
                    else {
                        continue;
                    };
                    for voice in &mut bar.voices {
                        for beat in &mut voice.beats {
                            beat.notes.clear();
                        }
                    }
                }
            }
        }
        if let Some(texture) = &section.energy_texture {
            if let Some(energy) = section.energy {
                if energy >= texture.octave_at {
                    if let Some(track_index) = texture.octave_track {
                        add_octave_doubling(doc, start, end, track_index)?;
                    }
                }
            }
        }
        if section.climax {
            apply_climax_texture(doc, start, end);
        }
        start = end;
    }
    Ok(())
}

fn apply_climax_texture(doc: &mut Document, start: usize, end: usize) {
    for track_index in 0..doc.tracks.len() {
        if !doc.tracks[track_index].tuning.is_empty() {
            continue;
        }
        for bar_index in start..end.min(doc.master_bars.len()) {
            let Some(id) = doc.master_bars[bar_index].bar_ids.get(track_index).copied() else {
                continue;
            };
            let Ok(id) = u32::try_from(id) else { continue };
            if let Some(bar) = doc.bars.iter_mut().find(|bar| bar.id == id) {
                for voice in &mut bar.voices {
                    let mut snare = 0usize;
                    for beat in &mut voice.beats {
                        for note in &mut beat.notes {
                            if note.articulation == Some(38) {
                                snare += 1;
                                if snare % 2 == 0 {
                                    note.articulation = Some(42);
                                    note.midi = Some(42);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn validate_energy_texture(
    doc: &Document,
    texture: &EnergyTexture,
) -> Result<(), ArrangementError> {
    if texture.unmute_at > 100 || texture.octave_at > 100 {
        return Err(ArrangementError::InvalidInput(
            "energy texture thresholds must be within 0..=100".into(),
        ));
    }
    if texture
        .optional_tracks
        .iter()
        .chain(texture.octave_track.iter())
        .any(|track| *track >= doc.tracks.len())
    {
        return Err(ArrangementError::InvalidInput(
            "energy texture names a track that does not exist".into(),
        ));
    }
    Ok(())
}

fn add_octave_doubling(
    doc: &mut Document,
    start_bar: usize,
    end_bar: usize,
    track_index: usize,
) -> Result<(), ArrangementError> {
    let mut next_id = doc
        .bars
        .iter()
        .flat_map(|bar| bar.voices.iter())
        .flat_map(|voice| voice.beats.iter())
        .flat_map(|beat| beat.notes.iter())
        .map(|note| note.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    for bar_index in start_bar..end_bar.min(doc.master_bars.len()) {
        let bar_id = doc
            .master_bars
            .get(bar_index)
            .and_then(|master| master.bar_ids.get(track_index))
            .and_then(|id| u32::try_from(*id).ok())
            .ok_or_else(|| {
                ArrangementError::InvalidInput("missing texture bar reference".into())
            })?;
        let bar = doc
            .bars
            .iter_mut()
            .find(|bar| bar.id == bar_id)
            .ok_or_else(|| ArrangementError::InvalidInput("missing texture bar".into()))?;
        for voice in &mut bar.voices {
            let mut added = Vec::new();
            for beat in &voice.beats {
                for note in &beat.notes {
                    let Some(midi) = note.midi.and_then(|midi| midi.checked_add(12)) else {
                        continue;
                    };
                    let mut doubled = note.clone();
                    doubled.id = next_id;
                    next_id = next_id.saturating_add(1);
                    doubled.midi = Some(midi);
                    doubled.fret = doubled.fret.and_then(|fret| fret.checked_add(12));
                    added.push((beat.id, doubled));
                }
            }
            for (beat_id, note) in added {
                if let Some(beat) = voice.beats.iter_mut().find(|beat| beat.id == beat_id) {
                    beat.notes.push(note);
                }
            }
        }
    }
    Ok(())
}

/// Applies a deterministic octave variation to an inclusive master-bar range.
///
/// The seed selects among valid `-12`, `0`, and `+12` semitone transforms.
/// Transposition updates sounding pitches and fretted positions together. The
/// document is unchanged if the requested range is invalid.
pub fn apply_seeded_variation(
    doc: &mut Document,
    start_bar: usize,
    end_bar: usize,
    seed: u64,
) -> Result<i32, ArrangementError> {
    if start_bar > end_bar || end_bar >= doc.master_bars.len() {
        return Err(ArrangementError::InvalidInput(format!(
            "variation range {start_bar}..={end_bar} is outside the score"
        )));
    }
    let candidates = [-12, 0, 12]
        .into_iter()
        .filter(|semitones| range_can_transpose(doc, start_bar, end_bar, *semitones))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(ArrangementError::InvalidInput(
            "no valid variation transform".into(),
        ));
    }
    let nonzero = candidates
        .iter()
        .copied()
        .filter(|semitones| *semitones != 0)
        .collect::<Vec<_>>();
    let choices = if nonzero.is_empty() {
        &candidates
    } else {
        &nonzero
    };
    let semitones = choices[seeded_index(seed, choices.len())];

    transpose_selected_bars(doc, start_bar, end_bar, semitones)?;
    doc.source = None;
    Ok(semitones)
}

#[cfg(test)]
#[path = "arrangement_test.rs"]
mod tests;

fn validate_motif(doc: &Document, motif: &Motif) -> Result<(), ArrangementError> {
    if motif.start_bar > motif.end_bar || motif.end_bar >= doc.master_bars.len() {
        return Err(ArrangementError::InvalidInput(format!(
            "motif range {}..={} is outside the score",
            motif.start_bar, motif.end_bar
        )));
    }
    Ok(())
}

fn combine_fingerprints(parts: &[MotifFingerprint]) -> MotifFingerprint {
    let tracks = parts.first().map(|item| item.rhythms.len()).unwrap_or(0);
    let mut result = MotifFingerprint {
        rhythms: vec![Vec::new(); tracks],
        pitch_classes: vec![Vec::new(); tracks],
        intervals: vec![Vec::new(); tracks],
        note_counts: vec![0; tracks],
        empty_tracks: Vec::new(),
    };
    for part in parts {
        for track in 0..tracks {
            result.rhythms[track].extend(part.rhythms.get(track).into_iter().flatten().copied());
            result.pitch_classes[track]
                .extend(part.pitch_classes.get(track).into_iter().flatten().copied());
            result.intervals[track]
                .extend(part.intervals.get(track).into_iter().flatten().copied());
            result.note_counts[track] += part.note_counts.get(track).copied().unwrap_or(0);
        }
    }
    result.empty_tracks = result
        .note_counts
        .iter()
        .enumerate()
        .filter_map(|(index, count)| (*count == 0).then_some(index))
        .collect();
    result
}

fn nested_similarity<T: PartialEq>(left: &[Vec<T>], right: &[Vec<T>]) -> f64 {
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let tracks = left.len().max(right.len());
    let score = (0..tracks)
        .map(|track| match (left.get(track), right.get(track)) {
            (Some(a), Some(b)) if a.is_empty() && b.is_empty() => 1.0,
            (Some(a), Some(b)) => {
                let matches = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
                f64::from(u32::try_from(matches).unwrap_or(u32::MAX))
                    / f64::from(u32::try_from(a.len().max(b.len())).unwrap_or(u32::MAX))
            }
            _ => 0.0,
        })
        .sum::<f64>();
    score / f64::from(u32::try_from(tracks).unwrap_or(u32::MAX))
}

fn count_similarity(left: &[usize], right: &[usize]) -> f64 {
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let tracks = left.len().max(right.len());
    let similarity = (0..tracks)
        .map(|track| match (left.get(track), right.get(track)) {
            (Some(a), Some(b)) => {
                let maximum = (*a).max(*b);
                if maximum == 0 {
                    1.0
                } else {
                    1.0 - f64::from(u32::try_from(a.abs_diff(*b)).unwrap_or(u32::MAX))
                        / f64::from(u32::try_from(maximum).unwrap_or(u32::MAX))
                }
            }
            _ => 0.0,
        })
        .sum::<f64>();
    similarity / f64::from(u32::try_from(tracks).unwrap_or(u32::MAX))
}
